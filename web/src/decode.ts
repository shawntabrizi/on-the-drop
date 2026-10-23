// Decoding with Web Audio, at the analysis rate. Local only.

/** The rate the analysis and the fingerprint see (the Godot app decodes at 44.1 kHz). */
export const ANALYSIS_RATE = 44100;

export interface Decoded {
  buffer: AudioBuffer;
  /** Interleaved PCM: one channel (mono files) or the first two. */
  pcm: Float32Array;
  channels: number;
}

export async function decodeFile(file: Blob): Promise<Decoded> {
  const bytes = await file.arrayBuffer();
  const ctx = new OfflineAudioContext(2, 1, ANALYSIS_RATE);
  const buffer = await ctx.decodeAudioData(bytes);
  return { buffer, ...interleave(buffer) };
}

export function interleave(b: AudioBuffer): { pcm: Float32Array; channels: number } {
  if (b.numberOfChannels === 1) return { pcm: b.getChannelData(0).slice(), channels: 1 };
  const l = b.getChannelData(0);
  const r = b.getChannelData(1);
  const pcm = new Float32Array(l.length * 2);
  for (let i = 0; i < l.length; i++) {
    pcm[2 * i] = l[i];
    pcm[2 * i + 1] = r[i];
  }
  return { pcm, channels: 2 };
}

const AUDIO_EXT = /\.(mp3|wav|wave|flac|ogg|oga|opus|m4a|mp4|aac|webm|aif|aiff|caf)$/i;

export function isAudio(f: File): boolean {
  return f.type.startsWith('audio/') || AUDIO_EXT.test(f.name);
}
