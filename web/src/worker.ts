// Analysis worker: fingerprint and analysis of decoded PCM, off the main
// thread. The PCM arrives transferred and never leaves this browser.
import * as otd from './otd';

export interface AnalyseRequest {
  id: number;
  pcm: Float32Array;
  channels: number;
  rate: number;
}

export type WorkerMessage =
  | { id: number; type: 'progress'; stage: 'fingerprint' | 'analyse' }
  | { id: number; type: 'done'; record: string; analysis: string }
  | { id: number; type: 'error'; message: string };

const post = (m: WorkerMessage) => (self as unknown as Worker).postMessage(m);

self.onmessage = async (e: MessageEvent<AnalyseRequest>) => {
  const { id, pcm, channels, rate } = e.data;
  try {
    await otd.ready();
    post({ id, type: 'progress', stage: 'fingerprint' });
    const record = otd.fingerprint(pcm, channels, rate);
    post({ id, type: 'progress', stage: 'analyse' });
    const analysis = otd.analyze(pcm, channels, rate);
    if (!record || !analysis) throw new Error('the audio is too short to analyse');
    post({ id, type: 'done', record: JSON.stringify(record), analysis: JSON.stringify(analysis) });
  } catch (err) {
    post({ id, type: 'error', message: err instanceof Error ? err.message : String(err) });
  }
};
