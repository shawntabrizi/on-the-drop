# on-the-drop

beatgrid, phrase and drop analysis — so your visuals hit on the drop

**Status:** pre-release; the API changes without notice. The whole-song
analysis of frame-visualizer, a VR music visualizer for the Steam Frame (master
2ae8592: `lib/song_overview.gd`, `song_map.gd`, `song_sections.gd`,
`song_novelty.gd`, `section_overrides.gd`) is ported to Rust and gives the
same results bit for bit on the same input (`parity/`: 103 inputs, map from
the GDScript bins; 100 inputs, bins and map from the decoded frames). The live
analysis (`band_math`) is not ported. Speed per audio minute (this arm64 Mac, one core):
map from bins 1.5 ms (GDScript 47 ms); bins + map from decoded frames 17 ms
(GDScript 873 ms).

## Layout

- `crates/on-the-drop` — pure Rust core, no Godot dependency.
- `crates/on-the-drop-godot` — GDExtension (godot-rust `godot` 0.5.5, API 4.7), class `OnTheDrop`.
- `godot/` — minimal Godot 4.7.1 test project; `test/spike.gd` checks the calls headless.
- `parity/` — GDScript vs Rust parity harness (see its README).
- `scripts/` — `build.sh [macos|linux-arm64|all]`, `test_mac.sh`, `export_pck.sh`, `frame_check.sh`.

## Use

```sh
cargo test -p on-the-drop  # song_map, overrides, track_id tests
scripts/build.sh all        # godot/bin/*.dylib, *.so (glibc 2.28 target)
scripts/test_mac.sh         # headless, Dummy audio
scripts/export_pck.sh       # build/otd-spike.pck
FRAME=user@host scripts/frame_check.sh  # on the Frame: ~/apps/otd-spike/, headless, Dummy audio
```

## Web app

`web/` is a browser tool to prepare tracks, with a fireworks visual
(Vite, TypeScript, three.js). Add files or a whole folder (picker or drag
and drop). The app decodes them with Web Audio, and a Web Worker runs the
analysis and the track ID in WebAssembly (`crates/on-the-drop-wasm`, the
same results as native). The timeline shows the waveform, the beats and
downbeats, the drops with their strength and the sections. Click a
section to change its label (✱ marks your labels; "Reset to auto" undoes
a change). Export the labels (`on-the-drop/labels`) or a track's map
(`on-the-drop/map`); import labels from another browser. The fireworks
burst on the beats of the map.

**Privacy: local only.** Nothing is uploaded. The audio stays on your
device; the results and labels stay in the browser's IndexedDB. The app
makes no requests to other sites, and has no analytics.

Run it locally (needs Rust with the `wasm32-unknown-unknown` target,
`wasm-bindgen-cli` 0.2.129 and Node 22):

```sh
cargo install wasm-bindgen-cli --version =0.2.129 --locked
cd web
npm ci
npm run wasm   # scripts/build_wasm.sh: the binding into web/src/wasm/
npm run dev    # http://localhost:5173
npm test       # web tests; scripts/wasm_test.sh: native vs wasm parity
```

## API

Core (`on_the_drop`): `analyze_stereo(frames, rate)`, `analyze_mono`,
`analyze_bins(Bins)` -> `Analysis { bins, levels, map: SongMap }`. Decoding
is the caller's. `overrides::LabelSession` applies the user's section labels
to a map; `format` reads and writes the JSON files; `track_id` identifies a
track.

Godot (`OnTheDrop`, all static): `analyze(PackedVector2Array, rate)`,
`analyze_mono(PackedFloat32Array, rate)`, `analyze_bins(raw, nb, bin_s, air)`
-> a Dictionary in frame-visualizer's `SongMap.to_dict` shape plus `levels`,
`bins`, `bin_s`, `analysis`, the diagnostics `part_bounds`, `fixed_tempo`,
`grid_drift` and the bins `bin_raw`, `bin_air` (empty on bad input);
`sections(raw, nb, bin_s, beats)` -> the raw section changes;
`fingerprint(frames, rate)`
-> the track record; `matches(fp_a, dur_a, fp_b, dur_b)` -> `matched`,
`cover`, `score`, `offset` (s); `version()`, `analysis_version()`.

## Track identity

A track is known by an acoustic fingerprint, not a file hash, so one
recording keeps its labels across decoders, sample rates, re-encodes and
padding. [rusty-chromaprint](https://crates.io/crates/rusty-chromaprint)
`=0.3.0` (MIT), preset `test2` (Chromaprint's default), on PCM quantised to
16 bit. `FINGERPRINT_VERSION` (`chromaprint-rs-0.3.0/test2/s16/1`) names the
crate, the preset and the front end. **Changing this breaks all saved IDs.
Bump the version only with a migration that keeps the old fingerprint next to
the new one.** A golden test guards it.

Match: Chromaprint's matcher; segments with a mean bit error under 8 of 32
bits must cover at least 80 % of the shorter fingerprint, and the durations
must agree within 3 s. The offset of the longest segment says how far the
copies are shifted. Calibration on 31 songs against MP3 re-encodes at 44.1
and 48 kHz: same recording cover 1.000, score 0.04–0.16 bits (bit error rate
0.1–0.5 %); 930 different-song pairs cover at most 0.034.

Record (`TrackRecord`, JSON): `fingerprint_version`, `fingerprint` (base64 of
the u32 items, little-endian: the crate cannot decompress the AcoustID
form), `duration`, `sample_rate`, `key` (a lookup hint, not proof), optional
`artist`, `title`, `album`. Records of another fingerprint version are not
compared bit by bit: normalised artist + title and a duration within 3 s
decide then.

## Files

- Map (`{"format": "on-the-drop/map", "version": 1, "analysis": 11, "track":
  TrackRecord?, "map": {...}}`): the `SongMap.to_dict` fields. A map of
  another `analysis` version is refused (analyse again).
- Labels (`{"format": "on-the-drop/labels", "version": 1, "tracks": {key:
  {"file", "labels": [{"start", "end", "auto", "label"}], "track"?:
  TrackRecord}}}`): the user's
  section labels; frame-visualizer's `section_labels.json` (no `format`)
  reads as version 1.

## Licence

MIT OR Apache-2.0, at your option. See `LICENSE-MIT` and `LICENSE-APACHE`.
The `godot` crate (godot-rust) is MPL-2.0.
