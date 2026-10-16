# Parakeet as a second speech engine

A prototype, on its own branch. It adds NVIDIA's Parakeet TDT 0.6B v3 next to
whisper.cpp. The model runs on the Neural Engine through
[FluidAudio](https://github.com/FluidInference/FluidAudio) 0.17.7, and the
user's dictionary and on-screen names feed FluidAudio's CTC vocabulary boosting.

## Recommendation

**Ship it as an extra choice in the model picker, after the three items under
[Before shipping](#before-shipping). Do not make it the default yet.**

On these tests Parakeet is about as fast as Base and nearly as accurate as
Large v3 Turbo. On this M1 Pro it is 12 to 23 times faster than Large. With the
user's words it reaches 5.2% WER, against 7.4% for Base and 3.8% for Large.
Large still hears jargon better through its glossary prompt (49 of 62 terms,
against 45 for Parakeet), and it covers far more languages.

The case for shipping is a model with Base's latency and close to Large's
accuracy. The case against making it the default:

- it needs macOS 14;
- the first load after download takes a minute or more;
- every number here comes from synthetic voices, not a real microphone.

## Which route

| | FluidAudio linked into the app (C ABI or swift-bridge) | **FluidAudio in a helper program (chosen)** | sherpa-onnx from Rust |
|---|---|---|---|
| Runs on | Neural Engine | Neural Engine | CPU (ONNX Runtime; Core ML EP is partial) |
| macOS floor | **14 for the whole app.** Utter today supports 12. Linking a macOS 14 library makes the app fail to launch on 12 and 13 unless every symbol is weak-linked. | 14 for the helper only. On 12 and 13, Utter hides the model. | 12 |
| App size | +6 to 7 MB | +6.6 MB stripped (2.7 MB compressed). Utter.app is 7.3 MB today. | +25 to 30 MB for onnxruntime, plus sherpa |
| Signing, notarisation | Nothing extra | One more Mach-O in `Contents/MacOS`. Tauri's `externalBin` signs it with the app's identity and hardened runtime. Core ML needs no entitlement. | A bundled dylib to sign |
| Build | `swift build` inside `cargo build`. FluidAudio's default build links a Rust static library (NeMo text processing), which clashes with Utter's own Rust runtime. | Separate `swift build`. Cargo never sees Swift. | Pure Cargo, but a large C++ dependency |
| Latency | Best | The same, plus under 1 ms to pipe 640 KB of audio | Several times slower on CPU (not measured) |
| Failure isolation | A Swift `fatalError` or Core ML crash takes down Utter | The helper dies and Utter reports an error. Quitting frees the Neural Engine memory. | In process |
| Word boosting | FluidAudio CTC | FluidAudio CTC | Hotwords need modified beam search. Not checked for NeMo TDT. |

The macOS floor decided it. A helper keeps Utter launching on 12 and 13.

## How it works

```
pipeline.rs ──► Transcriber (transcribe.rs)
                  └─ trait Engine
                       ├─ Whisper   whisper.cpp, Metal, in process
                       └─ Parakeet  parakeet.rs ──stdin/stdout──► utter-parakeet (Swift)
                                                                    ├─ AsrManager (TDT v3, ANE)
                                                                    └─ CTC 110M word spotter
```

- **Choosing the engine.** `models::CATALOG` entries carry an `engine`. The
  `Transcriber` loads whichever engine the selected model names, under the
  same lock as before. Switching models drops the old engine, and dropping
  Parakeet closes the helper's stdin, which makes it exit.
- **Protocol.** One JSON line per request. A `transcribe` request is followed
  by the raw f32 samples. The reply is the transcript plus a list of candidate
  replacements, each with UTF-8 byte offsets. See the header of
  `src-tauri/parakeet/Sources/main.swift`.
- **Audio.** Parakeet gets the whole recording. FluidAudio runs its own
  overlapping 15 s windows, so Whisper's pause splitting is not used. Silence
  is skipped, short clips are padded as for Whisper, and `collapse_repeats` and
  `tidy` still run. Utter's "transcribe while speaking" works unchanged,
  because it calls the same `transcribe_with`.
- **Where the helper is found.** `UTTER_PARAKEET`, then next to Utter's
  executable (where Tauri's `externalBin` puts it), then, in debug builds,
  `src-tauri/parakeet/.build/release/`. Parakeet is offered in settings only on
  macOS 14+ with the helper present.
- **Download.** 41 files from two Hugging Face repositories at pinned
  revisions: the four Parakeet Core ML bundles (483 MB), and the CTC model and
  tokenizer for boosting (102 MB). They download into
  `models/parakeet-tdt-0.6b-v3.part/`, which is renamed when complete, so a
  model is never half-present. Progress is one running total across the files,
  on the existing `model-download` event, and the stop button works. The
  download then loads the model once (see the first-load issue below).

### The user's words

The same list Whisper gets as its glossary prompt (the dictionary, then up to
12 names from the screen) goes to FluidAudio as a custom vocabulary, capped at
100 terms. A separate CTC model scores each term against the audio, and
FluidAudio proposes replacements where a term sounds more likely than what
Parakeet wrote. Three changes were needed to make this usable:

1. **Keep the punctuation.** FluidAudio's own rescoring rebuilds the text from
   word timings, which drops every comma, full stop and capital. The helper
   uses `ctcTokenEvaluateCandidates` instead and returns byte ranges into
   Parakeet's own text, and Rust splices the terms in.
2. **Utter decides.** The spotter judges by sound alone and turned "whether"
   into "Whisper" six times in the 24 recordings. Rust accepts a replacement
   only when `words::sounds_alike` agrees, or when the two differ only in case
   and spacing ("hyper frames" and "HyperFrames"). That removed every harmful
   edit: 17 fixes and none broken. It costs one real fix, "GROD" to "prod".
3. **Spot in parallel.** The CTC pass needs only the audio, so it runs while
   Parakeet transcribes. That takes the extra cost of boosting from about 260 ms
   down to about 120 ms.

A term heard right but written in the wrong case ("anthropic") is then
corrected, as Whisper does through its prompt.

## Results

Twelve dictations full of jargon (Anthropic, Claude, QA, evals, prod, Groq,
HyperFrames), plus two longer ones of 18 and 22 s. Each is spoken by a different
macOS `say` voice (US, UK, Australian, Irish, Indian and South African), and
each is used twice: clean, and with fan-like noise 15 dB below the speech. That
gives 24 recordings, 217 s and 652 words. The terms given were Anthropic,
Claude, QA, evals, prod, Groq, HyperFrames, Priya, Parakeet, Whisper and Marcus.
The machine was an M1 Pro running macOS 26.1. Each model ran in its own process
on an otherwise idle machine. Times are per recording after one warm-up, and
are what Rust waits for, pipe included.

| Model | User's words | WER | Clean | Noisy | Jargon heard | Median | Slowest |
|---|---|---|---|---|---|---|---|
| base.en | no | 10.7% | 8.3% | 13.2% | 28/62 | 195 ms | 407 ms |
| base.en | yes | 7.4% | 6.1% | 8.6% | 41/62 | 215 ms | 458 ms |
| large-v3-turbo-q5_0 | no | 6.4% | 4.0% | 8.9% | 35/62 | 2,798 ms | 3,060 ms |
| large-v3-turbo-q5_0 | yes | **3.8%** | **1.5%** | **6.1%** | **49/62** | 2,859 ms | 3,812 ms |
| parakeet-tdt-0.6b-v3 | no | 7.8% | 5.8% | 9.8% | 28/62 | **122 ms** | **346 ms** |
| parakeet-tdt-0.6b-v3 | yes | 5.2% | 2.8% | 7.7% | 45/62 | 242 ms | 618 ms |

What the numbers say:

- **Speed.** Parakeet takes 105 to 180 ms for a 5 to 8 s dictation and about
  300 ms for 20 s. With boosting it takes 220 to 250 ms and 550 to 620 ms.
  Large v3 Turbo q5_0 needs about 2.8 s on this M1 Pro. That is slower than
  Superwhisper's 1.2 s on newer chips, but it is what Utter users with M1-class
  Macs get today.
- **Accuracy.** Without words, Parakeet beats Base (7.8% against 10.7%) and
  trails Large (6.4%). With words, it cuts Base's errors by nearly a third and still trails Large.
  Whisper's glossary prompt is a stronger hint than CTC rescoring: it gets
  "Groq", "evals" and "prod" more often.
- **What every model misses.** "Groq" (hosted turns into "prod coasted",
  "drug-hosted", "plot posted"), "prod" in noise, and "QA" said as a word.
  FluidAudio skips terms shorter than three letters, so "QA" is never boosted.
- **Synthetic audio.** `say` voices are clean and evenly paced. Real
  dictation has hesitations, restarts and a laptop microphone, and the ranking
  could change. No real recordings were available: Utter keeps text history,
  not audio.

## Costs and caveats

- **The first load is slow.** Core ML prepares the model for the Neural Engine
  the first time it is loaded from a given place. That took 75 to 112 s here,
  and later loads take about 1 s. The download now ends by loading the model
  once, so this happens while the progress bar is still up (124 s for download
  plus preparation, on a fast connection), not in the first dictation. But the
  preparation was lost and redone (45 to 95 s) when two copies of the model at
  different paths were loaded in turn. That only happened in testing, but it
  shows the cache cannot be counted on. Not tested: after a reboot, an OS
  update, or a low-disk purge. If it happens, the next dictation waits that
  long with the pill showing "Transcribing".
- **Disk.** 586 MB per user (483 MB model and 102 MB word spotter), about the
  same as Large.
- **Memory.** The helper's resident memory is about 39 MB. The Core ML and
  Neural Engine allocations sit outside that figure.
- **Languages.** v3 covers English and 24 European languages, and works out
  the language itself. The language setting is ignored. Large v3 Turbo covers
  about 100.
- **Build.** The release build now needs the Swift 6.2 toolchain and fetches
  FluidAudio. The first `swift build` takes about 3 minutes.

## Before shipping

1. **Real voices.** Record 15 to 20 real dictations on a laptop microphone
   (the `make.sh` layout takes any WAV with a `.txt` beside it) and rerun the
   table. Ship only if Parakeet with words stays clearly ahead of Base.
2. **A preparing state.** When loading Parakeet takes more than about 2 s,
   show "Preparing Parakeet (first use, about a minute)" in settings and the
   pill, instead of a long "Transcribing".
3. **Bundle and sign.** Build the helper in `release.sh` (`swift build -c release
   --package-path src-tauri/parakeet`, then strip it). Copy it to
   `src-tauri/binaries/utter-parakeet-aarch64-apple-darwin` and add
   `"externalBin": ["binaries/utter-parakeet"]` to `tauri.conf.json`. This was
   left out of the prototype because tauri-build fails every `cargo build`
   (tests included) when the file is missing.

Worth trying next:

- Parakeet "ultra", which FluidAudio says is more accurate than v3 on English
  at the same speed.
- v2 for English-only users.
- Passing `heard_as` mishearings from the dictionary as FluidAudio aliases.
- FluidAudio's newer text-only preflight, which skips the CTC pass when no
  word is near a term. It came after 0.17.7.

## Reproducing

```sh
swift build -c release --package-path src-tauri/parakeet
src-tauri/examples/dictations/make.sh /tmp/dictations
cd src-tauri && cargo build --release --example transcribe
UTTER_PARAKEET=parakeet/.build/release/utter-parakeet ./target/release/examples/transcribe \
  --compare /tmp/dictations --show \
  --terms "Anthropic,Claude,QA,evals,prod,Groq,HyperFrames,Priya,Parakeet,Whisper,Marcus" \
  path/to/ggml-base.en.bin path/to/ggml-large-v3-turbo-q5_0.bin path/to/parakeet-tdt-0.6b-v3
```

To run the dev app on Parakeet without touching your real settings, run the
debug binary with a scratch `HOME` whose settings name the model. Then:

```sh
UTTER_SIMULATE_DOWNLOAD=parakeet-tdt-0.6b-v3 UTTER_SIMULATE=clip.f32 ./target/debug/utter
```

This downloads and prepares the model through the app's own code, then
dictates the clip.
