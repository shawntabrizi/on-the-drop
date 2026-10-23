#!/usr/bin/env bash
# Exports the spike project to build/otd-spike.pck with the "Steam Frame Arm64"
# preset (official Godot 4.7.1 templates, the same ones frame-visualizer uses).
# The pck holds the scripts and the .gdextension, not the .so: the .so must sit
# at bin/ under the working directory when the app starts (res://bin/... resolves
# to ./bin/... when the pck is not inside a project folder).
set -euo pipefail
cd "$(dirname "$0")/.."
GODOT="${GODOT:-/Applications/Godot.app/Contents/MacOS/Godot}"
RUN=(--headless --audio-driver Dummy)
mkdir -p build
( "$GODOT" "${RUN[@]}" --path godot --import ) >/dev/null 2>&1 || true  # see test_mac.sh
"$GODOT" "${RUN[@]}" --path godot --export-pack "Steam Frame Arm64" ../build/otd-spike.pck >/dev/null
ls -la build/otd-spike.pck
