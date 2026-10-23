import { beforeAll, expect, test } from 'vitest';
import * as otd from '../src/otd';
import { COMET, FLASH, Particles, STAR, TRAIL, type Particle } from '../src/fireworks/particles';
import { schedule, type Shell } from '../src/fireworks/schedule';
import { Show } from '../src/fireworks/show';
import { makeSong } from './synth';
import { loadWasm } from './wasm';

let shells: Shell[];

beforeAll(() => {
  loadWasm();
  shells = schedule(otd.analyze(makeSong().pcm, 1, 44100)!);
});

/** Runs the show from t0 to t1 at `fps`; the particles written. */
function run(t0: number, t1: number, fps: number, seekTo?: number): Particle[] {
  const p = new Particles(1 << 16);
  let out: Particle[] = [];
  p.emit = (q) => {
    out.push(structuredClone(q));
  };
  p.clear = () => {
    out = [];
  };
  p.flush = () => {};
  const s = new Show(p);
  s.setShells(shells, t0);
  for (let t = t0; t <= t1 + 1e-9; t += 1 / fps) s.update(t);
  if (seekTo !== undefined) s.update(seekTo);
  return out;
}

const alive = (ps: Particle[], t: number) =>
  ps.filter((q) => q.t0 <= t && t <= q.t0 + q.life).map((q) => JSON.stringify(q)).sort();

test('the sky is the same at any frame rate and after a seek', () => {
  const t = 30;
  const a = alive(run(20, t, 60), t);
  const b = alive(run(20, t, 144), t);
  const c = alive(run(0, 5, 60, t), t);
  expect(a.length).toBeGreaterThan(500);
  expect(b).toEqual(a);
  expect(c).toEqual(a);
});

test('stars start at the burst point at the burst time; comets at the launch', () => {
  const ps = run(0, 40, 60);
  const bursts = new Map(shells.map((s) => [s.burst, s]));
  const launches = new Set(shells.map((s) => s.launch));
  for (const q of ps) {
    if (q.kind === STAR || q.kind === FLASH) {
      const s = bursts.get(q.t0);
      expect(s, `a star born at ${q.t0}`).toBeDefined();
      expect(shells.some((x) => x.burst === q.t0 && x.at.every((v, k) => v === q.p[k]))).toBe(true);
    }
    if (q.kind === COMET) expect(launches.has(q.t0)).toBe(true);
  }
});

test('trails hang where they were emitted', () => {
  const ps = run(0, 40, 60).filter((q) => q.kind === TRAIL);
  expect(ps.length).toBeGreaterThan(1000);
  for (const q of ps) {
    const speed = Math.hypot(...q.v);
    // a slow drift only, stopped quickly by drag: it stays within a few units
    expect(speed / q.drag).toBeLessThan(4);
    expect(q.gs).toBeLessThan(0.2);
  }
});
