#!/usr/bin/env bash
# Parity harness driver: GDScript (frame-visualizer master) vs Rust, per input.
#   parity/run.sh <fv-min> <out dir> <input> ...
# <fv-min>: a Godot project that holds frame-visualizer's lib/ and
#   test/song_map_synth.gd (parity/make_fv_min.sh makes one from master).
# <input>: a .wav / .mp3 file, synth:<form> or bins:<form> (see fv_dump.gd).
# Per input: GDScript dump (+ PCM), Rust stage A (from the GDScript bins) and
# stage B (from the PCM); the PCM is deleted after use. Then compare.py a / b.
# Godot runs headless with the Dummy audio driver: nothing plays.
# Outputs are audio-derived: put <out dir> outside git.
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
FV="$1"; OUT="$2"; shift 2
GODOT="${GODOT:-/Applications/Godot.app/Contents/MacOS/Godot}"
mkdir -p "$OUT" "$OUT/.home"
cargo build --release -q -p on-the-drop --example parity --manifest-path "$ROOT/Cargo.toml" || exit 1
P="$ROOT/target/release/examples/parity"
for inp in "$@"; do
  case "$inp" in
    synth:*) name="synth_${inp#synth:}" ;;
    bins:*) name="bins_${inp#bins:}" ;;
    *) name="$(basename "${inp%.*}")" ;;
  esac
  if [ -f "$OUT/$name.a.json" ] && { [[ "$inp" == bins:* ]] || [ -f "$OUT/$name.b.json" ]; }; then
    continue  # done in an earlier run
  fi
  # Godot's user dir goes to a private HOME (the sandbox may block ~/Library).
  HOME="$OUT/.home" "$GODOT" --headless --audio-driver Dummy --path "$FV" \
    --script "$ROOT/parity/fv_dump.gd" -- --out "$OUT" --pcm "$inp" 2>&1 | grep -E "fv_dump" || true
  [ -f "$OUT/$name.bins" ] || { echo "run: no GDScript dump for $inp"; continue; }
  "$P" bins "$OUT/$name.bins" "$OUT/$name.a.json"
  if [ -f "$OUT/$name.pcm" ]; then
    "$P" pcm "$OUT/$name.pcm" "$OUT/$name.b.json" "$OUT/$name.rs.bins"
    rm -f "$OUT/$name.pcm"
  fi
done
python3 "$ROOT/parity/compare.py" "$OUT" a; a=$?
python3 "$ROOT/parity/compare.py" "$OUT" b; b=$?
exit $((a | b))
