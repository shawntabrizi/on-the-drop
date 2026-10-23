// IndexedDB: the library (tracks: record + analysis) and the user's labels,
// both keyed by the track key of the fingerprint record. Audio is never
// stored; it stays in the File the user picked.
import type { Analysis, TrackLabels, TrackRecord } from './types';

export interface StoredTrack {
  /** The track key (`record.key` of the first copy imported). */
  key: string;
  record: TrackRecord;
  /** The file name shown. */
  name: string;
  /** Every file name imported for this recording. */
  files: string[];
  /** The analysis (map, overview); the band bins are not kept. */
  analysis: Analysis;
  added: number;
  updated: number;
}

const DB_NAME = 'on-the-drop';
const DB_VERSION = 1;

function req<T>(r: IDBRequest<T>): Promise<T> {
  return new Promise((resolve, reject) => {
    r.onsuccess = () => resolve(r.result);
    r.onerror = () => reject(r.error);
  });
}

function done(tx: IDBTransaction): Promise<void> {
  return new Promise((resolve, reject) => {
    tx.oncomplete = () => resolve();
    tx.onerror = () => reject(tx.error);
    tx.onabort = () => reject(tx.error);
  });
}

export class Store {
  private constructor(private db: IDBDatabase) {}

  static async open(factory: IDBFactory = indexedDB, name = DB_NAME): Promise<Store> {
    const r = factory.open(name, DB_VERSION);
    r.onupgradeneeded = () => {
      const db = r.result;
      if (!db.objectStoreNames.contains('tracks')) db.createObjectStore('tracks', { keyPath: 'key' });
      if (!db.objectStoreNames.contains('labels')) db.createObjectStore('labels');
    };
    return new Store(await req(r));
  }

  close(): void {
    this.db.close();
  }

  async tracks(): Promise<StoredTrack[]> {
    const tx = this.db.transaction('tracks', 'readonly');
    return req(tx.objectStore('tracks').getAll() as IDBRequest<StoredTrack[]>);
  }

  async track(key: string): Promise<StoredTrack | undefined> {
    const tx = this.db.transaction('tracks', 'readonly');
    return req(tx.objectStore('tracks').get(key) as IDBRequest<StoredTrack | undefined>);
  }

  async putTrack(t: StoredTrack): Promise<void> {
    const tx = this.db.transaction('tracks', 'readwrite');
    tx.objectStore('tracks').put(t);
    await done(tx);
  }

  async deleteTrack(key: string): Promise<void> {
    const tx = this.db.transaction('tracks', 'readwrite');
    tx.objectStore('tracks').delete(key);
    await done(tx);
  }

  /** All labels: track key -> labels. */
  async labels(): Promise<Record<string, TrackLabels>> {
    const tx = this.db.transaction('labels', 'readonly');
    const s = tx.objectStore('labels');
    const [keys, values] = await Promise.all([req(s.getAllKeys()), req(s.getAll() as IDBRequest<TrackLabels[]>)]);
    const out: Record<string, TrackLabels> = {};
    keys.forEach((k, i) => (out[String(k)] = values[i]));
    return out;
  }

  /** Replaces all labels with `tracks` in one transaction. */
  async setLabels(tracks: Record<string, TrackLabels>): Promise<void> {
    const tx = this.db.transaction('labels', 'readwrite');
    const s = tx.objectStore('labels');
    s.clear();
    for (const [k, v] of Object.entries(tracks)) s.put(v, k);
    await done(tx);
  }

  /** The labels of one track (`undefined`: none, delete the entry). */
  async setTrackLabels(key: string, v: TrackLabels | undefined): Promise<void> {
    const tx = this.db.transaction('labels', 'readwrite');
    if (v) tx.objectStore('labels').put(v, key);
    else tx.objectStore('labels').delete(key);
    await done(tx);
  }
}
