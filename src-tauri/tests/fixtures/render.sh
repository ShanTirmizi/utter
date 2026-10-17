#!/bin/sh
# Render the spoken test clips with macOS's own voices: 16 kHz mono 16-bit
# WAV, the format Utter records in. The clips are committed, so tests do not
# depend on which voices a machine has; run this to change or add one.
#
#   sh src-tauri/tests/fixtures/render.sh
set -eu
cd "$(dirname "$0")"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

render() { # name voice text
  say -v "$2" -o "$work/$1.aiff" "$3"
  afconvert -f WAVE -d LEI16@16000 -c 1 "$work/$1.aiff" "$1.wav"
}

render send Samantha "Send the report to Priya by Friday."
render thanks Samantha "Thank you."
render yes Samantha "Yes."
# A deep voice, to check the high-pass filter leaves low voices alone.
render deep "Grandpa (English (US))" "Can we move the meeting to Thursday afternoon?"
