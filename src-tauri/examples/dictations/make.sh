#!/bin/sh
# Speak the dictations in dictations.tsv with macOS voices, into a folder the
# transcribe example can compare models on:
#
#   src-tauri/examples/dictations/make.sh /tmp/dictations
#   cargo run --release --example transcribe -- --compare /tmp/dictations ...
#
# Each dictation is written twice: as spoken (NAME.wav) and with steady
# background noise mixed in at about 15 dB below the speech (NAME-noisy.wav),
# closer to a laptop microphone in a room. Each has a .txt of what was said.
set -e
here=$(cd "$(dirname "$0")" && pwd)
out=${1:?usage: make.sh OUTPUT_FOLDER}
mkdir -p "$out"
while IFS="	" read -r name voice text; do
    say -v "$voice" --file-format=WAVE --data-format=LEI16@16000 -o "$out/$name.wav" "$text"
    printf '%s\n' "$text" > "$out/$name.txt"
    printf '%s\n' "$text" > "$out/$name-noisy.txt"
    python3 -I "$here/noise.py" "$out/$name.wav" "$out/$name-noisy.wav"
done < "$here/dictations.tsv"
ls "$out" | wc -l
