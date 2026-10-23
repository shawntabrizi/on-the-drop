// The show: turns the schedule into particles as song time passes. Each
// frame handles the window (last, now]: shells launched in it (a comet with
// a spark trail), shells that burst in it (stars by shell kind, a flash),
// and the trails of moving stars, sampled on a fixed time grid. A trail
// particle is born where its star was at that instant and stays there
// (it only cools and sinks a little). Because the motion is in closed form,
// a seek re-creates the sky as it would be, by replaying the last seconds.
import { position, solveLaunch, SHELL_DRAG, velocity, type V3 } from './ballistics';
import { COMET, FLASH, GLITTER, Particles, STAR, TRAIL, type Particle } from './particles';
import { rng, type Shell, type ShellKind } from './schedule';

type RGB = [number, number, number];

/** [start colour, end colour] per palette (stars change from one to the other). */
export const PALETTE: [RGB, RGB][] = [
  [[1.0, 0.85, 0.55], [1.0, 0.55, 0.15]], // gold -> amber
  [[0.95, 0.95, 1.0], [0.45, 0.65, 1.0]], // silver -> blue
  [[1.0, 0.25, 0.3], [1.0, 0.75, 0.3]], // red -> gold
  [[0.4, 1.0, 0.5], [0.9, 1.0, 0.85]], // green -> white
  [[0.75, 0.4, 1.0], [1.0, 0.45, 0.75]], // violet -> pink
  [[0.35, 0.85, 1.0], [0.25, 0.4, 1.0]], // cyan -> blue
  [[1.0, 0.6, 0.85], [0.7, 0.5, 1.0]], // pink -> lilac
  [[1.0, 1.0, 0.8], [1.0, 0.7, 0.3]], // white -> gold
];
const GOLD: RGB = [1.0, 0.7, 0.3];

/**
 * s of song time replayed after a seek: the longest chain from a burst to
 * its last particle (a willow star emits trails until ~3.7 s, which live up
 * to ~3.4 s).
 */
export const REPLAY = 8;
const COMET_DT = 1 / 60;
const TRAIL_DT = 1 / 30;
const STAR_DRAG = 1.6;
const STAR_GS = 0.35;

interface Emitter {
  p0: V3;
  v0: V3;
  drag: number;
  gs: number;
  t0: number;
  end: number;
  dt: number;
  color: RGB;
  life: number;
  size: number;
  seed: number;
  spread: number;
}

/** What a shell kind bursts into. */
interface Recipe {
  stars: number;
  /** Burst radius (units) for size 1. */
  radius: number;
  life: number;
  drag: number;
  gs: number;
  kind: number;
  /** Share of stars that leave a trail, and the trail particles' life. */
  trail: number;
  trailLife: number;
  ring?: boolean;
  starSize: number;
}

const RECIPES: Record<ShellKind, Recipe> = {
  peony: { stars: 110, radius: 75, life: 2.0, drag: STAR_DRAG, gs: STAR_GS, kind: STAR, trail: 0, trailLife: 0, starSize: 6 },
  chrysanthemum: { stars: 90, radius: 80, life: 2.2, drag: STAR_DRAG, gs: STAR_GS, kind: STAR, trail: 1, trailLife: 0.9, starSize: 5.5 },
  willow: { stars: 70, radius: 60, life: 3.8, drag: 1.1, gs: 0.6, kind: STAR, trail: 1, trailLife: 2.6, starSize: 4.5 },
  ring: { stars: 64, radius: 80, life: 1.9, drag: STAR_DRAG, gs: STAR_GS, kind: STAR, trail: 0, trailLife: 0, ring: true, starSize: 6.5 },
  palm: { stars: 9, radius: 95, life: 2.2, drag: 1.3, gs: 0.5, kind: STAR, trail: 1, trailLife: 1.4, starSize: 11 },
  glitter: { stars: 120, radius: 70, life: 1.8, drag: STAR_DRAG, gs: STAR_GS, kind: GLITTER, trail: 0, trailLife: 0, starSize: 4.5 },
  hero: { stars: 200, radius: 100, life: 3.0, drag: 1.4, gs: 0.4, kind: STAR, trail: 0.5, trailLife: 1.6, starSize: 7 },
  finale: { stars: 300, radius: 120, life: 3.6, drag: 1.3, gs: 0.45, kind: STAR, trail: 0.6, trailLife: 2.2, starSize: 8 },
};

export class Show {
  private shells: Shell[] = [];
  private byBurst: Shell[] = [];
  private iLaunch = 0;
  private iBurst = 0;
  private last = Number.NEGATIVE_INFINITY;
  private emitters: Emitter[] = [];
  private q: Particle = {
    p: [0, 0, 0], v: [0, 0, 0], t0: 0, life: 1, drag: 1, gs: 1,
    c0: [1, 1, 1], c1: [1, 1, 1], size: 1, kind: 0, seed: 0, bright: 1,
  };
  private tmp: V3 = [0, 0, 0];
  private tmpV: V3 = [0, 0, 0];
  /** Particles written (stats). */
  emitted = 0;

  constructor(private particles: Particles) {}

  setShells(shells: Shell[], t: number): void {
    this.shells = shells;
    this.byBurst = [...shells].sort((a, b) => a.burst - b.burst);
    this.reset(t);
  }

  /** Clears the sky and replays the last REPLAY s up to t. */
  reset(t: number): void {
    this.particles.clear();
    this.emitters = [];
    this.last = t - REPLAY;
    this.iLaunch = lower(this.shells, (s) => s.launch, this.last);
    this.iBurst = lower(this.byBurst, (s) => s.burst, this.last);
    // Shells in flight at the window start: their comets and trails.
    for (const s of this.shells) {
      if (s.launch <= this.last && s.burst > this.last) this.launch(s);
    }
    this.update(t);
  }

  /** Advances to song time t (a jump back or far ahead is a seek). */
  update(t: number): void {
    if (t < this.last - 0.05 || t - this.last > REPLAY + 1) {
      this.reset(t);
      return;
    }
    const a = this.last;
    while (this.iLaunch < this.shells.length && this.shells[this.iLaunch].launch <= t) {
      const s = this.shells[this.iLaunch++];
      if (s.launch > a) this.launch(s);
    }
    while (this.iBurst < this.byBurst.length && this.byBurst[this.iBurst].burst <= t) {
      const s = this.byBurst[this.iBurst++];
      if (s.burst > a) this.burst(s);
    }
    this.runEmitters(a, t);
    this.last = t;
    this.particles.flush();
  }

  private put(p: V3, v: V3, t0: number, life: number, drag: number, gs: number, c0: RGB, c1: RGB, size: number, kind: number, seed: number, bright: number): void {
    const q = this.q;
    q.p[0] = p[0]; q.p[1] = p[1]; q.p[2] = p[2];
    q.v[0] = v[0]; q.v[1] = v[1]; q.v[2] = v[2];
    q.t0 = t0; q.life = life; q.drag = drag; q.gs = gs;
    q.c0 = c0; q.c1 = c1; q.size = size; q.kind = kind; q.seed = seed; q.bright = bright;
    this.particles.emit(q);
    this.emitted++;
  }

  private launch(s: Shell): void {
    const v0 = solveLaunch(s.from, s.at, s.flight, SHELL_DRAG);
    const big = s.kind === 'hero' || s.kind === 'finale';
    this.put(s.from, v0, s.launch, s.flight, SHELL_DRAG, 1, GOLD, [1, 0.9, 0.7], big ? 7 : 5, COMET, s.seed % 997, big ? 1.2 : 0.9);
    this.emitters.push({
      p0: s.from, v0, drag: SHELL_DRAG, gs: 1, t0: s.launch, end: s.burst, dt: COMET_DT,
      color: GOLD, life: big ? 1.1 : 0.7, size: big ? 3.5 : 2.6, seed: s.seed, spread: 2.5,
    });
  }

  private burst(s: Shell): void {
    const r = RECIPES[s.kind];
    const rnd = rng(s.seed);
    const [c0, c1] = PALETTE[s.palette % PALETTE.length];
    const scale = Math.sqrt(s.size);
    const n = Math.round(r.stars * (0.6 + 0.4 * Math.min(s.size, 1.8)));
    const speed = r.radius * s.size * r.drag;
    // the flash: the sky lit for a moment
    this.put(s.at, [0, 0, 0], s.burst, 0.5, 1, 0, [1, 0.92, 0.8], [1, 0.8, 0.6], 260 * scale, FLASH, 0, 0.55 * Math.min(1.4, s.size));
    // a tilted ring plane
    const tilt = rnd() * Math.PI;
    const layers = s.kind === 'finale' ? 3 : s.kind === 'hero' ? 2 : 1;
    for (let layer = 0; layer < layers; layer++) {
      const lc0: RGB = layer === 1 ? [1, 1, 1] : layer === 2 ? PALETTE[2][0] : c0;
      const lc1: RGB = layer === 1 ? GOLD : layer === 2 ? PALETTE[2][1] : c1;
      const lspeed = speed * (1 - 0.3 * layer);
      for (let i = 0; i < n; i++) {
        let dx: number, dy: number, dz: number;
        if (r.ring) {
          const a = (i / n) * Math.PI * 2;
          dx = Math.cos(a);
          dy = Math.sin(a) * Math.cos(tilt);
          dz = Math.sin(a) * Math.sin(tilt);
        } else {
          // evenly spread on a sphere (Fibonacci), a little jitter
          const y = 1 - (2 * (i + 0.5)) / n;
          const rr = Math.sqrt(1 - y * y);
          const a = i * 2.399963 + rnd() * 0.3;
          dx = Math.cos(a) * rr;
          dy = y;
          dz = Math.sin(a) * rr;
        }
        const sp = lspeed * (0.85 + 0.25 * rnd());
        const v: V3 = [dx * sp, dy * sp, dz * sp];
        const life = r.life * (0.85 + 0.3 * rnd()) * (layer === 2 ? 0.7 : 1);
        const kind = layer === 2 ? GLITTER : r.kind;
        const seed = rnd() * 1000;
        this.put(s.at, v, s.burst, life, r.drag, r.gs, lc0, lc1, r.starSize * (0.8 + 0.4 * rnd()), kind, seed, 1);
        if (r.trail > 0 && rnd() < r.trail && layer === 0) {
          this.emitters.push({
            p0: s.at, v0: v, drag: r.drag, gs: r.gs, t0: s.burst, end: s.burst + life * 0.85, dt: TRAIL_DT,
            color: s.kind === 'willow' ? GOLD : lc0, life: r.trailLife, size: r.starSize * 0.45, seed, spread: 0.6,
          });
        }
      }
    }
  }

  private runEmitters(a: number, b: number): void {
    const keep: Emitter[] = [];
    const p = this.tmp;
    const v = this.tmpV;
    for (const e of this.emitters) {
      const from = Math.max(a, e.t0);
      const to = Math.min(b, e.end);
      // samples on the grid t0 + k dt in (from, to]
      let k = Math.floor((from - e.t0) / e.dt) + 1;
      for (let ts = e.t0 + k * e.dt; ts <= to; ts = e.t0 + ++k * e.dt) {
        const age = ts - e.t0;
        position(e.p0, e.v0, e.drag, e.gs, age, p);
        velocity(e.v0, e.drag, e.gs, age, v);
        const h = Math.sin(e.seed * 12.9898 + k * 78.233) * 43758.5453;
        const j = h - Math.floor(h);
        const s = e.spread;
        this.put(p, [v[0] * 0.03 + (j - 0.5) * s, v[1] * 0.03 + (j * 7 % 1 - 0.5) * s, v[2] * 0.03], ts, e.life * (0.7 + 0.6 * j), 2.5, 0.08, e.color, e.color, e.size, TRAIL, j * 100, 0.55);
      }
      if (e.end > b) keep.push(e);
    }
    this.emitters = keep;
  }
}

function lower<T>(a: T[], key: (x: T) => number, t: number): number {
  let lo = 0;
  let hi = a.length;
  while (lo < hi) {
    const m = (lo + hi) >> 1;
    if (key(a[m]) <= t) lo = m + 1;
    else hi = m;
  }
  return lo;
}
