#!/usr/bin/env bash
# Makes the minimal Godot project the parity harness runs the GDScript in:
# frame-visualizer's lib/ and test/song_map_synth.gd from a git ref (default
# FV_REF: 2ae8592, the last commit with the GDScript analysis; later
# frame-visualizer commits call on-the-drop instead), with no autoloads (nothing starts the player). Does not touch the
# frame-visualizer checkout (git archive reads the object store only).
#   parity/make_fv_min.sh <frame-visualizer repo> <out dir> [ref]
set -euo pipefail
FV_REF=2ae85920373c72f81ba8eca583a7edc6a7bc40bd  # frame-visualizer master 2ae8592, the reference of the port
FV_REPO="$1"; OUT="$2"; REF="${3:-$FV_REF}"
rm -rf "$OUT"; mkdir -p "$OUT"
git -C "$FV_REPO" cat-file -e "$REF^{commit}" || { echo "no commit $REF in $FV_REPO" >&2; exit 1; }
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
