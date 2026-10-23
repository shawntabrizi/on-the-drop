#!/usr/bin/env bash
# Makes the minimal Godot project the parity harness runs the GDScript in:
# frame-visualizer's lib/ and test/song_map_synth.gd from a git ref (default
# master), with no autoloads (nothing starts the player). Does not touch the
# frame-visualizer checkout (git archive reads the object store only).
#   parity/make_fv_min.sh <frame-visualizer repo> <out dir> [ref]
set -euo pipefail
FV_REPO="$1"; OUT="$2"; REF="${3:-master}"
rm -rf "$OUT"; mkdir -p "$OUT"
git -C "$FV_REPO" archive "$REF" lib test/song_map_synth.gd | tar -x -C "$OUT"
git -C "$FV_REPO" rev-parse "$REF" > "$OUT/FV_COMMIT"
cat > "$OUT/project.godot" <<'EOF'
; Minimal project for the on-the-drop parity harness: frame-visualizer lib/ only.
config_version=5

[application]

config/name="otd-parity"
config/use_custom_user_dir=true
config/custom_user_dir_name="otd-parity"
config/features=PackedStringArray("4.7", "Mobile")

[audio]

driver/enable_input=false
EOF
GODOT="${GODOT:-/Applications/Godot.app/Contents/MacOS/Godot}"
mkdir -p "$OUT/.home"
HOME="$OUT/.home" "$GODOT" --headless --audio-driver Dummy --path "$OUT" --import >/dev/null 2>&1 || true
test -f "$OUT/.godot/global_script_class_cache.cfg" && echo "fv-min ready: $OUT ($(cat "$OUT/FV_COMMIT"))"
