import { beforeAll, describe, expect, test } from 'vitest';
import * as otd from '../src/otd';
import { finaleTime, FLIGHT_MAX, FLIGHT_MIN, MAX_PER_S, rateLimit, schedule, type Shell } from '../src/fireworks/schedule';
import { position, solveLaunch, SHELL_DRAG } from '../src/fireworks/ballistics';
import type { SongMap } from '../src/types';
import { makeSong } from './synth';
import { loadWasm } from './wasm';

let map: SongMap;
let shells: Shell[];

beforeAll(() => {
  loadWasm();
  map = otd.analyze(makeSong().pcm, 1, 44100)!;
  shells = schedule(map);
});

const moments = (ss: Shell[]) => [...new Set(ss.map((s) => s.burst))].sort((a, b) => a - b);

describe('the show plan of an analysed synthetic song', () => {
  test('the song has drops and a finale', () => {
    expect(map.drops.length).toBeGreaterThanOrEqual(2);
    expect(map.finale[0]).toBeGreaterThanOrEqual(0);
    expect(shells.length).toBeGreaterThan(50);
  });

  test('every burst lands exactly on a beat of the map', () => {
    const beats = new Set(map.beats);
    for (const s of shells) expect(beats.has(s.burst), `burst at ${s.burst}`).toBe(true);
  });

  test('each shell launches early by its flight time (2-4 s)', () => {
    for (const s of shells) {
      expect(s.flight).toBeGreaterThanOrEqual(FLIGHT_MIN);
      expect(s.flight).toBeLessThanOrEqual(FLIGHT_MAX);
      expect(s.launch).toBeCloseTo(s.burst - s.flight, 9);
    }
  });

  test('the drag-ballistic arc reaches the burst point at the burst time', () => {
    for (const s of shells.slice(0, 40)) {
      const v0 = solveLaunch(s.from, s.at, s.flight, SHELL_DRAG);
      const p = position(s.from, v0, SHELL_DRAG, 1, s.flight);
      for (let k = 0; k < 3; k++) expect(p[k]).toBeCloseTo(s.at[k], 6);
      expect(v0[1]).toBeGreaterThan(0);
    }
  });

  test(`at most ${MAX_PER_S} burst moments in any second`, () => {
    const ts = moments(shells);
    for (let i = 0; i < ts.length; i++) {
      const inSecond = ts.filter((t) => t >= ts[i] && t < ts[i] + 1).length;
      expect(inSecond, `from ${ts[i]}`).toBeLessThanOrEqual(MAX_PER_S);
    }
  });

  test('each drop gets a barrage and exactly one hero shell', () => {
    const fin = finaleTime(map);
    for (const d of map.drops) {
      const at = shells.filter((s) => Math.abs(s.burst - d) < 0.13);
      if (Math.abs(d - fin) < 0.13) continue; // the finale's own drop: see below
      expect(at.filter((s) => s.kind === 'hero').length, `drop at ${d}`).toBe(1);
      expect(at.length, `barrage at ${d}`).toBeGreaterThanOrEqual(6);
    }
  });

  test("the finale's biggest burst lands on its last kick", () => {
    const [fs, fe] = map.finale;
    const kicks = map.kicks.filter((k) => k >= fs && k <= fe);
    const last = kicks[kicks.length - 1];
    const biggest = shells.reduce((a, b) => (b.size > a.size ? b : a));
    expect(biggest.kind).toBe('finale');
    expect(Math.abs(biggest.burst - last)).toBeLessThanOrEqual(0.05);
    expect(map.beats).toContain(biggest.burst);
  });

  test('builds crowd the sky: more shells per second at the end of a build than at its start', () => {
    const k = map.sections.findIndex((x, i) => i % 2 === 1 && x === 1);
    const a = map.sections[k - 1];
    const b = map.sections[k + 1];
    const third = (b - a) / 3;
    const count = (t0: number, t1: number) => shells.filter((s) => s.burst >= t0 && s.burst < t1).length;
    expect(count(b - third, b)).toBeGreaterThan(count(a, a + third));
  });

  test('the plan is the same for the same map', () => {
    expect(schedule(map)).toEqual(shells);
  });
});

test('the rate limit keeps the higher priority', () => {
  const kept = rateLimit([
    { t: 1.0, priority: 1 },
    { t: 1.2, priority: 5 },
    { t: 1.5, priority: 1 },
    { t: 1.6, priority: 1 },
  ]);
  expect(kept.map((m) => m.t)).toEqual([1.2, 1.6]);
});
