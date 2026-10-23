// The timeline dock: the overview (whole song: waveform, drops with their
// strength, the finale, the playhead; a click seeks), the section strip
// (one button per section: its word, ✱ when the user changed it; a click
// opens the label menu) and the zoomed strip around the playhead (beats,
// downbeats, drops, sections).
import { BREAKDOWN, BUILD, CALM, DROP, type SongMap } from '../types';

export const SECTION_COLORS: Record<number, string> = {
  [CALM]: '#4c5d82',
  [BUILD]: '#d39a32',
  [DROP]: '#e5486a',
  [BREAKDOWN]: '#2fa59b',
};

export interface TimelineTrack {
  duration: number;
  levels: number[];
  bins: number;
  bin_s: number;
}

export interface TimelineEvents {
  seek(t: number): void;
  section(i: number, anchor: HTMLElement): void;
}

/** Seconds shown in the zoomed strip. */
const ZOOM_S = 10;

export function fmt(t: number): string {
  if (!Number.isFinite(t) || t < 0) t = 0;
  const m = Math.floor(t / 60);
  const s = Math.floor(t % 60);
  return `${m}:${s.toString().padStart(2, '0')}`;
}

export function sectionSpans(m: SongMap): { start: number; end: number; id: number }[] {
  const out = [];
  for (let k = 0; k < m.sections.length; k += 2) {
    out.push({ start: m.sections[k], end: k + 2 < m.sections.length ? m.sections[k + 2] : m.duration, id: m.sections[k + 1] });
  }
  return out;
}

export class Timeline {
  private overview: HTMLCanvasElement;
  private zoom: HTMLCanvasElement;
  private strip: HTMLElement;
  private wave: HTMLCanvasElement | null = null;
  private track: TimelineTrack | null = null;
  private map: SongMap | null = null;
  private user: boolean[] = [];
  private lastT = -1;
  private dirty = true;

  constructor(root: HTMLElement, private on: TimelineEvents) {
    this.overview = root.querySelector('canvas.overview')!;
    this.zoom = root.querySelector('canvas.zoom')!;
    this.strip = root.querySelector('.sections')!;
    this.overview.addEventListener('click', (e) => {
      if (!this.track) return;
      const r = this.overview.getBoundingClientRect();
      this.on.seek(((e.clientX - r.left) / r.width) * this.track.duration);
    });
    this.zoom.addEventListener('click', (e) => this.zoomClick(e));
    new ResizeObserver(() => {
      this.wave = null;
      this.dirty = true;
    }).observe(root);
  }

  setTrack(track: TimelineTrack | null, map: SongMap | null, user: boolean[]): void {
    this.track = track;
    this.wave = null;
    this.setMap(map, user);
  }

  setMap(map: SongMap | null, user: boolean[]): void {
    this.map = map;
    this.user = user;
    this.dirty = true;
    this.buildStrip();
  }

  /** Focus the strip button of section i (after the menu closes). */
  focusSection(i: number): void {
    (this.strip.children[i] as HTMLElement | undefined)?.focus();
  }

  private buildStrip(): void {
    this.strip.replaceChildren();
    const m = this.map;
    if (!m || !this.track) return;
    const d = this.track.duration;
    sectionSpans(m).forEach((s, i) => {
      const b = document.createElement('button');
      const word = m.words[i] ?? '';
      const mine = !!this.user[i];
      b.className = 'section' + (mine ? ' user' : '');
      b.style.left = `${(s.start / d) * 100}%`;
      b.style.width = `${((s.end - s.start) / d) * 100}%`;
      b.style.setProperty('--c', SECTION_COLORS[s.id] ?? '#555');
      b.textContent = mine ? `${word} ✱` : word;
      b.title = `${word}${mine ? ' (your label)' : ''} · ${fmt(s.start)}–${fmt(s.end)}`;
      b.setAttribute('aria-label', `Section ${i + 1}: ${word}, ${fmt(s.start)} to ${fmt(s.end)}${mine ? ', your label' : ''}. Change label`);
      b.setAttribute('aria-haspopup', 'menu');
      b.addEventListener('click', () => this.on.section(i, b));
      this.strip.append(b);
    });
  }

  private fit(c: HTMLCanvasElement): CanvasRenderingContext2D {
    const dpr = Math.min(window.devicePixelRatio || 1, 2);
    const w = Math.max(1, Math.round(c.clientWidth * dpr));
    const h = Math.max(1, Math.round(c.clientHeight * dpr));
    if (c.width !== w || c.height !== h) {
      c.width = w;
      c.height = h;
    }
    const g = c.getContext('2d')!;
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    return g;
  }

  /** Waveform of [t0, t1] into a w x h box (3 bands, mirrored). */
  private drawWave(g: CanvasRenderingContext2D, t0: number, t1: number, w: number, y: number, h: number): void {
    const tr = this.track!;
    const nb = tr.bins;
    const mid = y + h / 2;
    const bands: [number, string][] = [
      [0, 'rgba(70,110,235,0.95)'],
      [1, 'rgba(235,160,70,0.75)'],
      [2, 'rgba(225,235,250,0.6)'],
    ];
    for (const [band, color] of bands) {
      g.fillStyle = color;
      for (let x = 0; x < w; x++) {
        const a = Math.floor(((t0 + ((t1 - t0) * x) / w) / tr.bin_s));
        const b = Math.max(a + 1, Math.floor(((t0 + ((t1 - t0) * (x + 1)) / w) / tr.bin_s)));
        let v = 0;
        for (let i = Math.max(0, a); i < Math.min(nb, b); i++) v = Math.max(v, tr.levels[i * 3 + band]);
        const hh = (v / 255) * (h / 2) * (band === 0 ? 1 : band === 1 ? 0.8 : 0.55);
        if (hh > 0.3) g.fillRect(x, mid - hh, 1, hh * 2);
      }
    }
  }

  /** Draws for playhead t; cheap when nothing changed. */
  draw(t: number): void {
    if (!this.dirty && Math.abs(t - this.lastT) < 1e-4) return;
    this.lastT = t;
    this.dirty = false;
    this.drawOverview(t);
    this.drawZoom(t);
  }

  private drawOverview(t: number): void {
    const c = this.overview;
    const g = this.fit(c);
    const w = c.clientWidth;
    const h = c.clientHeight;
    g.clearRect(0, 0, w, h);
    const tr = this.track;
    const m = this.map;
    if (!tr) return;
    if (!this.wave || this.wave.width !== c.width) {
      const off = document.createElement('canvas');
      off.width = c.width;
      off.height = c.height;
      const og = off.getContext('2d')!;
      og.setTransform(c.width / w, 0, 0, c.height / h, 0, 0);
      this.drawWave(og, 0, tr.duration, w, 12, h - 14);
      this.wave = off;
    }
    g.drawImage(this.wave, 0, 0, w, h);
    const X = (s: number) => (s / tr.duration) * w;
    if (m) {
      const [fs, fe] = m.finale;
      if (fs >= 0 && fe > fs) {
        g.fillStyle = 'rgba(255,200,90,0.85)';
        g.fillRect(X(fs), 1, X(fe) - X(fs), 3);
      }
      m.drops.forEach((d, i) => {
        const s = m.drop_strength[i] ?? 0;
        const x = X(d);
        const th = 4 + 8 * s;
        g.fillStyle = '#ff5c8a';
        g.beginPath();
        g.moveTo(x - 4, 0);
        g.lineTo(x + 4, 0);
        g.lineTo(x, th);
        g.closePath();
        g.fill();
        g.fillStyle = 'rgba(255,92,138,0.5)';
        g.fillRect(x - 0.5, th, 1, h - th);
      });
    }
    // the zoomed window and the playhead
    const z0 = Math.max(0, Math.min(tr.duration - ZOOM_S, t - ZOOM_S / 2));
    g.fillStyle = 'rgba(255,255,255,0.07)';
    g.fillRect(X(z0), 0, X(z0 + ZOOM_S) - X(z0), h);
    g.fillStyle = '#fff';
    g.fillRect(X(t) - 1, 0, 2, h);
  }

  private zoomWindow(t: number): [number, number] {
    const d = this.track?.duration ?? 0;
    const t0 = d <= ZOOM_S ? 0 : Math.max(0, Math.min(d - ZOOM_S, t - ZOOM_S / 2));
    return [t0, t0 + ZOOM_S];
  }

  private drawZoom(t: number): void {
    const c = this.zoom;
    const g = this.fit(c);
    const w = c.clientWidth;
    const h = c.clientHeight;
    g.clearRect(0, 0, w, h);
    const tr = this.track;
    const m = this.map;
    if (!tr) return;
    const [t0, t1] = this.zoomWindow(t);
    const X = (s: number) => ((s - t0) / (t1 - t0)) * w;
    const stripH = 18;
    const waveH = h - stripH - 4;
    this.drawWave(g, t0, t1, w, 0, waveH);
    if (m) {
      // beats: downbeats tall and bright, the others short
      for (let i = 0; i < m.beats.length; i++) {
        const b = m.beats[i];
        if (b < t0 || b > t1) continue;
        const down = m.beat_bar[i] === 0;
        g.fillStyle = down ? 'rgba(255,255,255,0.85)' : 'rgba(255,255,255,0.35)';
        const bh = down ? waveH : waveH * 0.35;
        g.fillRect(X(b) - (down ? 1 : 0.5), waveH - bh, down ? 2 : 1, bh);
      }
      g.font = '600 11px system-ui, sans-serif';
      m.drops.forEach((d, i) => {
        if (d < t0 || d > t1) return;
        const x = X(d);
        g.fillStyle = '#ff5c8a';
        g.fillRect(x - 1.5, 0, 3, waveH);
        const label = `DROP ${(m.drop_strength[i] ?? 0).toFixed(2)}`;
        g.fillStyle = 'rgba(10,12,20,0.75)';
        g.fillRect(x + 4, 2, g.measureText(label).width + 8, 16);
        g.fillStyle = '#ff9ab6';
        g.fillText(label, x + 8, 14);
      });
      // section strip
      const y = h - stripH;
      g.font = '600 11px system-ui, sans-serif';
      sectionSpans(m).forEach((s, i) => {
        if (s.end < t0 || s.start > t1) return;
        const a = Math.max(0, X(s.start));
        const b = Math.min(w, X(s.end));
        g.fillStyle = SECTION_COLORS[s.id] ?? '#555';
        g.fillRect(a + 1, y, b - a - 2, stripH);
        const word = (m.words[i] ?? '') + (this.user[i] ? ' ✱' : '');
        g.save();
        g.beginPath();
        g.rect(a + 1, y, b - a - 2, stripH);
        g.clip();
        g.fillStyle = '#fff';
        g.fillText(word, a + 6, y + 13);
        g.restore();
      });
    }
    g.fillStyle = '#fff';
    g.fillRect(X(t) - 1, 0, 2, h);
  }

  private zoomClick(e: MouseEvent): void {
    if (!this.track) return;
    const r = this.zoom.getBoundingClientRect();
    const [t0, t1] = this.zoomWindow(this.lastT);
    const t = t0 + ((e.clientX - r.left) / r.width) * (t1 - t0);
    if (this.map && e.clientY - r.top > r.height - 20) {
      const i = sectionSpans(this.map).findIndex((s) => t >= s.start && t < s.end);
      const b = this.strip.children[i] as HTMLElement | undefined;
      if (i >= 0 && b) {
        this.on.section(i, b);
        return;
      }
    }
    this.on.seek(t);
  }
}
