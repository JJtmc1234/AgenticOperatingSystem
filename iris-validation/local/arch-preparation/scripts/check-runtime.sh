#!/usr/bin/env bash
set -uo pipefail
export PATH="$HOME/.local/bin:$HOME/.cargo/bin:$PATH"
fail=0
for cmd in claude carl carl-panel carl-python bwrap jq rg fd bat pw-record pw-play aplay arecord cmake qemu-system-x86_64 xorriso; do
  if command -v "$cmd" >/dev/null; then printf 'OK %s\n' "$cmd"; else printf 'MISSING %s\n' "$cmd"; fail=1; fi
done
for file in .local/share/whisper.cpp/build/bin/whisper-cli \
  .local/share/whisper.cpp/models/ggml-tiny.en.bin \
  .local/share/whisper.cpp/models/ggml-base.en.bin \
  .local/share/piper/piper/piper .local/share/piper/voices/en_US-lessac-medium.onnx \
  .local/share/piper/voices/en_US-lessac-medium.onnx.json; do
  [[ -f "$HOME/$file" ]] || { printf 'MISSING %s\n' "$file"; fail=1; }
done
if [[ -x "$HOME/.local/share/piper/piper/piper" ]]; then
  "$HOME/.local/share/piper/piper/piper" --help >/dev/null 2>&1 || { echo 'Piper needs rebuilding or reinstalling for Arch.'; fail=1; }
fi
if command -v carl-python >/dev/null; then
  carl-python -c 'print(2 + 2)' || fail=1
fi
exit "$fail"
