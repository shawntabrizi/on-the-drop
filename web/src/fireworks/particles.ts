// All fireworks particles in one ring buffer of GPU points. The CPU writes a
// particle once, at its birth (start point, velocity, drag, colours, life);
// the vertex shader moves it with the closed-form drag ballistics of
// ballistics.ts, so nothing is simulated or re-uploaded per frame. The same
// geometry is drawn twice: the sky, and its reflection in the water
// (mirrored, dimmed, rippled).
import * as THREE from 'three';
import { GRAVITY } from './ballistics';

/** Particle kinds (the shader's `kind`). */
export const STAR = 0;
export const TRAIL = 1;
export const FLASH = 2;
export const COMET = 3;
export const GLITTER = 4;

const STRIDE = 20;

export interface Particle {
  p: [number, number, number];
  v: [number, number, number];
  t0: number;
  life: number;
  drag: number;
  gs: number;
  c0: [number, number, number];
  c1: [number, number, number];
  size: number;
  kind: number;
  seed: number;
  bright: number;
}

const vertex = /* glsl */ `
uniform float uTime;
uniform float uScale;
uniform float uMirror;
uniform float uFlash;
uniform float uAttack;
uniform float uStrobe;
attribute vec3 aP0;
attribute vec3 aV0;
attribute vec4 aTime;   // t0, life, drag, gravity scale
attribute vec3 aC0;
attribute vec3 aC1;
attribute vec4 aMisc;   // size, kind, seed, brightness
varying vec3 vColor;
varying float vSoft;

float hash(float n) { return fract(sin(n) * 43758.5453123); }

void main() {
  float age = uTime - aTime.x;
  float life = aTime.y;
  if (age < 0.0 || age > life) {
    gl_Position = vec4(2.0, 2.0, 2.0, 1.0);
    gl_PointSize = 0.0;
    return;
  }
  float k = aTime.z;
  float vty = -${GRAVITY.toFixed(1)} * aTime.w / k;
  float f = (1.0 - exp(-k * age)) / k;
  vec3 p = aP0 + aV0 * f;
  p.y += vty * age - vty * f;
  float u = age / life;
  float kind = aMisc.y;
  float seed = aMisc.z;
  float b = aMisc.w;
  vec3 c = mix(aC0, aC1, smoothstep(0.2, 0.7, u));
  float soft = 0.0;
  if (kind < 0.5) {
    // star: a short attack, a peak that softens, a colour change, then it
    // burns out: dims to an ember and flickers.
    float attack = clamp(age / uAttack, 0.0, 1.0);
    float peak = 1.0 + 1.6 * uFlash * exp(-age * 7.0);
    float out_ = 1.0 - smoothstep(0.72, 1.0, u);
    float flick = mix(1.0, 0.35 + 0.65 * step(0.45, hash(seed + floor(uTime * 22.0))), smoothstep(0.6, 0.85, u));
    c = mix(c, vec3(1.0, 0.32, 0.08), smoothstep(0.7, 1.0, u) * 0.8);
    b *= attack * peak * out_ * flick;
  } else if (kind < 1.5) {
    // trail: hangs where it was emitted, cools and fades
    c = mix(c, vec3(0.9, 0.25, 0.05), u);
    b *= (1.0 - u) * (1.0 - u);
  } else if (kind < 2.5) {
    // flash: the sky lit for a moment around a burst
    b *= uFlash * exp(-age * 9.0) * clamp(age / uAttack, 0.0, 1.0);
    soft = 1.0;
  } else if (kind < 3.5) {
    // comet: the rising shell
    b *= 0.8 + 0.2 * hash(seed + floor(uTime * 30.0));
  } else {
    // glitter: strobing points that fade
    float on = mix(0.6, step(0.55, hash(seed + floor(uTime * 26.0))), uStrobe);
    b *= on * (1.0 - smoothstep(0.6, 1.0, u)) * clamp(age / uAttack, 0.0, 1.0);
  }
  if (uMirror > 0.5) {
    if (p.y < 0.0) { gl_Position = vec4(2.0, 2.0, 2.0, 1.0); gl_PointSize = 0.0; return; }
    // the reflection: mirrored in the water plane, broken up by ripples.
    // Squashed towards the horizon (a painter's mirror: the true one of a
    // high burst falls below the bottom of the screen).
    float wob = sin(p.y * 0.09 + uTime * 1.3 + seed * 6.0) + 0.5 * sin(p.x * 0.05 - uTime * 0.9);
    p.x += wob * (1.5 + p.y * 0.02);
    p.y = -p.y * (0.42 + 0.02 * sin(uTime * 0.7 + p.x * 0.01));
    b *= 0.45;
  } else if (p.y < 0.0) {
    b = 0.0;
  }
  vec4 mv = modelViewMatrix * vec4(p, 1.0);
  gl_Position = projectionMatrix * mv;
  gl_PointSize = max(aMisc.x * uScale / -mv.z, 1.0);
  vColor = c * b;
  vSoft = soft;
}
`;

const fragment = /* glsl */ `
varying vec3 vColor;
varying float vSoft;
void main() {
  vec2 d = gl_PointCoord - 0.5;
  float r2 = dot(d, d) * 4.0;
  if (r2 > 1.0) discard;
  float core = exp(-r2 * 10.0);
  float glow = exp(-r2 * 3.5) * 0.45;
  float a = mix(core + glow, exp(-r2 * 2.5) * (1.0 - r2), vSoft);
  gl_FragColor = vec4(vColor * a, 1.0);
}
`;

export class Particles {
  readonly capacity: number;
  readonly sky: THREE.Points;
  readonly reflection: THREE.Points;
  private data: Float32Array;
  private buffer: THREE.InterleavedBuffer;
  private head = 0;
  private dirtyFrom = -1;
  private dirtyCount = 0;
  private skyMat: THREE.ShaderMaterial;
  private reflMat: THREE.ShaderMaterial;

  constructor(capacity = 1 << 18) {
    this.capacity = capacity;
    this.data = new Float32Array(capacity * STRIDE);
    this.buffer = new THREE.InterleavedBuffer(this.data, STRIDE);
    this.buffer.setUsage(THREE.DynamicDrawUsage);
    const g = new THREE.BufferGeometry();
    const attr = (name: string, size: number, offset: number) => g.setAttribute(name, new THREE.InterleavedBufferAttribute(this.buffer, size, offset));
    attr('position', 3, 0); // three needs a position attribute; aP0 is the same data
    attr('aP0', 3, 0);
    attr('aV0', 3, 3);
    attr('aTime', 4, 6);
    attr('aC0', 3, 10);
    attr('aC1', 3, 13);
    attr('aMisc', 4, 16);
    g.boundingSphere = new THREE.Sphere(new THREE.Vector3(), 1e6);
    g.setDrawRange(0, capacity);
    const uniforms = () => ({
      uTime: { value: 0 },
      uScale: { value: 400 },
      uMirror: { value: 0 },
      uFlash: { value: 1 },
      uAttack: { value: 0.02 },
      uStrobe: { value: 1 },
    });
    const mat = (mirror: boolean) =>
      new THREE.ShaderMaterial({
        uniforms: { ...uniforms(), uMirror: { value: mirror ? 1 : 0 } },
        vertexShader: vertex,
        fragmentShader: fragment,
        blending: THREE.AdditiveBlending,
        depthWrite: false,
        depthTest: false,
        transparent: true,
      });
    this.skyMat = mat(false);
    this.reflMat = mat(true);
    this.sky = new THREE.Points(g, this.skyMat);
    this.reflection = new THREE.Points(g, this.reflMat);
    this.sky.frustumCulled = false;
    this.reflection.frustumCulled = false;
    this.reflection.renderOrder = 1;
    this.sky.renderOrder = 2;
  }

  /** Song time, pixel scale (px per unit at distance 1), flash softening. */
  setFrame(time: number, scale: number, reduceFlashing: boolean): void {
    for (const m of [this.skyMat, this.reflMat]) {
      m.uniforms.uTime.value = time;
      m.uniforms.uScale.value = scale;
      m.uniforms.uFlash.value = reduceFlashing ? 0.3 : 1;
      m.uniforms.uAttack.value = reduceFlashing ? 0.12 : 0.02;
      m.uniforms.uStrobe.value = reduceFlashing ? 0 : 1;
    }
  }

  emit(q: Particle): void {
    const i = this.head;
    const o = i * STRIDE;
    const d = this.data;
    d[o] = q.p[0];
    d[o + 1] = q.p[1];
    d[o + 2] = q.p[2];
    d[o + 3] = q.v[0];
    d[o + 4] = q.v[1];
    d[o + 5] = q.v[2];
    d[o + 6] = q.t0;
    d[o + 7] = q.life;
    d[o + 8] = q.drag;
    d[o + 9] = q.gs;
    d[o + 10] = q.c0[0];
    d[o + 11] = q.c0[1];
    d[o + 12] = q.c0[2];
    d[o + 13] = q.c1[0];
    d[o + 14] = q.c1[1];
    d[o + 15] = q.c1[2];
    d[o + 16] = q.size;
    d[o + 17] = q.kind;
    d[o + 18] = q.seed;
    d[o + 19] = q.bright;
    if (this.dirtyFrom < 0) this.dirtyFrom = i;
    this.dirtyCount = Math.min(this.capacity, this.dirtyCount + 1);
    this.head = (i + 1) % this.capacity;
  }

  /** Everything gone (a seek). */
  clear(): void {
    this.data.fill(0);
    this.head = 0;
    this.dirtyFrom = 0;
    this.dirtyCount = this.capacity;
  }

  /** Uploads what was written since the last frame (one or two ranges). */
  flush(): void {
    if (this.dirtyFrom < 0) return;
    const b = this.buffer;
    b.clearUpdateRanges();
    const from = this.dirtyFrom;
    const n = this.dirtyCount;
    if (from + n <= this.capacity) {
      b.addUpdateRange(from * STRIDE, n * STRIDE);
    } else {
      b.addUpdateRange(from * STRIDE, (this.capacity - from) * STRIDE);
      b.addUpdateRange(0, (from + n - this.capacity) * STRIDE);
    }
    b.needsUpdate = true;
    this.dirtyFrom = -1;
    this.dirtyCount = 0;
  }
}
