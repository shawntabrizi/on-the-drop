# Parity harness: GDScript vs Rust

Checks that the Rust core gives the same results as frame-visualizer's
GDScript analysis (`lib/song_overview.gd`, `lib/song_map.gd`,
`lib/song_sections.gd`, `lib/song_novelty.gd`) on the same input.

The reference is frame-visualizer commit `2ae8592` (pinned in
`make_fv_min.sh`). Later frame-visualizer commits (branch `fv-rust` on)
call this library and no longer hold the GDScript analysis; git history
keeps it, so `git archive 2ae8592` still works. Pass another ref only to
compare against an older GDScript version.

- `make_fv_min.sh <frame-visualizer repo> <dir> [ref]` — a minimal Godot
  project with frame-visualizer's `lib/` and `test/song_map_synth.gd` at a git
  ref (default `2ae8592`; `git archive`: the checkout is not touched),
  imported headless.
- `fv_dump.gd` — GDScript side: decodes each input as the app does
  (`SongOverview.build` at 44.1 kHz) and writes the map, the band bins it
  computed and the decoded frames.
- `examples/parity.rs` (core crate) — Rust side: stage A (map from the
  GDScript bins) and stage B (bins and map from the decoded frames).
- `compare.py <dir> a|b` — diffs every field; prints a line per input and the
  time per audio minute of both sides.
- `run.sh <fv-min> <dir> <input>...` — all of it per input; the decoded frames
  are deleted after use.

Inputs: `.wav` / `.mp3` files, `synth:<form>` (default, half, drift, late,
fill, repeat, drops, loudbuild, entry: the forms of `test/test_song_map.gd`)
and `bins:<form>` (parts, steady, alt: its band-bin forms, stage A only).

Fields compared: beats, downbeats, bpm, beat_bar, beat_support, phrases,
sections (start and id), section words, drops, drop strengths, finale,
energy, kicks / snares / hats and their strengths, character, kick cover,
four floor, drop step, fixed tempo, grid drift, part boundaries; stage B also
the bins (raw and air) and the overview levels. Tolerance in `compare.py`:
1e-4 (s for times); ids, words, bytes and ints exact. The result so far is
bit equality, so the tolerance is not used.

Silent: Godot runs `--headless --audio-driver Dummy`, nothing plays. All
outputs are derived from audio: keep the out dir outside git.

```sh
parity/make_fv_min.sh ~/Documents/GitHub/frame-visualizer /tmp/fv-min
parity/run.sh /tmp/fv-min /tmp/otd-dump synth:default bins:parts song.wav
```
