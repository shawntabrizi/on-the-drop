// Ballistics with linear drag, in closed form (the star shader uses the same
// formula, so the CPU knows where every star is without simulating):
//   dv/dt = g - k v   =>   v(a) = vt + (v0 - vt) e^(-k a),  vt = g / k
//   p(a) = p0 + vt a + (v0 - vt) (1 - e^(-k a)) / k
// World units are metres-ish; gravity is stylised (shells 2-4 s in flight).

export const GRAVITY = 30;
/** Drag of a rising shell (1/s). */
export const SHELL_DRAG = 0.25;

export type V3 = [number, number, number];

/** Position at age `a` of a body launched at p0 with v0, drag k, gravity scale gs. */
export function position(p0: V3, v0: V3, k: number, gs: number, a: number, out: V3 = [0, 0, 0]): V3 {
  const vty = (-GRAVITY * gs) / k;
  const f = (1 - Math.exp(-k * a)) / k;
  out[0] = p0[0] + v0[0] * f;
  out[1] = p0[1] + vty * a + (v0[1] - vty) * f;
  out[2] = p0[2] + v0[2] * f;
  return out;
}

export function velocity(v0: V3, k: number, gs: number, a: number, out: V3 = [0, 0, 0]): V3 {
  const vty = (-GRAVITY * gs) / k;
  const e = Math.exp(-k * a);
  out[0] = v0[0] * e;
  out[1] = vty + (v0[1] - vty) * e;
  out[2] = v0[2] * e;
  return out;
}

/** The launch velocity that puts a body from p0 at `target` after `t` s. */
export function solveLaunch(p0: V3, target: V3, t: number, k: number, gs = 1): V3 {
  const vty = (-GRAVITY * gs) / k;
  const f = (1 - Math.exp(-k * t)) / k;
  return [(target[0] - p0[0]) / f, vty + (target[1] - p0[1] - vty * t) / f, (target[2] - p0[2]) / f];
}

/** Height (above the launch) of the apex reached after exactly `t` s. */
export function apexHeight(t: number, k = SHELL_DRAG): number {
  const vt = GRAVITY / k;
  const vy0 = vt * (Math.exp(k * t) - 1);
  return -vt * t + (vy0 + vt) * ((1 - Math.exp(-k * t)) / k);
}
