// Shapes of the JSON the wasm binding returns (crates/on-the-drop-wasm).

/** Section ids (on_the_drop::sections). */
export const CALM = 0;
export const BUILD = 1;
export const DROP = 2;
export const BREAKDOWN = 3;

/** The track's identity (on_the_drop::track_id::TrackRecord). */
export interface TrackRecord {
  fingerprint_version: string;
  fingerprint: string;
  duration: number;
  sample_rate: number;
  key: string;
  artist?: string;
  title?: string;
  album?: string;
}

/** The map fields (SongMap.to_dict) plus the derived drops and words. */
export interface SongMap {
  duration: number;
  bpm: number;
  beats: number[];
  beat_bar: number[];
  beat_support: number[];
  phrases: number[];
  /** [t0, id0, t1, id1, ...] */
  sections: number[];
  finale: [number, number];
  /** Per 0.1 s: overall, low, mid, high (0..255). */
  energy: number[];
  kicks: number[];
  snares: number[];
  hats: number[];
  kick_amp: number[];
  snare_amp: number[];
  hat_amp: number[];
  character: number;
  kick_cover: number;
  four_floor: number;
  drop_step: number;
  drop_strength: number[];
  drops: number[];
  words: string[];
  vocabulary: string[];
}

/** analyze(): the map plus the overview and the diagnostics. */
export interface Analysis extends SongMap {
  /** 3 bytes per bin: low, mid, high. */
  levels: number[];
  bins: number;
  bin_s: number;
  analysis: number;
  part_bounds: number[];
  fixed_tempo: boolean;
  grid_drift: number;
  bin_raw?: number[];
  bin_air?: number[];
}

export interface MatchResult {
  matched: boolean;
  cover: number;
  score: number;
  offset: number;
}

export interface Label {
  start: number;
  end: number;
  auto: string;
  label: string;
}

export interface TrackLabels {
  file: string;
  labels: Label[];
  track?: TrackRecord;
}

export interface LabelsFile {
  format?: string;
  version?: number;
  tracks: Record<string, TrackLabels>;
}

/** relabel() / applyLabels(). */
export interface LabelResult {
  map: SongMap;
  /** The labels file text after the call. */
  labels: string;
  auto_words: string[];
  user: boolean[];
  changed: boolean;
  log: string[];
}

export const SECTION_WORD_IDS: Record<string, number> = {
  Intro: CALM, Outro: CALM, Low: CALM,
  Build: BUILD, Rising: BUILD,
  Drop: DROP, Peak: DROP,
  Breakdown: BREAKDOWN, Dip: BREAKDOWN,
};
