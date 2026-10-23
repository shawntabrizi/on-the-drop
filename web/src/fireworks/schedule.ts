// The show plan: which shells burst when, from the song map alone (pure,
// deterministic for a map). Every burst lands on a beat of the map (the
// finale's biggest on its last kick), and each shell is launched early by
// its flight time. Calm parts get a few small shells, builds crowd the sky,
// a drop gets a barrage plus one hero shell, the finale the biggest burst.
// At most MAX_PER_S burst moments per second.
import { BREAKDOWN, BUILD, CALM, DROP, type SongMap } from '../types';
import { apexHeight, type V3 } from './ballistics';

export type ShellKind = 'peony' | 'chrysanthemum' | 'willow' | 'ring' | 'palm' | 'glitter' | 'hero' | 'finale';

export interface Shell {
  /** Song time (s) of the burst. */
  burst: number;
  /** Song time (s) of the launch: burst - flight. */
  launch: number;
  flight: number;
  from: V3;
  at: V3;
  /** 0.3 (small) .. 1 (big); hero 1.4, the finale shell 1.8. */
  size: number;
  kind: ShellKind;
  palette: number;
  seed: number;
}

/** Burst moments per second, at most. */
export const MAX_PER_S = 3;
/** s between burst moments (> 1 / MAX_PER_S, so any 1 s holds at most 3). */
export const MIN_GAP = 0.34;
export const FLIGHT_MIN = 2;
export const FLIGHT_MAX = 4;
/** s: a drop or the finale snaps to a beat this close. */
const SNAP = 0.12;
/** Launch line (the barge) and the sky width bursts spread over. */
export const BARGE_Z = -700;
export const SKY_HALF_WIDTH = 330;
export const PALETTES = 8;

interface Moment {
  t: number;
  /** Higher keeps its slot when moments collide. */
  priority: number;
  shells: { size: number; kind: ShellKind }[];
}

/** mulberry32: a small seeded PRNG. */
export function rng(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = a;
    t = Math.imul(t ^ (t >>> 15), t | 1);
    t ^= t + Math.imul(t ^ (t >>> 7), t | 61);
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

function lowerBound(a: number[], t: number): number {
  let lo = 0;
  let hi = a.length;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if (a[m] < t) lo = m + 1;
    else hi = m;
  }
  return lo;
}

/** The beat nearest to t if within `tol`, else t. */
export function snapToBeat(beats: number[], t: number, tol = SNAP): number {
  const i = lowerBound(beats, t);
  let best = t;
  let d = tol;
  for (const j of [i - 1, i]) {
    if (j >= 0 && j < beats.length && Math.abs(beats[j] - t) <= d) {
      d = Math.abs(beats[j] - t);
      best = beats[j];
    }
  }
  return best;
}

function energyAt(m: SongMap, t: number): number {
  const n = Math.floor(m.energy.length / 4);
  if (!n) return 0.5;
  const i = Math.max(0, Math.min(n - 1, Math.round(t / 0.1 - 0.5)));
  return m.energy[i * 4] / 255;
}

interface Span {
  a: number;
  b: number;
  id: number;
}

function spans(m: SongMap): Span[] {
  const out: Span[] = [];
  for (let k = 0; k < m.sections.length; k += 2) {
    const b = k + 2 < m.sections.length ? m.sections[k + 2] : m.duration;
    out.push({ a: m.sections[k], b, id: m.sections[k + 1] });
  }
  return out;
}

/** The time of the finale's biggest burst: its last kick (on its beat), or -1. */
export function finaleTime(m: SongMap): number {
  const [fs, fe] = m.finale;
  if (!(fs >= 0 && fe > fs)) return -1;
  let last = -1;
  for (const k of m.kicks) if (k >= fs && k <= fe) last = k;
  return last < 0 ? -1 : snapToBeat(m.beats, last, 0.05);
}

/** The moments (time, priority, shells) before the rate limit. */
function moments(m: SongMap, rnd: () => number): Moment[] {
  const out: Moment[] = [];
  const secs = spans(m);
  const [fs, fe] = m.finale;
  const dropTimes = m.drops.map((d) => snapToBeat(m.beats, d));
  const fin = finaleTime(m);
  const pick = <T>(xs: T[]): T => xs[Math.floor(rnd() * xs.length) % xs.length];
  let si = 0;
  let bar = -1;
  for (let i = 0; i < m.beats.length; i++) {
    const t = m.beats[i];
    const q = m.beat_bar[i] ?? i % 4;
    if (q === 0) bar++;
    while (si + 1 < secs.length && secs[si + 1].a <= t) si++;
    const s = secs[si] ?? { a: 0, b: m.duration, id: CALM };
    const p = (t - s.a) / Math.max(0.1, s.b - s.a);
    const e = energyAt(m, t);
    const inFinale = fs >= 0 && t >= fs && t <= fe;
    // A breath before each drop: the barrage lands on a clear sky.
    if (dropTimes.some((d) => d > t && d - t < 0.9)) continue;
    const shells: Moment['shells'] = [];
    const add = (n: number, size: number, kinds: ShellKind[]) => {
      for (let j = 0; j < n; j++) shells.push({ size: size * (0.85 + 0.3 * rnd()), kind: pick(kinds) });
    };
    if (s.id === DROP) {
      add((q === 0 ? 2 : 1) + (e > 0.6 && q === 0 ? 1 : 0) + (inFinale ? 1 : 0), inFinale ? 0.8 : 0.65, ['peony', 'chrysanthemum', 'ring', 'palm', 'glitter']);
    } else if (s.id === BUILD) {
      // Faster and fuller towards the drop: every 2 beats, then every beat;
      // small, long-lived stars, so the sky fills up.
      const every = p < 0.5 ? 2 : 1;
      if (q % every === 0) add(1 + Math.floor(p * 3), 0.35 + 0.2 * p, ['glitter', 'willow', 'chrysanthemum', 'peony']);
    } else if (s.id === BREAKDOWN) {
      if (q === 0 && bar % 2 === 0) add(1, 0.55, ['willow', 'ring', 'palm']);
    } else if (q === 0 && bar % 2 === 0 && t - s.a >= 1) {
      add(1, 0.4 + 0.2 * e, ['peony', 'willow', 'ring']);
    }
    if (shells.length) out.push({ t, priority: 1, shells });
  }
  m.drops.forEach((_, i) => {
    const t = dropTimes[i];
    const strength = Math.max(0.2, Math.min(1, m.drop_strength[i] ?? 0.6));
    const shells: Moment['shells'] = [{ size: 1.4, kind: 'hero' }];
    const n = Math.round(5 + 9 * strength);
    for (let j = 0; j < n; j++) shells.push({ size: 0.6 + 0.3 * strength * rnd(), kind: pick(['peony', 'chrysanthemum', 'palm', 'ring', 'glitter']) });
    if (t !== fin) out.push({ t, priority: 2 + strength, shells });
  });
  if (fin >= 0) {
    const shells: Moment['shells'] = [{ size: 1.8, kind: 'finale' }];
    for (let j = 0; j < 8; j++) shells.push({ size: 0.8, kind: pick(['chrysanthemum', 'glitter', 'palm']) });
    out.push({ t: fin, priority: 10, shells });
  }
  return out;
}

/** Keeps the moments that fit MIN_GAP, the highest priority first. */
export function rateLimit<T extends { t: number; priority: number }>(ms: T[], gap = MIN_GAP): T[] {
  const order = ms.map((m, i) => ({ m, i })).sort((a, b) => b.m.priority - a.m.priority || a.m.t - b.m.t || a.i - b.i);
  const kept: number[] = [];
  const out: T[] = [];
  for (const { m } of order) {
    const j = lowerBound(kept, m.t);
    if ((j > 0 && m.t - kept[j - 1] < gap) || (j < kept.length && kept[j] - m.t < gap)) continue;
    kept.splice(j, 0, m.t);
    out.push(m);
  }
  return out.sort((a, b) => a.t - b.t);
}

function flightOf(size: number, kind: ShellKind, rnd: () => number): number {
  if (kind === 'finale') return FLIGHT_MAX;
  if (kind === 'hero') return 3.8;
  const f = FLIGHT_MIN + 1.6 * Math.min(1, size) + 0.3 * rnd();
  return Math.max(FLIGHT_MIN, Math.min(FLIGHT_MAX, f));
}

/** The whole show of a map, sorted by launch time. */
export function schedule(m: SongMap): Shell[] {
  const seed = Math.round(m.duration * 1000) ^ (m.beats.length * 2654435761);
  const rnd = rng(seed);
  const kept = rateLimit(moments(m, rnd));
  const shells: Shell[] = [];
  for (const mo of kept) {
    const n = mo.shells.length;
    const palette = Math.floor(rnd() * PALETTES);
    mo.shells.forEach((s, j) => {
      const flight = flightOf(s.size, s.kind, rnd);
      const big = s.kind === 'hero' || s.kind === 'finale';
      // A barrage spreads across the sky; single shells land anywhere.
      const x = big ? (rnd() - 0.5) * 60 : n > 1 ? ((j + 0.5) / n - 0.5) * 2 * SKY_HALF_WIDTH * 0.9 + (rnd() - 0.5) * 30 : (rnd() - 0.5) * 2 * SKY_HALF_WIDTH * 0.8;
      const z = BARGE_Z + (rnd() - 0.5) * 120;
      const h = apexHeight(flight) * (0.9 + 0.1 * rnd());
      const fromX = x * 0.85 + (rnd() - 0.5) * 30;
      shells.push({
        burst: mo.t,
        launch: mo.t - flight,
        flight,
        from: [fromX, 0, z],
        at: [x, h, z + (rnd() - 0.5) * 40],
        size: s.size,
        kind: s.kind,
        palette: big ? (s.kind === 'finale' ? 0 : 1) : (palette + (j % 3)) % PALETTES,
        seed: Math.floor(rnd() * 1e9),
      });
    });
  }
  return shells.sort((a, b) => a.launch - b.launch);
}
