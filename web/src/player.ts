// Playback of a decoded track with Web Audio: an AudioBufferSourceNode on
// the AudioContext clock (sample accurate, unlike <audio>.currentTime),
// an AnalyserNode for the live bands, and the latency offset.

export interface Bands {
  low: number;
  mid: number;
  high: number;
  level: number;
}

export class Player {
  readonly ctx: AudioContext;
  private analyser: AnalyserNode;
  private gain: GainNode;
  private buffer: AudioBuffer | null = null;
  private length = 0;
  private src: AudioBufferSourceNode | null = null;
  /** Song time at `startedAt` (context time) while playing; the position while paused. */
  private offset = 0;
  private startedAt = 0;
  private freq: Uint8Array<ArrayBuffer>;
  private smooth: Bands = { low: 0, mid: 0, high: 0, level: 0 };
  /** s, from the settings: positive = the visuals later. */
  userOffset = 0;
  onEnded: () => void = () => {};

  constructor() {
    this.ctx = new AudioContext({ latencyHint: 'interactive' });
    this.analyser = this.ctx.createAnalyser();
    this.analyser.fftSize = 2048;
    this.analyser.smoothingTimeConstant = 0.6;
    this.gain = this.ctx.createGain();
    this.analyser.connect(this.gain).connect(this.ctx.destination);
    this.freq = new Uint8Array(this.analyser.frequencyBinCount);
  }

  /** s: the buffer's length, else the analysed length (seek without audio). */
  get duration(): number {
    return this.buffer?.duration ?? this.length;
  }

  get playing(): boolean {
    return this.src !== null;
  }

  get loaded(): boolean {
    return this.buffer !== null;
  }

  load(b: AudioBuffer | null, length = 0): void {
    this.stop();
    this.buffer = b;
    this.length = length;
    this.offset = 0;
  }

  /** The song time being rendered now (no latency correction). */
  position(): number {
    if (!this.src) return this.offset;
    return Math.min(this.offset + this.ctx.currentTime - this.startedAt, this.duration);
  }

  /** The song time heard now, plus the user's offset: the visuals' clock. */
  visualTime(): number {
    if (!this.src) return this.offset - this.userOffset;
    const lat = (this.ctx.outputLatency || 0) + (this.ctx.baseLatency || 0);
    return this.position() - lat - this.userOffset;
  }

  async play(): Promise<void> {
    if (!this.buffer || this.src) return;
    if (this.ctx.state !== 'running') await this.ctx.resume();
    if (this.offset >= this.duration - 0.01) this.offset = 0;
    const src = this.ctx.createBufferSource();
    src.buffer = this.buffer;
    src.connect(this.analyser);
    src.onended = () => {
      if (this.src !== src) return;
      this.offset = this.duration;
      this.src = null;
      this.onEnded();
    };
    this.startedAt = this.ctx.currentTime;
    src.start(0, this.offset);
    this.src = src;
  }

  pause(): void {
    if (!this.src) return;
    this.offset = this.position();
    this.stop();
  }

  private stop(): void {
    const s = this.src;
    this.src = null;
    if (s) {
      s.onended = null;
      try {
        s.stop();
      } catch {
        /* not started */
      }
      s.disconnect();
    }
  }

  async toggle(): Promise<void> {
    if (this.playing) this.pause();
    else await this.play();
  }

  seek(t: number): void {
    const was = this.playing;
    this.stop();
    this.offset = Math.max(0, Math.min(t, this.duration));
    if (was) void this.play();
  }

  /** Live bands 0..1 (smoothed): low < 250 Hz, mid < 4 kHz, high above. */
  bands(): Bands {
    if (!this.src) {
      const s = this.smooth;
      s.low *= 0.95;
      s.mid *= 0.95;
      s.high *= 0.95;
      s.level *= 0.95;
      return s;
    }
    this.analyser.getByteFrequencyData(this.freq);
    const hz = this.ctx.sampleRate / this.analyser.fftSize;
    const band = (a: number, b: number) => {
      const i0 = Math.max(1, Math.floor(a / hz));
      const i1 = Math.min(this.freq.length, Math.ceil(b / hz));
      let s = 0;
      for (let i = i0; i < i1; i++) s += this.freq[i];
      return s / Math.max(1, i1 - i0) / 255;
    };
    const k = 0.25;
    const s = this.smooth;
    s.low += (band(20, 250) - s.low) * k;
    s.mid += (band(250, 4000) - s.mid) * k;
    s.high += (band(4000, 16000) - s.high) * k;
    s.level = (s.low + s.mid + s.high) / 3;
    return s;
  }
}
