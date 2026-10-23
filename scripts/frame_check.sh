#!/usr/bin/env bash
# On-device check: does the GDExtension load and work on the Steam Frame?
# Copies build/otd-spike.pck and the linux-arm64 .so to ~/apps/otd-spike/ on the
# Frame, then runs the spike headless and silent with the Godot binary already
# on the Frame. It does NOT start the frame-visualizer app.
#
# Why a copy of the binary, not --main-pack: official Godot 4.7.1 export templates
# are built with disable_path_overrides, so --main-pack and --path abort
# ("compiled without support for path overrides"). frame-visualizer.arm64 is
# that template byte for byte. A template loads <its own name>.pck from its own
# folder, so a copy named otd-spike.arm64 loads otd-spike.pck and nothing else.
# A symlink would not work: Linux resolves /proc/self/exe to the real file name.
#
# Needs: scripts/build.sh linux-arm64 and scripts/export_pck.sh first.
# Usage: scripts/frame_check.sh    (FRAME=user@host to override)
set -euo pipefail
cd "$(dirname "$0")/.."
FRAME="${FRAME:-steamos@192.168.1.169}"
DEST=apps/otd-spike
SO=libon_the_drop_godot.linux.arm64.so
test -f build/otd-spike.pck || { echo "no build/otd-spike.pck; run scripts/export_pck.sh" >&2; exit 1; }
test -f "godot/bin/$SO" || { echo "no godot/bin/$SO; run scripts/build.sh linux-arm64" >&2; exit 1; }

ssh "$FRAME" "mkdir -p ~/$DEST/bin"
scp -q build/otd-spike.pck "$FRAME:$DEST/otd-spike.pck"
scp -q "godot/bin/$SO" "$FRAME:$DEST/bin/$SO"

LOG=$(mktemp "${TMPDIR:-/tmp}/otd-frame.XXXXXX")
# shellcheck disable=SC2087  # expand $SO locally on purpose
ssh "$FRAME" bash -s <<REMOTE 2>&1 | tee "$LOG" || true
set -u
cd ~/$DEST
echo "== device: \$(uname -m), \$(ldd --version | head -1)"
echo "== needed libs of bin/$SO:"
ldd bin/$SO
test -x ~/apps/frame-visualizer/frame-visualizer.arm64 || { echo "no ~/apps/frame-visualizer/frame-visualizer.arm64"; exit 1; }
cp -f ~/apps/frame-visualizer/frame-visualizer.arm64 otd-spike.arm64
echo "== probe: --main-pack (expected to abort on official templates)"
timeout 60 ./otd-spike.arm64 --main-pack otd-spike.pck --headless --audio-driver Dummy 2>&1 | head -3
echo "== run: otd-spike.arm64 --headless --audio-driver Dummy"
timeout 120 ./otd-spike.arm64 --headless --audio-driver Dummy
echo "== exit=\$?"
REMOTE
if grep -q "otd-spike: PASS" "$LOG" && grep -q "== exit=0" "$LOG"; then
  echo "FRAME CHECK: PASS"; rm -f "$LOG"; exit 0
fi
echo "FRAME CHECK: FAIL (log: $LOG)"; exit 1
