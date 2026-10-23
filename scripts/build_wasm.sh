#!/usr/bin/env bash
# Builds the web binding into web/src/wasm/ (wasm-bindgen --target web).
# Needs the wasm32-unknown-unknown target and wasm-bindgen-cli 0.2.129
# (cargo install wasm-bindgen-cli --version =0.2.129 --locked).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -p on-the-drop-wasm --target wasm32-unknown-unknown --release
wasm-bindgen --target web --out-dir web/src/wasm --out-name on_the_drop \
  target/wasm32-unknown-unknown/release/on_the_drop_wasm.wasm
