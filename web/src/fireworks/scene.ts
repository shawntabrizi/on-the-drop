// The one scene: fireworks over water at night. A still camera at the
// water's edge; one full-screen pass draws the sky (gradient, stars, haze),
// the far hills and the water; then the particles' reflection, then the
// particles. The live bands only warm the haze and the water's glints,
// slowly: nothing in the sky moves on a beat.
import * as THREE from 'three';
import type { Bands } from '../player';
import { Particles } from './particles';
import { BARGE_Z, SKY_HALF_WIDTH } from './schedule';
import { Show } from './show';

const backdropVertex = /* glsl */ `
varying vec2 vNdc;
void main() {
  vNdc = position.xy;
  gl_Position = vec4(position.xy, 0.9999, 1.0);
}
`;

const backdropFragment = /* glsl */ `
uniform mat4 uInvProj;
uniform mat4 uCamWorld;
uniform float uTime;
uniform float uHaze;
uniform float uGlint;
varying vec2 vNdc;

float hash(vec2 p) { return fract(sin(dot(p, vec2(127.1, 311.7))) * 43758.5453); }
float noise(float x) {
  float i = floor(x);
  float f = fract(x);
  return mix(hash(vec2(i, 1.7)), hash(vec2(i + 1.0, 1.7)), f * f * (3.0 - 2.0 * f));
}

vec3 sky(vec3 d) {
  float h = max(d.y, 0.0);
  vec3 top = vec3(0.010, 0.016, 0.045);
  vec3 low = vec3(0.045, 0.050, 0.100);
  vec3 c = mix(low, top, pow(h, 0.45));
  // haze over the horizon, warmed by the music
  c += vec3(0.10, 0.055, 0.07) * exp(-h * 9.0) * (0.35 + uHaze);
  // stars
  vec2 g = vec2(atan(d.x, -d.z) * 160.0, h * 260.0);
  vec2 cell = floor(g);
  float s = hash(cell);
  if (s > 0.985 && h > 0.04) {
    vec2 f = fract(g) - 0.5;
    float tw = 0.6 + 0.4 * sin(uTime * (1.0 + s * 3.0) + s * 40.0);
    c += vec3(0.75, 0.8, 1.0) * smoothstep(0.18, 0.0, length(f)) * (s - 0.985) * 60.0 * tw * smoothstep(0.04, 0.2, h);
  }
  return c;
}

void main() {
  vec4 v = uInvProj * vec4(vNdc, 1.0, 1.0);
  vec3 dir = normalize((uCamWorld * vec4(v.xyz / v.w, 0.0)).xyz);
  float az = atan(dir.x, -dir.z);
  // far hills: a ridge a little above the horizon
  float ridge = 0.012 + 0.03 * noise(az * 9.0) + 0.012 * noise(az * 31.0);
  vec3 c;
  if (dir.y > 0.0) {
    c = dir.y < ridge ? vec3(0.012, 0.014, 0.024) + vec3(0.03, 0.02, 0.03) * uHaze * (1.0 - dir.y / ridge) : sky(dir);
  } else {
    // water: the sky mirrored, darker, with ripples and glints
    vec3 r = normalize(vec3(dir.x, -dir.y / 0.42, dir.z)); // squashed as the particles' mirror
    float depth = -dir.y;
    float ripple = sin(depth * 900.0 + uTime * 1.5 + sin(az * 40.0) * 2.0) * 0.5 + 0.5;
    r.x += (ripple - 0.5) * 0.004;
    vec3 refl = r.y < ridge ? vec3(0.01, 0.012, 0.02) : sky(r);
    float fres = 0.25 + 0.6 * pow(1.0 - clamp(depth * 6.0, 0.0, 1.0), 3.0);
    c = vec3(0.004, 0.008, 0.016) + refl * fres;
    float gl = step(0.985, hash(floor(vec2(az * 600.0, depth * 2400.0 + uTime * 3.0))));
    c += vec3(0.25, 0.3, 0.4) * gl * uGlint * smoothstep(0.0, 0.05, depth) * (1.0 - smoothstep(0.1, 0.4, depth));
  }
  gl_FragColor = vec4(c, 1.0);
}
`;

export class Fireworks {
  readonly renderer: THREE.WebGLRenderer;
  readonly show: Show;
  private scene = new THREE.Scene();
  private camera: THREE.PerspectiveCamera;
  private particles: Particles;
  private backdrop: THREE.ShaderMaterial;
  private haze = 0;
  private glint = 0;

  constructor(private canvas: HTMLCanvasElement) {
    this.renderer = new THREE.WebGLRenderer({ canvas, antialias: false, alpha: false, powerPreference: 'high-performance' });
    this.renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
    this.renderer.outputColorSpace = THREE.LinearSRGBColorSpace;
    this.camera = new THREE.PerspectiveCamera(55, 1, 1, 5000);
    // The camera never moves: at the water's edge, looking at the barge.
    this.camera.position.set(0, 8, 0);
    this.camera.lookAt(0, 95, BARGE_Z);
    this.backdrop = new THREE.ShaderMaterial({
      uniforms: {
        uInvProj: { value: new THREE.Matrix4() },
        uCamWorld: { value: new THREE.Matrix4() },
        uTime: { value: 0 },
        uHaze: { value: 0 },
        uGlint: { value: 0 },
      },
      vertexShader: backdropVertex,
      fragmentShader: backdropFragment,
      depthWrite: false,
      depthTest: false,
    });
    const quad = new THREE.Mesh(new THREE.PlaneGeometry(2, 2), this.backdrop);
    quad.frustumCulled = false;
    quad.renderOrder = 0;
    this.scene.add(quad);
    this.particles = new Particles();
    this.scene.add(this.particles.reflection, this.particles.sky);
    this.show = new Show(this.particles);
    this.resize();
  }

  /** Fits the view: the whole sky width stays in view on a narrow screen. */
  resize(): void {
    const w = this.canvas.clientWidth || window.innerWidth;
    const h = this.canvas.clientHeight || window.innerHeight;
    this.renderer.setSize(w, h, false);
    const aspect = w / Math.max(1, h);
    const dist = -BARGE_Z;
    const needHalf = (SKY_HALF_WIDTH + 60) / dist;
    const fov = Math.max(55, Math.min(100, (2 * Math.atan(needHalf / aspect) * 180) / Math.PI));
    this.camera.aspect = aspect;
    this.camera.fov = fov;
    this.camera.updateProjectionMatrix();
    this.camera.updateMatrixWorld();
    this.backdrop.uniforms.uInvProj.value.copy(this.camera.projectionMatrixInverse);
    this.backdrop.uniforms.uCamWorld.value.copy(this.camera.matrixWorld);
  }

  /** Draws song time `t` (seconds; the clock of the show). */
  render(t: number, wall: number, bands: Bands, reduceFlashing: boolean): void {
    // slow followers: the haze and glints swell with a part, not a beat
    this.haze += (bands.low * 0.9 + bands.mid * 0.3 - this.haze) * 0.02;
    this.glint += (bands.high * 1.5 - this.glint) * 0.03;
    const u = this.backdrop.uniforms;
    u.uTime.value = wall;
    u.uHaze.value = Math.min(1, this.haze) * (reduceFlashing ? 0.6 : 1);
    u.uGlint.value = Math.min(1, this.glint);
    this.show.update(t);
    // px per world unit at distance 1 (drawing-buffer pixels)
    const scale = this.renderer.domElement.height / (2 * Math.tan((this.camera.fov * Math.PI) / 360));
    this.particles.setFrame(t, scale, reduceFlashing);
    this.renderer.render(this.scene, this.camera);
  }

  dispose(): void {
    this.renderer.dispose();
  }
}
