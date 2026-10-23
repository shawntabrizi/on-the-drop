// Loads the wasm binding (built by scripts/build_wasm.sh) in Node.
import { readFileSync } from 'node:fs';
import { initFromBytes } from '../src/otd';

let loaded = false;

export function loadWasm(): void {
  if (loaded) return;
  initFromBytes(readFileSync(new URL('../src/wasm/on_the_drop_bg.wasm', import.meta.url)));
  loaded = true;
}
