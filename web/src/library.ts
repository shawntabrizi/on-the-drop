// The library logic between the analysis, the store and the labels: which
// recording a new file is (by fingerprint), the labels of a track, and the
// labels and map files. No DOM here (tested in Node).
import * as otd from './otd';
import type { StoredTrack, Store } from './db';
import type { Analysis, LabelResult, LabelsFile, TrackLabels, TrackRecord } from './types';

export const LABELS_FORMAT = 'on-the-drop/labels';
/** s: two copies of one recording may differ this much (track_id::DURATION_TOL). */
const DURATION_TOL = 3;

export function labelsText(tracks: Record<string, TrackLabels>): string {
  const f: LabelsFile = { format: LABELS_FORMAT, version: 1, tracks };
  return JSON.stringify(f);
}

function parseLabels(text: string): Record<string, TrackLabels> {
  return (JSON.parse(text) as LabelsFile).tracks ?? {};
}

/** The analysis without the band bins (not needed by the app, ~1 MB a track). */
export function slim(a: Analysis): Analysis {
  const { bin_raw: _r, bin_air: _a, ...rest } = a;
  return rest;
}

/**
 * The library entry of the recording `record`: the same key, else a track
 * whose fingerprint matches (another file or encoding of it).
 */
export async function findTrack(store: Store, record: TrackRecord): Promise<StoredTrack | undefined> {
  const same = await store.track(record.key);
  if (same) return same;
  for (const t of await store.tracks()) {
    if (Math.abs(t.record.duration - record.duration) > DURATION_TOL) continue;
    if (otd.matches(t.record, record).matched) return t;
  }
  return undefined;
}

/**
 * Adds an analysed file to the library. Another copy of a known recording
 * updates that entry (same key, so its labels stay); a new recording also
 * takes over labels stored under another key for it (an imported file).
 */
export async function addAnalysed(store: Store, record: TrackRecord, analysis: Analysis, fileName: string): Promise<{ track: StoredTrack; known: boolean }> {
  const now = Date.now();
  const old = await findTrack(store, record);
  const track: StoredTrack = old
    ? { ...old, name: fileName, files: [...new Set([...old.files, fileName])], analysis: slim(analysis), updated: now }
    : { key: record.key, record, name: fileName, files: [fileName], analysis: slim(analysis), added: now, updated: now };
  await store.putTrack(track);
  if (!old) await adoptLabels(store, track);
  return { track, known: !!old };
}

/**
 * Labels stored under another key for the recording of `track` (from an
 * imported labels file) move to the track's key.
 */
export async function adoptLabels(store: Store, track: StoredTrack): Promise<boolean> {
  const all = await store.labels();
  if (all[track.key]) return false;
  const owned = new Set((await store.tracks()).map((t) => t.key));
  const free: Record<string, TrackLabels> = {};
  for (const [k, v] of Object.entries(all)) {
    if (!owned.has(k) && v.track && Math.abs(v.track.duration - track.record.duration) <= DURATION_TOL) free[k] = v;
  }
  if (!Object.keys(free).length) return false;
  const k = otd.findLabelKey(labelsText(free), track.record);
  if (!k) return false;
  await store.setTrackLabels(track.key, { ...all[k], track: all[k].track ?? track.record });
  await store.setTrackLabels(k, undefined);
  return true;
}

/** The track's map with its labels; `section` >= 0 first sets that section's word. */
export async function labelState(store: Store, track: StoredTrack, section = -1, word = ''): Promise<LabelResult> {
  const all = await store.labels();
  const mine: Record<string, TrackLabels> = all[track.key] ? { [track.key]: all[track.key] } : {};
  const r = otd.relabel(JSON.stringify(track.analysis), labelsText(mine), track.key, track.name, track.record, section, word);
  if (r.changed) await store.setTrackLabels(track.key, parseLabels(r.labels)[track.key]);
  return r;
}

/** All labels as an on-the-drop/labels file (the Rust writer's form). */
export async function exportLabels(store: Store): Promise<string> {
  return otd.normalizeLabels(labelsText(await store.labels()));
}

/**
 * Merges a labels file (on-the-drop/labels, or frame-visualizer's) into the
 * stored labels by recording; library tracks then adopt entries of their
 * recording stored under another key.
 */
export async function importLabels(store: Store, text: string): Promise<{ merged: number; added: number }> {
  const imported = otd.normalizeLabels(text); // throws on another format
  const r = otd.mergeLabels(labelsText(await store.labels()), imported);
  await store.setLabels(parseLabels(r.labels));
  for (const t of await store.tracks()) await adoptLabels(store, t);
  return { merged: r.merged, added: r.added };
}

/** The map file (on-the-drop/map) of a track's labelled map, with its record. */
export function exportMap(track: StoredTrack, map: object): string {
  return otd.mapFile(map, track.record);
}

export function safeName(name: string): string {
  return name.replace(/\.[^.]+$/, '').replace(/[^\w.-]+/g, '_').slice(0, 80) || 'track';
}
