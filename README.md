# on-the-drop

beatgrid, phrase and drop analysis — so your visuals hit on the drop

**Status:** spike / private. This repo only proves the toolchain end to end
(Rust core, Godot 4.7 GDExtension, macOS arm64 and Steam Frame linux arm64 builds).
No analysis is ported yet.

## Layout

- `crates/on-the-drop` — pure Rust core, no Godot dependency.
- `crates/on-the-drop-godot` — GDExtension (godot-rust `godot` 0.5.5, API 4.7), class `OnTheDrop`.
- `godot/` — minimal Godot 4.7.1 test project; `test/spike.gd` checks the calls headless.
- `scripts/` — `build.sh [macos|linux-arm64|all]`, `test_mac.sh`, `export_pck.sh`, `frame_check.sh`.

## Use

```sh
cargo test -p on-the-drop
scripts/build.sh all        # godot/bin/*.dylib, *.so (glibc 2.28 target)
scripts/test_mac.sh         # headless, Dummy audio
scripts/export_pck.sh       # build/otd-spike.pck
scripts/frame_check.sh      # on the Frame: ~/apps/otd-spike/, headless, Dummy audio
```

## Licence

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
The `godot` crate (godot-rust) is MPL-2.0.
