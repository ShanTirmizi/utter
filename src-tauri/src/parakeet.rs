//! NVIDIA's Parakeet TDT v3 speech model, run on the Neural Engine by
//! FluidAudio. FluidAudio is Swift and needs macOS 14, so it lives in a small
//! helper program, `utter-parakeet` (src-tauri/parakeet), which is started
//! once and kept running. Audio goes to it over a pipe and text comes back.
//! See docs/parakeet.md for why it is a helper and not a linked library.

use std::{
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::mpsc::{self, Receiver, RecvTimeoutError},
    time::{Duration, Instant},
};

use serde_json::{Value, json};

/// The helper's file name, next to Utter's own executable in the app bundle.
const BINARY: &str = "utter-parakeet";

/// Where the helper is: beside the app, or, in a development build, named by
/// `UTTER_PARAKEET` or where `swift build` leaves it. A release build does not
/// look at `UTTER_PARAKEET`: anything that can set Utter's environment could
/// otherwise have it run another program and send it every dictation.
pub fn binary() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if cfg!(debug_assertions)
        && let Some(path) = std::env::var_os("UTTER_PARAKEET")
    {
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

/// How long the helper may take to load the model. The first load from a new
/// folder has Core ML prepare the model for this Mac's Neural Engine, which
/// has taken up to two minutes; after that a load takes about a second.
const LOAD_LIMIT: Duration = Duration::from_secs(5 * 60);

/// How long a transcription may take on top of the length of the audio. The
/// helper does a minute of speech in a second or so, so a reply this late
/// means it is stuck.
const TRANSCRIBE_LIMIT: Duration = Duration::from_secs(30);

/// How long a helper asked to stop gets to exit before it is killed.
const EXIT_LIMIT: Duration = Duration::from_secs(2);

/// A running helper with a model loaded. Dropping it closes the helper's
/// input, which makes it exit, and waits for that.
pub struct Parakeet {
    child: Child,
    /// Always there until the helper is stopped.
    input: Option<ChildStdin>,
    /// The helper's replies, a line each. They are read on a thread of their
    /// own so that a reply can be waited for with a time limit. The channel
    /// closes when the helper exits.
    replies: Receiver<String>,
    /// The helper and the model folder, for starting again after a crash.
    binary: PathBuf,
    folder: PathBuf,
    /// How long a transcription may take on top of the audio's length.
    patience: Duration,
    /// Set once the helper has crashed, hung or got out of step. It is never
    /// asked anything again.
    broken: Option<Broken>,
}

/// Why a helper can no longer be used.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Broken {
    /// It exited, or said something that could not be read.
    Stopped,
    /// It took far too long to answer, and was killed.
    Hung,
}

impl Parakeet {
    /// Start the helper and load the model folder at `path`.
    pub fn start(path: &Path) -> Result<Self, String> {
        let binary = binary().ok_or("The Parakeet engine is not installed with this copy of Utter.")?;
        Self::launch(&binary, path, LOAD_LIMIT)
    }

    /// Start the helper at `binary` and give it `limit` to load `folder`.
    fn launch(binary: &Path, folder: &Path, limit: Duration) -> Result<Self, String> {
        let mut child = Command::new(binary)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .map_err(|e| format!("Could not start the Parakeet engine: {e}"))?;
        let (input, output) = match (child.stdin.take(), child.stdout.take()) {
            (Some(input), Some(output)) => (input, output),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("No pipe to the Parakeet engine.".into());
            }
        };
        let (send, replies) = mpsc::channel();
        std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if send.send(line).is_err() {
                    break;
                }
            }
        });
        let mut engine = Self {
            child,
            input: Some(input),
            replies,
            binary: binary.to_path_buf(),
            folder: folder.to_path_buf(),
            patience: TRANSCRIBE_LIMIT,
            broken: None,
        };
        let path = folder.to_str().ok_or("Model path is not valid UTF-8.")?;
        engine.ask(&json!({ "load": path }), &[], limit)?;
        Ok(engine)
    }

    /// Mono 16 kHz audio to text. `terms` are the user's words and names on
    /// screen; where the model wrote something close to one of them and the
    /// audio supports the term, the term replaces it.
    ///
    /// A helper that crashed is started again, once, for the same audio, so
    /// the dictation is not lost. One that hung is not: it has already kept
    /// the user waiting long enough.
    pub fn transcribe(&mut self, samples: &[f32], terms: &[String]) -> Result<String, String> {
        let reply = match self.ask_to_transcribe(samples, terms) {
            Err(e) if self.broken == Some(Broken::Stopped) => {
                eprintln!("[parakeet] {e} Starting it again.");
                let mut fresh = Self::launch(&self.binary, &self.folder, LOAD_LIMIT)?;
                fresh.patience = self.patience;
                *self = fresh;
                self.ask_to_transcribe(samples, terms)?
            }
            reply => reply?,
        };
        let text = reply["text"].as_str().unwrap_or_default();
        let candidates: Vec<Candidate> =
            serde_json::from_value(reply["candidates"].clone()).unwrap_or_default();
        Ok(apply(text, &candidates))
    }

    fn ask_to_transcribe(&mut self, samples: &[f32], terms: &[String]) -> Result<Value, String> {
        let audio = Duration::from_secs_f64(samples.len() as f64 / f64::from(crate::audio::TARGET_RATE));
        let limit = self.patience + audio;
        self.ask(&json!({ "transcribe": samples.len(), "terms": terms }), samples, limit)
    }

    /// Whether the helper can still be asked anything. Once it cannot, the
    /// caller lets it go and starts another next time.
    pub fn alive(&mut self) -> bool {
        if self.broken.is_none() && !matches!(self.child.try_wait(), Ok(None)) {
            self.stop(Broken::Stopped);
        }
        self.broken.is_none()
    }

    /// Send one request, and the audio that goes with it, and wait up to
    /// `limit` for the reply. If the helper exits, hangs or answers with
    /// something unreadable, it is stopped for good: a late reply left in the
    /// pipe would otherwise be taken as the answer to the next request.
    fn ask(&mut self, request: &Value, samples: &[f32], limit: Duration) -> Result<Value, String> {
        if self.broken.is_some() {
            return Err("The Parakeet engine stopped.".into());
        }
        let mut message = request.to_string().into_bytes();
        message.push(b'\n');
        message.reserve(samples.len() * 4);
        for sample in samples {
            message.extend_from_slice(&sample.to_le_bytes());
        }
        let Self { child, input, replies, .. } = self;
        let input = input.as_mut().ok_or("The Parakeet engine was stopped.")?;
        // The audio is written on a thread of its own: a helper that has
        // stopped reading would block the write for good once the pipe is
        // full. Killing the helper ends a blocked write.
        let (reply, written) = std::thread::scope(|scope| {
            let deadline = Instant::now() + limit;
            let writer = scope.spawn(move || input.write_all(&message).and_then(|()| input.flush()));
            let reply = replies.recv_timeout(limit);
            // The helper reads all the audio before it answers. One that
            // answered first and then stopped reading would leave the write
            // blocked, so it gets until the deadline and is then killed.
            while reply.is_ok() && !writer.is_finished() && Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            if reply.is_err() || !writer.is_finished() {
                let _ = child.kill();
            }
            let written = writer.join().unwrap_or_else(|_| Err(std::io::Error::other("writer panicked")));
            (reply, written)
        });

        let line = match (reply, written) {
            (Ok(line), Ok(())) => line,
            (Err(RecvTimeoutError::Timeout), _) => {
                self.stop(Broken::Hung);
                return Err(format!(
                    "The Parakeet engine did not answer within {} seconds, so it was stopped.",
                    limit.as_secs()
                ));
            }
            (_, Err(e)) => {
                self.stop(Broken::Stopped);
                return Err(format!("The Parakeet engine stopped: {e}."));
            }
            (Err(RecvTimeoutError::Disconnected), Ok(())) => {
                self.stop(Broken::Stopped);
                return Err("The Parakeet engine stopped unexpectedly.".into());
            }
        };
        let reply: Value = match serde_json::from_str(&line) {
            Ok(reply) => reply,
            Err(e) => {
                self.stop(Broken::Stopped);
                return Err(format!("Unreadable reply from Parakeet: {e}"));
            }
        };
        match reply["error"].as_str() {
            Some(error) => Err(error.to_string()),
            None => Ok(reply),
        }
    }

    /// Kill the helper and wait for it, so it does not linger as a zombie.
    fn stop(&mut self, why: Broken) {
        self.broken = Some(why);
        drop(self.input.take());
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Parakeet {
    fn drop(&mut self) {
        // Closing its input ends the helper's read loop. One that has not
        // exited a little later is stuck, and is killed.
        drop(self.input.take());
        let deadline = Instant::now() + EXIT_LIMIT;
        while Instant::now() < deadline {
            if !matches!(self.child.try_wait(), Ok(None)) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = self.child.kill();
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
    /// SHA-256 of the files Hugging Face keeps in Git LFS, which are all the
    /// `.bin` files, as it publishes them for this revision. The others are
    /// small text files with no published SHA-256, checked only by size.
    pub sha256: &'static [(&'static str, &'static str)],
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
        sha256: &[
            ("Preprocessor.mlmodelc/coremldata.bin", "dbde3f2300842c1fd51ef3ff948a0bcffe65ffd2dca10707f2509f32c1d65b1d"),
            ("Preprocessor.mlmodelc/analytics/coremldata.bin", "c9beeb989c8d66f8be11df59bc6df277ec76cee404f6865b46243835ef562f6d"),
            ("Preprocessor.mlmodelc/weights/weight.bin", "129b76e3aeafa8afa3ea76d995b964b145fe83700d579f6ff42c4c38fa0968ea"),
            ("Encoder.mlmodelc/coremldata.bin", "d48034a167a82e88fc3df64f60af963ab3983538271175b8319e7d5720a0fb86"),
            ("Encoder.mlmodelc/analytics/coremldata.bin", "42e638870d73f26b332918a3496ce36793fbb413a81cbd3d16ba01328637a105"),
            ("Encoder.mlmodelc/weights/weight.bin", "e2020f323703477a5b21d7c2d282c403e371afb5962e79877e3033e73ba6f421"),
            ("Decoder.mlmodelc/coremldata.bin", "18647af085d87bd8f3121c8a9b4d4564c1ede038dab63d295b4e745cf2d7fb99"),
            ("Decoder.mlmodelc/analytics/coremldata.bin", "4238c4e81ecd0dc94bd7dfbb60f7e2cc824107c1ffe0387b8607b72833dba350"),
            ("Decoder.mlmodelc/weights/weight.bin", "48adf0f0d47c406c8253d4f7fef967436a39da14f5a65e66d5a4b407be355d41"),
            ("JointDecisionv3.mlmodelc/coremldata.bin", "f5fc08b741400f0088492c9e839418b1e18522f19cba28d361dd030c5f398342"),
            ("JointDecisionv3.mlmodelc/analytics/coremldata.bin", "26def4bf73dd56d29dee21c8ef97cb8969e62f6120ed1adc91e46828e2737b6c"),
            ("JointDecisionv3.mlmodelc/weights/weight.bin", "4e0e63d840032f7f07ddb1d64446051166281e5491bf22da8a945c41f6eedb3e"),
        ],
    },
    Source {
        repo: "FluidInference/parakeet-ctc-110m-coreml",
        revision: "accdafd8cf8a2ff1cabe3c11e54416b405d409aa",
        bundles: &["MelSpectrogram.mlmodelc", "AudioEncoder.mlmodelc"],
        files: &["vocab.json", "tokenizer.json", "tokenizer_config.json", "special_tokens_map.json"],
        into: "ctc",
        sha256: &[
            ("MelSpectrogram.mlmodelc/coremldata.bin", "3a32ec67c76aa0aa2faef518413c311493e89aeb7fa11289fa4b8653ab8a160c"),
            ("MelSpectrogram.mlmodelc/analytics/coremldata.bin", "22f2a8cba1de25c984050566b534a1d8caf22a82f9fe6c1c6f3149a0dd7e8ae3"),
            ("MelSpectrogram.mlmodelc/weights/weight.bin", "0a89c055bfde9022029d3cc59a23e949385e063974460d8eaec3a7614c3eaaa8"),
            ("AudioEncoder.mlmodelc/coremldata.bin", "a88b002b58193b4c31211754cdfdf220a85f9651dc61caf336ab84400cbc191a"),
            ("AudioEncoder.mlmodelc/analytics/coremldata.bin", "8906c823e9bb3bf6b16d9f0308f98cd70573526333ad85dd767dc3f9ae6b25fa"),
            ("AudioEncoder.mlmodelc/weights/weight.bin", "af0734b4a5d7465ad9e8bb170f0c53c5e6b91ebb75a9bdf88d3f59ae4ad6aebd"),
        ],
    },
];

/// Everything in `SOURCES` together, in bytes.
pub const TOTAL_BYTES: u64 = 585_909_011;

/// Each file to download: its address, where it goes in the model folder,
/// and its SHA-256 where one is published.
pub fn files() -> Vec<(String, PathBuf, Option<&'static str>)> {
    let mut out = Vec::new();
    for source in &SOURCES {
        let paths = source
            .bundles
            .iter()
            .flat_map(|bundle| BUNDLE_FILES.iter().map(move |file| format!("{bundle}/{file}")))
            .chain(source.files.iter().map(|file| file.to_string()));
        for path in paths {
            let url = format!("https://huggingface.co/{}/resolve/{}/{path}", source.repo, source.revision);
            let sha256 = source.sha256.iter().find(|(file, _)| *file == path).map(|(_, hash)| *hash);
            out.push((url, Path::new(source.into).join(path), sha256));
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
        assert!(files.iter().any(|(url, to, _)| url.ends_with("Encoder.mlmodelc/weights/weight.bin")
            && to == Path::new("Encoder.mlmodelc/weights/weight.bin")));
        assert!(files.iter().any(|(_, to, _)| to == Path::new("ctc/tokenizer.json")));
    }

    #[test]
    fn every_binary_file_is_checked_against_its_published_hash() {
        let files = files();
        for (_, to, sha256) in &files {
            let binary = to.extension().is_some_and(|e| e == "bin");
            assert_eq!(sha256.is_some(), binary, "{}", to.display());
            assert!(sha256.is_none_or(|h| h.len() == 64 && h.bytes().all(|b| b.is_ascii_hexdigit())));
        }
        // Every pinned hash names a file that is downloaded.
        let pinned: usize = SOURCES.iter().map(|s| s.sha256.len()).sum();
        assert_eq!(pinned, files.iter().filter(|(_, _, h)| h.is_some()).count());
    }

    /// A stand-in for the helper: a shell script, alone in a fresh folder.
    fn helper(name: &str, script: &str) -> PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("utter-parakeet-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("helper");
        std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    /// Answers the load, so the tests reach a transcription.
    const LOADS: &str = r#"read request; echo '{"ok":true}'"#;

    fn launch(binary: &Path) -> Result<Parakeet, String> {
        Parakeet::launch(binary, Path::new("/nowhere"), Duration::from_secs(10))
    }

    #[test]
    fn a_helper_that_exits_or_mumbles_at_load_is_an_error() {
        let gone = launch(&helper("exits", "exit 1")).err().unwrap();
        assert!(gone.contains("stopped"), "{gone}");
        let garbled = launch(&helper("garbled", "read request; echo garbage")).err().unwrap();
        assert!(garbled.contains("Unreadable"), "{garbled}");
    }

    #[test]
    fn a_crashed_helper_is_started_again_for_the_same_audio() {
        // Crashes on its first transcription, and answers on the next start.
        let script = format!(
            r#"{LOADS}
read request
if [ -e "$0.ran" ]; then echo '{{"text":"Second try.","candidates":[]}}'; else touch "$0.ran"; exit 1; fi
read request"#
        );
        let mut engine = launch(&helper("crash-once", &script)).unwrap();
        assert_eq!(engine.transcribe(&[0.0; 1600], &[]).unwrap(), "Second try.");
        assert!(engine.alive());
    }

    #[test]
    fn a_helper_that_keeps_crashing_is_let_go() {
        let mut engine = launch(&helper("crashes", &format!("{LOADS}\nread request; exit 1"))).unwrap();
        assert!(engine.alive());
        assert!(engine.transcribe(&[0.0; 1600], &[]).is_err());
        assert!(!engine.alive());
        // Never asked again, and reaped rather than left a zombie.
        assert!(engine.transcribe(&[0.0; 1600], &[]).is_err());
        assert!(matches!(engine.child.try_wait(), Ok(Some(_))));
    }

    #[test]
    fn a_hung_helper_is_killed_and_let_go() {
        let mut engine = launch(&helper("hangs", &format!("{LOADS}\nread request; exec sleep 60"))).unwrap();
        engine.patience = Duration::from_millis(300);
        let started = Instant::now();
        let error = engine.transcribe(&[0.0; 1600], &[]).err().unwrap();
        assert!(error.contains("did not answer"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(5));
        assert!(!engine.alive());
        assert!(matches!(engine.child.try_wait(), Ok(Some(_))));

        let started = Instant::now();
        let slow = Parakeet::launch(&helper("hangs-loading", "exec sleep 60"), Path::new("/nowhere"), Duration::from_millis(300));
        assert!(slow.is_err());
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn an_error_from_a_working_helper_leaves_it_running() {
        let script = format!(r#"{LOADS}
read request; echo '{{"error":"No speech model is loaded."}}'
read request"#);
        let mut engine = launch(&helper("refuses", &script)).unwrap();
        assert_eq!(engine.transcribe(&[], &[]).err().unwrap(), "No speech model is loaded.");
        assert!(engine.alive());
    }
}
