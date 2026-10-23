// Typed wrapper of the wasm binding. The page and the worker each load it
// once (`ready()`); the tests load it from bytes (`initFromBytes`).
import init, * as wasm from './wasm/on_the_drop.js';
import type { Analysis, LabelResult, MatchResult, TrackRecord } from './types';

let loading: Promise<unknown> | null = null;

export function ready(): Promise<unknown> {
  loading ??= init();
  return loading;
}

export function initFromBytes(bytes: BufferSource): void {
  wasm.initSync({ module: bytes });
  loading = Promise.resolve();
}

export const version = (): string => wasm.version();
export const analysisVersion = (): number => wasm.analysisVersion();

/** The analysis of `channels` interleaved channels at `rate` Hz; null when too short. */
export function analyze(pcm: Float32Array, channels: number, rate: number): Analysis | null {
  const s = wasm.analyze(pcm, channels, rate);
  return s === '{}' ? null : (JSON.parse(s) as Analysis);
}

export function fingerprint(pcm: Float32Array, channels: number, rate: number): TrackRecord | null {
  const s = wasm.fingerprint(pcm, channels, rate);
  return s === '{}' ? null : (JSON.parse(s) as TrackRecord);
}

export function matches(a: TrackRecord, b: TrackRecord): MatchResult {
  return JSON.parse(wasm.matches(a.fingerprint, a.duration, b.fingerprint, b.duration)) as MatchResult;
}

export function sameTrack(a: TrackRecord, b: TrackRecord): boolean {
  return wasm.sameTrack(JSON.stringify(a), JSON.stringify(b));
}

/**
 * The labels of track `key` applied to `autoMap` (the map as analysed); with
 * `section` >= 0 that section first gets `word` ("" = back to auto).
 */
export function relabel(
  autoMap: string,
  labels: string,
  key: string,
  file: string,
  track: TrackRecord | null,
  section = -1,
  word = '',
): LabelResult {
  const t = track ? JSON.stringify(track) : '';
  return JSON.parse(wasm.relabel(autoMap, labels, key, file, t, section, word)) as LabelResult;
}

export function findLabelKey(labels: string, track: TrackRecord): string {
  return wasm.findLabelKey(labels, JSON.stringify(track));
}

export function mergeLabels(labels: string, imported: string): { labels: string; merged: number; added: number } {
  return JSON.parse(wasm.mergeLabels(labels, imported));
}

/** A map file (on-the-drop/map) of `map` with its record. */
export function mapFile(map: object, track: TrackRecord | null): string {
  return wasm.mapFile(JSON.stringify(map), track ? JSON.stringify(track) : '');
}

export function readMapFile(s: string): { map: import('./types').SongMap; track: TrackRecord | null } {
  return JSON.parse(wasm.readMapFile(s));
}

/** A labels file read and written back in the current form (throws on a bad file). */
export function normalizeLabels(s: string): string {
  return wasm.normalizeLabels(s);
}
