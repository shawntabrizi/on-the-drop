#!/usr/bin/env bash
# Native vs wasm32 parity of the web binding (crates/on-the-drop-wasm):
# the native half writes target/wasm-parity/*.json, the wasm half (Node,
# wasm-bindgen-test-runner from wasm-bindgen-cli 0.2.129) compares.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo test -p on-the-drop-wasm
CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
  cargo test -p on-the-drop-wasm --test parity --target wasm32-unknown-unknown
