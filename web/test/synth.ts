// A synthetic EDM track with known beats and form (the idea of
// crates/on-the-drop/tests/common/synth.rs, simplified): 124 BPM, 4/4.
//   intro 8 bars pad + hats | drop 8 bars kick, snare, bass | breakdown 6
//   bars pad | build 6 bars snare roll + riser | finale 12 bars drop + lead,
//   louder | outro 8 bars quiet pad. Starts LEAD s after the file start.
// No real audio anywhere: tests use this only.
export const RATE = 44100;
export const BPM = 124;
export const LEAD = 0.37;
export const FORM: [number, string][] = [[8, 'intro'], [8, 'drop1'], [6, 'breakdown'], [6, 'build'], [12, 'finale'], [8, 'outro']];

export interface Song {
  pcm: Float32Array;
  beats: number[];
  starts: Record<string, number>;
  duration: number;
}

function prng(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (Math.imul(s, 1664525) + 1013904223) >>> 0;
    return s / 4294967296 * 2 - 1;
  };
}

export function makeSong(opts: { bpm?: number; gain?: number } = {}): Song {
  const bpm = opts.bpm ?? BPM;
  const beat = 60 / bpm;
  const bars = FORM.reduce((s, [n]) => s + n, 0);
  const duration = LEAD + bars * 4 * beat + 1;
  const n = Math.floor(duration * RATE);
  const x = new Float32Array(n);
  const rnd = prng(11);
  const beats: number[] = [];
  const starts: Record<string, number> = {};
  const add = (t: number, len: number, f: (u: number, i: number) => number, g: number) => {
    const i0 = Math.round(t * RATE);
    const m = Math.floor(len * RATE);
    for (let i = 0; i < m && i0 + i < n; i++) x[i0 + i] += g * f(i / RATE, i);
  };
  const kick = (u: number) => Math.sin(2 * Math.PI * (50 * u + (100 * 0.03) * (1 - Math.exp(-u / 0.03)))) * Math.exp(-u / 0.12);
  const snare = (u: number) => rnd() * Math.exp(-u / 0.05) + 0.5 * Math.sin(2 * Math.PI * 190 * u) * Math.exp(-u / 0.06);
  let prev = 0;
  const hat = (u: number) => {
    const w = rnd();
    const d = w - prev;
    prev = w;
    return 0.5 * d * Math.exp(-u / 0.025);
  };
  let t = LEAD;
  for (const [nb, part] of FORM) {
    starts[part] = t;
    const partStart = t;
    const partLen = nb * 4 * beat;
    const loud = part === 'finale' ? 1.4 : 1;
    for (let b = 0; b < nb * 4; b++) {
      const bt = t + b * beat;
      beats.push(bt);
      const drums = part === 'drop1' || part === 'finale';
      if (drums) {
        add(bt, 0.3, kick, 0.9 * loud);
        if (b % 4 === 1 || b % 4 === 3) add(bt, 0.2, snare, 0.5 * loud);
        add(bt, beat, (u) => Math.sin(2 * Math.PI * 82 * u) * Math.min(1, u * 40), 0.18 * loud);
      }
      if (part === 'intro' || drums || part === 'breakdown') add(bt + beat / 2, 0.06, hat, 0.35);
      if (part === 'build') {
        const sub = b < nb * 2 ? 2 : 4;
        for (let k = 0; k < sub; k++) add(bt + (k * beat) / sub, 0.12, snare, 0.25 + 0.3 * (b / (nb * 4)));
      }
    }
    // pad (mid band) and the build's riser and the finale's lead
    const pad = { intro: 0.12, drop1: 0.08, breakdown: 0.08, build: 0.06, finale: 0.1, outro: 0.03 }[part] ?? 0.05;
    add(partStart, partLen, (u) => Math.sin(2 * Math.PI * 440 * u) + Math.sin(2 * Math.PI * 554 * u) + Math.sin(2 * Math.PI * 659 * u), pad / 3);
    if (part === 'build') add(partStart, partLen, (u) => (u / partLen) * rnd(), 0.08);
    if (part === 'finale') add(partStart, partLen, (u) => Math.sin(2 * Math.PI * 880 * u + Math.sin(2 * Math.PI * 5 * u)), 0.05);
    t += partLen;
  }
  const g = opts.gain ?? 1;
  for (let i = 0; i < n; i++) x[i] = Math.max(-1, Math.min(1, x[i] * g));
  return { pcm: x, beats, starts, duration: n / RATE };
}
