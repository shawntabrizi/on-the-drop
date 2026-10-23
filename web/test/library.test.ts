// The library with a fake IndexedDB: import, re-import of another copy of
// the same recording (found by fingerprint), labels, and the exported files
// read back through the Rust readers (the wasm build of on_the_drop::format).
import 'fake-indexeddb/auto';
import { beforeAll, expect, test } from 'vitest';
import * as otd from '../src/otd';
import { Store } from '../src/db';
import { addAnalysed, exportLabels, exportMap, importLabels, labelState } from '../src/library';
import type { Analysis, TrackRecord } from '../src/types';
import { makeSong, RATE } from './synth';
import { loadWasm } from './wasm';

let song: ReturnType<typeof makeSong>;
let rec: TrackRecord;
let analysis: Analysis;

beforeAll(() => {
  loadWasm();
  song = makeSong();
  rec = otd.fingerprint(song.pcm, 1, RATE)!;
  analysis = otd.analyze(song.pcm, 1, RATE)!;
});

let dbn = 0;
const open = () => Store.open(indexedDB, `test-${++dbn}`);

/** Another "encoding" of the song: stereo, quieter, 8-bit-ish, padded. */
function otherCopy(): Float32Array {
  const pad = Math.round(0.25 * RATE);
  const out = new Float32Array((song.pcm.length + pad) * 2);
  for (let i = 0; i < song.pcm.length; i++) {
    const v = Math.round(song.pcm[i] * 0.8 * 127) / 127;
    out[2 * (i + pad)] = v;
    out[2 * (i + pad) + 1] = v * 0.9;
  }
  return out;
}

test('a label is set, shown with ✱ data, stored with the fingerprint record, and reset', async () => {
  const store = await open();
  const { track, known } = await addAnalysed(store, rec, analysis, 'song.wav');
  expect(known).toBe(false);
  expect(track.key).toBe(rec.key);
  expect(track.analysis.bin_raw).toBeUndefined();
  const auto = await labelState(store, track);
  expect(auto.user.every((u) => !u)).toBe(true);
  expect(auto.map.words).toEqual(analysis.words);
  // the build after the breakdown becomes a drop: a drop starts there
  const i = auto.map.words.lastIndexOf('Build');
  expect(auto.map.words[i - 1]).toBe('Breakdown');
  const r = await labelState(store, track, i, 'Drop');
  expect(r.user[i]).toBe(true);
  expect(r.map.words[i]).toBe('Drop');
  expect(r.auto_words[i]).toBe('Build');
  expect(r.map.drops).toContain(r.map.sections[i * 2]);
  expect(r.map.drop_strength[r.map.drops.indexOf(r.map.sections[i * 2])]).toBe(1);
  const stored = (await store.labels())[track.key];
  expect(stored.labels).toHaveLength(1);
  expect(stored.labels[0]).toMatchObject({ auto: 'Build', label: 'Drop' });
  expect(stored.track).toEqual(rec);
  // a new session finds the label again (anchored by time)
  const again = await labelState(store, track);
  expect(again.user[i]).toBe(true);
  // reset to auto
  const reset = await labelState(store, track, i, '');
  expect(reset.user[i]).toBe(false);
  expect(reset.map.words[i]).toBe('Build');
  expect(reset.map.drops).toEqual(analysis.drops);
  expect((await store.labels())[track.key]).toBeUndefined();
});

test('another file of the same recording finds its labels through matches()', async () => {
  const store = await open();
  const { track } = await addAnalysed(store, rec, analysis, 'song.wav');
  const i = analysis.words.indexOf('Breakdown');
  await labelState(store, track, i, 'Drop');
  const pcm2 = otherCopy();
  const rec2 = otd.fingerprint(pcm2, 2, RATE)!;
  expect(rec2.key).not.toBe(rec.key);
  expect(otd.matches(rec, rec2).matched).toBe(true);
  const a2 = otd.analyze(pcm2, 2, RATE)!;
  const { track: t2, known } = await addAnalysed(store, rec2, a2, 'song (copy).mp3');
  expect(known).toBe(true);
  expect(t2.key).toBe(track.key);
  expect(t2.files).toEqual(['song.wav', 'song (copy).mp3']);
  const r = await labelState(store, t2);
  expect(r.map.words.filter((_, k) => r.user[k])).toEqual(['Drop']);
  expect((await store.tracks()).length).toBe(1);
});

test('exported labels and map files round-trip through the Rust readers', async () => {
  const store = await open();
  const { track } = await addAnalysed(store, rec, analysis, 'song.wav');
  const i = analysis.words.indexOf('Breakdown');
  const r = await labelState(store, track, i, 'Drop');
  const labels = await exportLabels(store);
  const parsed = JSON.parse(labels);
  expect(parsed.format).toBe('on-the-drop/labels');
  expect(parsed.tracks[track.key].track).toEqual(rec);
  expect(otd.normalizeLabels(labels)).toBe(labels);
  const map = exportMap(track, r.map);
  const back = otd.readMapFile(map);
  expect(back.track).toEqual(rec);
  expect(back.map.sections).toEqual(r.map.sections);
  expect(back.map.drops).toEqual(r.map.drops);
  expect(back.map.drop_strength).toEqual(r.map.drop_strength);
  expect(back.map.beats).toEqual(r.map.beats);
  expect(otd.mapFile(back.map, back.track)).toBe(map);
  expect(JSON.parse(map)).toMatchObject({ format: 'on-the-drop/map', version: 1, analysis: otd.analysisVersion() });
});

test('imported labels merge by fingerprint into the library', async () => {
  const a = await open();
  const { track } = await addAnalysed(a, rec, analysis, 'song.wav');
  const i = analysis.words.indexOf('Breakdown');
  await labelState(a, track, i, 'Drop');
  const file = await exportLabels(a);
  // another browser has the same recording from another file (another key)
  const b = await open();
  const pcm2 = otherCopy();
  const rec2 = otd.fingerprint(pcm2, 2, RATE)!;
  const { track: t2 } = await addAnalysed(b, rec2, otd.analyze(pcm2, 2, RATE)!, 'copy.flac');
  const r = await importLabels(b, file);
  expect(r).toEqual({ merged: 0, added: 1 });
  const all = await b.labels();
  expect(Object.keys(all)).toEqual([t2.key]);
  const st = await labelState(b, t2);
  expect(st.map.words.filter((_, k) => st.user[k])).toEqual(['Drop']);
  // importing again merges into the same entry
  expect(await importLabels(b, file)).toEqual({ merged: 1, added: 0 });
  await expect(importLabels(b, '{"format":"something/else","tracks":{}}')).rejects.toThrow();
});
