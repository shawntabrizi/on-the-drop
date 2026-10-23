// Per-browser settings (localStorage; the page works without it).

export interface Settings {
  reduceFlashing: boolean;
  /** ms; positive = the visuals later. */
  latencyMs: number;
  /** The photosensitivity notice was read. */
  noticeSeen: boolean;
}

const KEY = 'on-the-drop/settings';
const DEFAULTS: Settings = { reduceFlashing: false, latencyMs: 0, noticeSeen: false };

export function loadSettings(): Settings {
  try {
    const s = JSON.parse(localStorage.getItem(KEY) ?? '{}') as Partial<Settings>;
    return { ...DEFAULTS, ...s };
  } catch {
    return { ...DEFAULTS };
  }
}

export function saveSettings(s: Settings): void {
  try {
    localStorage.setItem(KEY, JSON.stringify(s));
  } catch {
    /* private mode: the settings last this visit */
  }
}
