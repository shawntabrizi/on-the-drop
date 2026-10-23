#!/usr/bin/env bash
# Runs the Godot spike test headless and silent on the Mac (no window, Dummy audio).
# Needs: scripts/build.sh macos first. Exit 0 only if the log shows "otd-spike: PASS".
set -uo pipefail
cd "$(dirname "$0")/.."
GODOT="${GODOT:-/Applications/Godot.app/Contents/MacOS/Godot}"
RUN=(--headless --audio-driver Dummy)
# Import writes godot/.godot/extension_list.cfg, which loads the extension.
# Godot 4.7.1 crashes (signal 11) on exit of a FIRST import of any project with a
# GDExtension (also with godot-cpp ones); the import itself is complete. Ignore it.
( "$GODOT" "${RUN[@]}" --path godot --import ) >/dev/null 2>&1 || true
LOG=$(mktemp "${TMPDIR:-/tmp}/otd-test.XXXXXX")
"$GODOT" "${RUN[@]}" --path godot >"$LOG" 2>&1
code=$?
grep -E "otd-spike|ERROR" "$LOG"
grep -q "otd-spike: PASS" "$LOG" || code=1
rm -f "$LOG"
echo "exit=$code"
exit $code
