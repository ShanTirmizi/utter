# Contributing

Thanks for helping. Utter is small on purpose: one person should be able to
read all of it. Changes that keep it that way are the easiest to accept.

## Set up

You need macOS, Xcode Command Line Tools, Rust, Node 20 or later with pnpm
(`corepack enable`), and CMake (`brew install cmake`).

```bash
pnpm install
pnpm tauri dev                 # the app, with live reload
pnpm dev                       # just the interface, in a browser, with a pretend backend
cd src-tauri && cargo test     # the Rust tests
```

`pnpm tauri dev` makes macOS ask for the microphone on behalf of your terminal,
not Utter. To test permissions, paste and the fn key properly, build the app
(`pnpm tauri build`) and run that.

## Testing without speaking

A debug build can run recordings from disk through the whole pipeline. Nothing
is pasted; the result goes to the clipboard and the terminal.

```bash
ffmpeg -i clip.wav -ar 16000 -ac 1 -f f32le clip.f32
UTTER_SIMULATE=clip.f32,clip.f32@rewrite pnpm tauri dev
UTTER_SIMULATE_LIVE=1 UTTER_SIMULATE=long.f32 pnpm tauri dev     # as if it arrived live
UTTER_SIMULATE_SITE=https://mail.google.com/ UTTER_SIMULATE=clip.f32 pnpm tauri dev
```

The pretend backend for `pnpm dev` has switches for most states; they are
listed at the top of `src/dev-mock.ts`.

## How the code is written

- One file per stage of the pipeline; `pipeline.rs` runs them in order. The
  README has the map, and `docs/DESIGN.md` explains the decisions.
- Comments say why, in plain words. Error messages are written for the person
  using the app, not for a developer.
- Anything typed into another app must be safe to type: a model's reply is
  checked before it is used, and on any doubt the raw transcript goes in.
- Never log or store dictated text, keys or anything read from other apps,
  except the history the user can see and clear.

## Before a pull request

- `pnpm exec tsc --noEmit` and `cargo test` pass (CI runs both).
- Say how you tried the change in the app, or why you couldn't.
- Keep the diff to one thing.

## Ideas and bugs

Open an issue with the templates. For anything security-related, see
[SECURITY.md](SECURITY.md).

## A note on other apps

Utter is inspired by other dictation tools but contains none of their code,
prompts or assets. Please keep it that way: describe behaviour in your own
words and write your own implementation.
