#!/usr/bin/env bash
# Builds the on-the-drop GDExtension into godot/bin/:
#   macos        libon_the_drop_godot.macos.arm64.dylib  (host build)
#   linux-arm64  libon_the_drop_godot.linux.arm64.so     (Steam Frame, glibc 2.28 target)
# Linux uses cargo-zigbuild with a pinned, hash-checked Zig in .deps/
# (same Zig as frame-visualizer's Beat This! build).
# Usage: scripts/build.sh [macos|linux-arm64|all]   (default: all)
# Needs: macOS arm64 host, rustup (rust-toolchain.toml pins Rust),
#        cargo-zigbuild $ZIGBUILD_VER (cargo install cargo-zigbuild --version $ZIGBUILD_VER --locked).
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT=$(pwd)
WHAT="${1:-all}"
DEPS="$ROOT/.deps"
OUT="$ROOT/godot/bin"
LIB=on_the_drop_godot
GLIBC=2.28           # Frame has glibc 2.39; lower target = wider support.
ZIG_VER=0.15.2
ZIG_SHA=3cc2bab367e185cdfb27501c4b30b1b0653c28d9f73df8dc91488e66ece5fa6b  # zig-aarch64-macos
ZIGBUILD_VER=0.24.0
LLVM_BIN=/opt/homebrew/opt/llvm/bin
mkdir -p "$DEPS" "$OUT"

zig_fetch() {
  local file="zig-aarch64-macos-$ZIG_VER.tar.xz"
  if [ ! -f "$DEPS/$file" ]; then
    echo "fetch zig $ZIG_VER"
    curl -fsSL -o "$DEPS/$file.part" "https://ziglang.org/download/$ZIG_VER/$file"
    mv "$DEPS/$file.part" "$DEPS/$file"
  fi
  local got
  got=$(shasum -a 256 "$DEPS/$file" | cut -d' ' -f1)
  [ "$got" = "$ZIG_SHA" ] || { echo "SHA-256 mismatch for $file: $got" >&2; rm -f "$DEPS/$file"; exit 1; }
  [ -d "$DEPS/zig-aarch64-macos-$ZIG_VER" ] || tar xJf "$DEPS/$file" -C "$DEPS"
}

build_macos() {
  cargo build -p on-the-drop-godot --release --target aarch64-apple-darwin
  cp -f "target/aarch64-apple-darwin/release/lib$LIB.dylib" "$OUT/lib$LIB.macos.arm64.dylib"
  codesign -f -s - "$OUT/lib$LIB.macos.arm64.dylib" 2>/dev/null || true
  echo "== macos: $(du -h "$OUT/lib$LIB.macos.arm64.dylib" | cut -f1)"
  otool -L "$OUT/lib$LIB.macos.arm64.dylib"
}

build_linux() {
  zig_fetch
  local have
  have=$(cargo-zigbuild --version 2>/dev/null | awk '{print $2}')
  [ "$have" = "$ZIGBUILD_VER" ] || { echo "need cargo-zigbuild $ZIGBUILD_VER (have: ${have:-none})" >&2; exit 1; }
  export CARGO_ZIGBUILD_ZIG_PATH="$DEPS/zig-aarch64-macos-$ZIG_VER/zig"
  export ZIG_GLOBAL_CACHE_DIR="$DEPS/zig-cache" ZIG_LOCAL_CACHE_DIR="$DEPS/zig-cache"
  cargo zigbuild -p on-the-drop-godot --release --target "aarch64-unknown-linux-gnu.$GLIBC"
  local so="$OUT/lib$LIB.linux.arm64.so"
  cp -f "target/aarch64-unknown-linux-gnu/release/lib$LIB.so" "$so"
  echo "== linux-arm64: $(du -h "$so" | cut -f1)"
  if [ -x "$LLVM_BIN/llvm-readelf" ]; then
    "$LLVM_BIN/llvm-readelf" -h "$so" | grep -E "Class|Machine"
    echo "needed libs:"
    "$LLVM_BIN/llvm-readelf" -d "$so" | awk '/NEEDED/ {print "  " $NF}'
    echo "max GLIBC symbol version:"
    "$LLVM_BIN/llvm-readelf" --dyn-syms -V "$so" | grep -oE "GLIBC_[0-9]+(\.[0-9]+)+" | sort -uV | tail -1 | sed 's/^/  /'
  else
    echo "no $LLVM_BIN/llvm-readelf (brew install llvm): cannot list needed libs / GLIBC versions" >&2
    exit 1
  fi
}

case "$WHAT" in
  macos) build_macos ;;
  linux-arm64) build_linux ;;
  all) build_macos; build_linux ;;
  *) echo "usage: $0 [macos|linux-arm64|all]" >&2; exit 2 ;;
esac
