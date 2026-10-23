// The wasm binding is built from the Rust crate (crates/on-the-drop-wasm),
// not committed: `npm run wasm` (scripts/build_wasm.sh) makes src/wasm/.
import { existsSync } from 'node:fs';

if (!existsSync(new URL('../src/wasm/on_the_drop_bg.wasm', import.meta.url))) {
  console.error('src/wasm/ is missing: run `npm run wasm` first (needs Rust and wasm-bindgen-cli 0.2.129).');
  process.exit(1);
}
