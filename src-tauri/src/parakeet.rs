//! NVIDIA's Parakeet TDT v3 speech model, run on the Neural Engine by
//! FluidAudio. FluidAudio is Swift and needs macOS 14, so it lives in a small
//! helper program, `utter-parakeet` (src-tauri/parakeet), which is started
//! once and kept running. Audio goes to it over a pipe and text comes back.
//! See docs/parakeet.md for why it is a helper and not a linked library.

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
};

use serde_json::{Value, json};

/// The helper's file name, next to Utter's own executable in the app bundle.
const BINARY: &str = "utter-parakeet";

/// Where the helper is: named by `UTTER_PARAKEET`, beside the app, or, in a
/// development build, where `swift build` leaves it.
pub fn binary() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("UTTER_PARAKEET") {
        candidates.push(PathBuf::from(path));
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join(BINARY));
    }
    if cfg!(debug_assertions) {
        candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("parakeet/.build/release").join(BINARY));
    }
    candidates.into_iter().find(|path| path.is_file())
}

/// Parakeet can run here: macOS 14 or later, with the helper present.
pub fn available() -> bool {
    use objc2_foundation::{NSOperatingSystemVersion, NSProcessInfo};
    let sonoma = NSOperatingSystemVersion { majorVersion: 14, minorVersion: 0, patchVersion: 0 };
    NSProcessInfo::processInfo().isOperatingSystemAtLeastVersion(sonoma) && binary().is_some()
}

/// A running helper with a model loaded. Dropping it closes the helper's
/// input, which makes it exit, and waits for that.
pub struct Parakeet {
    child: Child,
    /// Always there until the helper is stopped.
    input: Option<ChildStdin>,
    output: BufReader<ChildStdout>,
}

impl Parakeet {
    /// Start the helper and load the model folder at `path`.
    pub fn start(path: &Path) -> Result<Self, String> {
        let binary = binary().ok_or("The Parakeet engine is not installed with this copy of Utter.")?;
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Could not start the Parakeet engine: {e}"))?;
        let input = child.stdin.take().ok_or("No pipe to the Parakeet engine.")?;
        let output = BufReader::new(child.stdout.take().ok_or("No pipe from the Parakeet engine.")?);
        let mut engine = Self { child, input: Some(input), output };
        let path = path.to_str().ok_or("Model path is not valid UTF-8.")?;
        engine.ask(&json!({ "load": path }), &[])?;
        Ok(engine)
    }

    /// Mono 16 kHz audio to text. `terms` are the user's words and names on
    /// screen; where the model wrote something close to one of them and the
    /// audio supports the term, the term replaces it.
    pub fn transcribe(&mut self, samples: &[f32], terms: &[String]) -> Result<String, String> {
        let reply = self.ask(&json!({ "transcribe": samples.len(), "terms": terms }), samples)?;
        let text = reply["text"].as_str().unwrap_or_default();
        let candidates: Vec<Candidate> =
            serde_json::from_value(reply["candidates"].clone()).unwrap_or_default();
        Ok(apply(text, &candidates))
    }

    /// Send one request, and the audio that goes with it, and read the reply.
    fn ask(&mut self, request: &Value, samples: &[f32]) -> Result<Value, String> {
        let gone = |e: std::io::Error| format!("The Parakeet engine stopped: {e}");
        let mut message = request.to_string().into_bytes();
        message.push(b'\n');
        message.reserve(samples.len() * 4);
        for sample in samples {
            message.extend_from_slice(&sample.to_le_bytes());
        }
        let input = self.input.as_mut().ok_or("The Parakeet engine was stopped.")?;
        input.write_all(&message).map_err(gone)?;
        input.flush().map_err(gone)?;

        let mut line = String::new();
        if self.output.read_line(&mut line).map_err(gone)? == 0 {
            return Err("The Parakeet engine stopped unexpectedly.".into());
        }
        let reply: Value = serde_json::from_str(&line).map_err(|e| format!("Unreadable reply from Parakeet: {e}"))?;
        match reply["error"].as_str() {
            Some(error) => Err(error.to_string()),
            None => Ok(reply),
        }
    }
}

impl Drop for Parakeet {
    fn drop(&mut self) {
        // Closing its input ends the helper's read loop.
        drop(self.input.take());
        let _ = self.child.wait();
    }
}

/// A place where the helper's word spotter heard one of the user's terms
/// more clearly than what the model wrote.
#[derive(Debug, serde::Deserialize)]
struct Candidate {
    /// What the model wrote there.
    from: String,
    /// The user's term.
    to: String,
    /// Byte offsets of `from` in the transcript.
    start: usize,
    end: usize,
}

/// Put the user's terms into the transcript where they were misheard. The
/// spotter judges by sound alone and will turn an ordinary word into a term
/// that sounds a little like it ("whether" into "Whisper"), so a replacement
/// is only made where Utter's own test for a mishearing agrees, or where the
/// two differ only in capitals and spacing ("hyper frames", "HyperFrames").
fn apply(text: &str, candidates: &[Candidate]) -> String {
    let squash = |s: &str| -> String { s.chars().filter(|c| c.is_alphanumeric()).collect::<String>().to_lowercase() };
    let mut accepted: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| c.start <= c.end && c.end <= text.len())
        .filter(|c| text.is_char_boundary(c.start) && text.is_char_boundary(c.end))
        .filter(|c| squash(&c.from) == squash(&c.to) || crate::words::sounds_alike(&squash(&c.from), &c.to))
        .collect();
    // From the end backwards, so earlier offsets still hold.
    accepted.sort_by_key(|c| std::cmp::Reverse(c.start));
    let mut out = text.to_string();
    let mut limit = text.len();
    for candidate in accepted {
        if candidate.end > limit {
            continue; // overlaps one already made
        }
        #[cfg(debug_assertions)]
        eprintln!("[parakeet] {} -> {}", candidate.from, candidate.to);
        out.replace_range(candidate.start..candidate.end, &candidate.to);
        limit = candidate.start;
    }
    out
}

/// One Hugging Face repository, at a fixed revision, and the files Utter
/// takes from it.
pub struct Source {
    pub repo: &'static str,
    pub revision: &'static str,
    /// Core ML bundles, each a folder of `BUNDLE_FILES`.
    pub bundles: &'static [&'static str],
    pub files: &'static [&'static str],
    /// Subfolder of the model folder they are saved in.
    pub into: &'static str,
}

/// What every compiled Core ML bundle in these repositories holds.
const BUNDLE_FILES: [&str; 5] =
    ["coremldata.bin", "metadata.json", "model.mil", "analytics/coremldata.bin", "weights/weight.bin"];

/// Parakeet TDT 0.6B v3, and the small CTC model that spots the user's words.
pub const SOURCES: [Source; 2] = [
    Source {
        repo: "FluidInference/parakeet-tdt-0.6b-v3-coreml",
        revision: "7dd20fe6b1797d35f5e3307e8b1732d9a178edfe",
        bundles: &["Preprocessor.mlmodelc", "Encoder.mlmodelc", "Decoder.mlmodelc", "JointDecisionv3.mlmodelc"],
        files: &["parakeet_vocab.json"],
        into: "",
    },
    Source {
        repo: "FluidInference/parakeet-ctc-110m-coreml",
        revision: "accdafd8cf8a2ff1cabe3c11e54416b405d409aa",
        bundles: &["MelSpectrogram.mlmodelc", "AudioEncoder.mlmodelc"],
        files: &["vocab.json", "tokenizer.json", "tokenizer_config.json", "special_tokens_map.json"],
        into: "ctc",
    },
];

/// Everything in `SOURCES` together, in bytes.
pub const TOTAL_BYTES: u64 = 585_909_011;

/// Each file to download: its address and where it goes in the model folder.
pub fn files() -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    for source in &SOURCES {
        let paths = source
            .bundles
            .iter()
            .flat_map(|bundle| BUNDLE_FILES.iter().map(move |file| format!("{bundle}/{file}")))
            .chain(source.files.iter().map(|file| file.to_string()));
        for path in paths {
            let url = format!("https://huggingface.co/{}/resolve/{}/{path}", source.repo, source.revision);
            out.push((url, Path::new(source.into).join(path)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(text: &str, from: &str, to: &str) -> Candidate {
        let start = text.find(from).unwrap();
        Candidate { from: from.into(), to: to.into(), start, end: start + from.len() }
    }

    #[test]
    fn mishearings_of_the_users_words_are_put_right() {
        let text = "Ask Clode about the evils in the hyper frames export.";
        let found = [
            candidate(text, "Clode", "Claude"),
            candidate(text, "evils", "evals"),
            candidate(text, "hyper frames", "HyperFrames"),
        ];
        assert_eq!(apply(text, &found), "Ask Claude about the evals in the HyperFrames export.");
    }

    #[test]
    fn an_ordinary_word_is_not_turned_into_a_term() {
        let text = "Check whether it shipped.";
        let found = [candidate(text, "whether", "Whisper"), candidate(text, "shipped", "Whisper")];
        assert_eq!(apply(text, &found), text);
    }

    #[test]
    fn bad_offsets_and_overlaps_are_ignored() {
        let text = "Ping Grok now.";
        let mut far = candidate(text, "Grok", "Groq");
        far.end = 99;
        assert_eq!(apply(text, &[far]), text);
        let both = [candidate(text, "Grok", "Groq"), candidate(text, "Ping Grok", "PingGroq")];
        assert_eq!(apply(text, &both), "Ping Groq now.");
    }

    #[test]
    fn every_bundle_file_is_downloaded_to_its_place() {
        let files = files();
        assert_eq!(files.len(), 6 * BUNDLE_FILES.len() + 5);
        assert!(files.iter().any(|(url, to)| url.ends_with("Encoder.mlmodelc/weights/weight.bin")
            && to == Path::new("Encoder.mlmodelc/weights/weight.bin")));
        assert!(files.iter().any(|(_, to)| to == Path::new("ctc/tokenizer.json")));
    }
}
