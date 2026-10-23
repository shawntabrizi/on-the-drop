// The page: import (files, folders, drag and drop), the library, the
// timeline editor, playback and the fireworks. Everything runs locally.
import './style.css';
import * as otd from './otd';
import { Store, type StoredTrack } from './db';
import { decodeFile, isAudio, type Decoded } from './decode';
import { addAnalysed, exportLabels, exportMap, importLabels, labelState, safeName } from './library';
import { Player } from './player';
import { loadSettings, saveSettings } from './settings';
import { Fireworks } from './fireworks/scene';
import { schedule } from './fireworks/schedule';
import { closeMenu, menuOpen, openMenu } from './ui/menu';
import { fmt, sectionSpans, Timeline } from './ui/timeline';
import type { Analysis, LabelResult, TrackLabels, TrackRecord } from './types';
import type { WorkerMessage } from './worker';

const $ = <T extends HTMLElement = HTMLElement>(id: string) => document.getElementById(id) as T;

const settings = loadSettings();
const player = new Player();
player.userOffset = settings.latencyMs / 1000;

let store: Store;
let fireworks: Fireworks | null = null;
let current: StoredTrack | null = null;
let label: LabelResult | null = null;
/** The Files picked this visit (audio is never stored): track key -> file. */
const files = new Map<string, File>();

// --- worker ---------------------------------------------------------------

const worker = new Worker(new URL('./worker.ts', import.meta.url), { type: 'module' });
let nextId = 1;
const waiting = new Map<number, { stage(s: string): void; resolve(r: { record: TrackRecord; analysis: Analysis }): void; reject(e: Error): void }>();
worker.onmessage = (e: MessageEvent<WorkerMessage>) => {
  const m = e.data;
  const w = waiting.get(m.id);
  if (!w) return;
  if (m.type === 'progress') w.stage(m.stage);
  else {
    waiting.delete(m.id);
    if (m.type === 'done') w.resolve({ record: JSON.parse(m.record), analysis: JSON.parse(m.analysis) });
    else w.reject(new Error(m.message));
  }
};

function analyseInWorker(d: Decoded, stage: (s: string) => void): Promise<{ record: TrackRecord; analysis: Analysis }> {
  const id = nextId++;
  return new Promise((resolve, reject) => {
    waiting.set(id, { stage, resolve, reject });
    worker.postMessage({ id, pcm: d.pcm, channels: d.channels, rate: d.buffer.sampleRate }, [d.pcm.buffer]);
  });
}

// --- import queue -----------------------------------------------------------

interface Job {
  id: number;
  file: File;
  state: 'queued' | 'decoding' | 'fingerprint' | 'analyse' | 'error';
  error?: string;
}

const jobs: Job[] = [];
let jobSeq = 0;
let running = false;
let batchTotal = 0;
let batchDone = 0;

function enqueue(list: File[]): void {
  const audio = list.filter(isAudio);
  if (!audio.length) {
    status(list.length ? 'No audio files in that selection.' : '');
    return;
  }
  for (const file of audio) jobs.push({ id: ++jobSeq, file, state: 'queued' });
  batchTotal += audio.length;
  renderLibrary();
  void pump();
}

const STAGE_SHARE: Record<string, number> = { queued: 0, decoding: 0.05, fingerprint: 0.45, analyse: 0.65 };

function showProgress(j: Job | null): void {
  const box = $('progress');
  if (!j) {
    box.hidden = true;
    return;
  }
  box.hidden = false;
  const words: Record<string, string> = { queued: 'Waiting', decoding: 'Decoding', fingerprint: 'Fingerprinting', analyse: 'Analysing', error: 'Failed' };
  $('progressText').textContent = `${words[j.state]} ${batchDone + 1} of ${batchTotal}: ${j.file.name}`;
  const share = (batchDone + (STAGE_SHARE[j.state] ?? 0)) / Math.max(1, batchTotal);
  $('progressBar').style.width = `${Math.round(share * 100)}%`;
}

async function pump(): Promise<void> {
  if (running) return;
  running = true;
  while (jobs.some((j) => j.state === 'queued')) {
    const j = jobs.find((x) => x.state === 'queued')!;
    const set = (s: Job['state']) => {
      j.state = s;
      showProgress(j);
      renderLibrary();
    };
    try {
      set('decoding');
      const d = await decodeFile(j.file);
      const buffer = d.buffer;
      set('fingerprint');
      const r = await analyseInWorker(d, (s) => set(s as Job['state']));
      const { track, known } = await addAnalysed(store, r.record, r.analysis, j.file.name);
      files.set(track.key, j.file);
      jobs.splice(jobs.indexOf(j), 1);
      if (!current || current.key === track.key) await openTrack(track.key, buffer);
      if (known) status(`${j.file.name}: a known recording; its labels are kept.`);
    } catch (e) {
      j.state = 'error';
      j.error = e instanceof Error ? e.message : String(e);
    }
    batchDone++;
    renderLibrary();
  }
  running = false;
  batchTotal = batchDone = 0;
  showProgress(null);
}

// --- library --------------------------------------------------------------

async function renderLibrary(): Promise<void> {
  const list = $('tracks');
  const tracks = (await store.tracks()).sort((a, b) => a.name.localeCompare(b.name));
  const labels = await store.labels();
  list.replaceChildren();
  for (const j of jobs) {
    const li = document.createElement('li');
    const b = document.createElement('button');
    b.disabled = true;
    const words: Record<string, string> = { queued: 'Queued', decoding: 'Decoding', fingerprint: 'Fingerprint', analyse: 'Analysing', error: 'Error' };
    b.innerHTML = `<span class="name"></span><span class="chip ${j.state === 'error' ? 'error' : 'work'}">${words[j.state]}</span><span class="sub"></span>`;
    b.querySelector('.name')!.textContent = j.file.name;
    b.querySelector('.sub')!.textContent = j.error ?? 'Local analysis';
    li.append(b);
    list.append(li);
  }
  for (const t of tracks) {
    const li = document.createElement('li');
    const b = document.createElement('button');
    const n = labels[t.key]?.labels.length ?? 0;
    const playable = files.has(t.key);
    b.setAttribute('aria-current', String(current?.key === t.key));
    b.innerHTML = `<span class="name"></span><span class="chip ${playable ? 'ready' : ''}">${playable ? 'Ready' : 'Analysed'}</span><span class="sub"></span>`;
    b.querySelector('.name')!.textContent = t.name;
    const sub = [`${Math.round(t.analysis.bpm)} BPM`, fmt(t.analysis.duration), `${t.analysis.drops.length} drop${t.analysis.drops.length === 1 ? '' : 's'}`];
    if (n) sub.push(`${n} label${n === 1 ? '' : 's'} ✱`);
    if (t.files.length > 1) sub.push(`${t.files.length} files`);
    b.querySelector('.sub')!.textContent = sub.join(' · ');
    b.title = playable ? t.name : `${t.name}: add the file again to play it (audio is not stored)`;
    b.addEventListener('click', () => {
      document.body.classList.remove('library-open');
      $('libraryToggle').setAttribute('aria-expanded', 'false');
      void openTrack(t.key);
    });
    li.append(b);
    list.append(li);
  }
  $('libCount').textContent = tracks.length ? `${tracks.length} track${tracks.length === 1 ? '' : 's'}` : '';
  $('empty').hidden = tracks.length > 0 || jobs.length > 0;
}

// --- the open track -----------------------------------------------------------

let loadSeq = 0;

async function openTrack(key: string, buffer?: AudioBuffer): Promise<void> {
  const seq = ++loadSeq;
  const t = await store.track(key);
  if (!t) return;
  current = t;
  label = await labelState(store, t);
  player.load(null, t.analysis.duration);
  $('trackName').textContent = t.name;
  $('bpm').textContent = `${Math.round(t.analysis.bpm)} BPM`;
  (document.getElementById('exportMap') as HTMLButtonElement).disabled = false;
  timeline.setTrack({ duration: t.analysis.duration, levels: t.analysis.levels, bins: t.analysis.bins, bin_s: t.analysis.bin_s }, label.map, label.user);
  restartShow();
  renderLibrary();
  const play = $('play') as HTMLButtonElement;
  play.disabled = true;
  if (!buffer) {
    const f = files.get(key);
    if (f) {
      status('Decoding for playback…');
      try {
        buffer = (await decodeFile(f)).buffer;
      } catch (e) {
        status(`Cannot decode ${f.name}: ${e instanceof Error ? e.message : e}`);
      }
    }
  }
  if (seq !== loadSeq) return;
  if (buffer) {
    player.load(buffer, t.analysis.duration);
    play.disabled = false;
    status('');
  } else {
    status('Analysis loaded. Add the file again to play it: audio is not stored.');
  }
  updatePlayButton();
}

/** The open track's labels again (after an import); playback goes on. */
async function refreshLabels(): Promise<void> {
  if (!current) return;
  const t = current;
  const r = await labelState(store, t);
  if (current !== t) return;
  label = r;
  timeline.setMap(r.map, r.user);
  restartShow();
}

function restartShow(): void {
  if (!fireworks) return;
  fireworks.show.setShells(label ? schedule(label.map) : [], player.visualTime());
}

function relabel(i: number, word: string): void {
  if (!current) return;
  const t = current;
  void labelState(store, t, i, word).then((r) => {
    if (current !== t) return;
    label = r;
    timeline.setMap(r.map, r.user);
    restartShow();
    renderLibrary();
    timeline.focusSection(i);
  });
}

const timeline = new Timeline($('dock'), {
  seek: (t) => {
    player.seek(t);
  },
  section: (i, anchor) => {
    if (!label) return;
    const s = sectionSpans(label.map)[i];
    openMenu(anchor, {
      title: `Section ${i + 1} · ${fmt(s.start)}–${fmt(s.end)}`,
      words: label.map.vocabulary,
      auto: label.auto_words[i],
      current: label.map.words[i],
      user: label.user[i],
      pick: (w) => relabel(i, w),
      reset: () => relabel(i, ''),
      closed: () => timeline.focusSection(i),
    });
  },
});

// --- playback ---------------------------------------------------------------

function updatePlayButton(): void {
  const b = $('play');
  b.textContent = player.playing ? '❚❚' : '▶';
  b.setAttribute('aria-label', player.playing ? 'Pause' : 'Play');
}

player.onEnded = updatePlayButton;

async function togglePlay(): Promise<void> {
  if (!player.loaded) return;
  if (!player.playing && !settings.noticeSeen) await showNotice();
  await player.toggle();
  updatePlayButton();
}

/** Seeks to the previous or next downbeat (one bar). */
function seekBar(dir: -1 | 1): void {
  if (!label) return;
  const t = player.position();
  const m = label.map;
  const downs = m.beats.filter((_, i) => m.beat_bar[i] === 0);
  const eps = player.playing ? 0.25 : 0.01;
  const next = dir > 0 ? downs.find((d) => d > t + 0.01) : [...downs].reverse().find((d) => d < t - eps);
  player.seek(next ?? (dir > 0 ? m.duration : 0));
}

// --- files and drag and drop ----------------------------------------------------

async function entryFiles(entry: FileSystemEntry): Promise<File[]> {
  if (entry.isFile) {
    return new Promise((res) => (entry as FileSystemFileEntry).file((f) => res([f]), () => res([])));
  }
  if (entry.isDirectory) {
    const reader = (entry as FileSystemDirectoryEntry).createReader();
    const out: File[] = [];
    for (;;) {
      const batch = await new Promise<FileSystemEntry[]>((res) => reader.readEntries(res, () => res([])));
      if (!batch.length) break;
      for (const e of batch) out.push(...(await entryFiles(e)));
    }
    return out;
  }
  return [];
}

function download(text: string, name: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: 'application/json' }));
  const a = document.createElement('a');
  a.href = url;
  a.download = name;
  document.body.append(a);
  a.click();
  a.remove();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}

let statusTimer = 0;
function status(s: string): void {
  $('status').textContent = s;
  clearTimeout(statusTimer);
  if (s) statusTimer = window.setTimeout(() => ($('status').textContent = ''), 8000);
}

// --- settings and the notice -------------------------------------------------------

function applySettings(): void {
  ($('reduceFlashing') as HTMLInputElement).checked = settings.reduceFlashing;
  ($('noticeReduce') as HTMLInputElement).checked = settings.reduceFlashing;
  ($('latency') as HTMLInputElement).value = String(settings.latencyMs);
  $('latencyOut').textContent = `${settings.latencyMs > 0 ? '+' : ''}${settings.latencyMs} ms`;
  player.userOffset = settings.latencyMs / 1000;
  saveSettings(settings);
}

function showNotice(): Promise<void> {
  const d = $('notice') as HTMLDialogElement;
  return new Promise((resolve) => {
    d.addEventListener(
      'close',
      () => {
        settings.noticeSeen = true;
        settings.reduceFlashing = ($('noticeReduce') as HTMLInputElement).checked;
        applySettings();
        resolve();
      },
      { once: true },
    );
    d.showModal();
  });
}

// --- events ---------------------------------------------------------------------

function wire(): void {
  const pick = $('pickFiles') as HTMLInputElement;
  const folder = $('pickFolder') as HTMLInputElement;
  const labelsIn = $('pickLabels') as HTMLInputElement;
  pick.addEventListener('change', () => {
    enqueue([...(pick.files ?? [])]);
    pick.value = '';
  });
  folder.addEventListener('change', () => {
    enqueue([...(folder.files ?? [])]);
    folder.value = '';
  });
  // the <label> buttons open the pickers from the keyboard too
  for (const l of document.querySelectorAll<HTMLLabelElement>('label.btn[for]')) {
    l.tabIndex = 0;
    l.setAttribute('role', 'button');
    l.addEventListener('keydown', (e) => {
      if (e.key === 'Enter' || e.key === ' ') {
        e.preventDefault();
        e.stopPropagation();
        $(l.htmlFor).click();
      }
    });
  }
  $('importLabels').addEventListener('click', () => labelsIn.click());
  labelsIn.addEventListener('change', async () => {
    const f = labelsIn.files?.[0];
    labelsIn.value = '';
    if (!f) return;
    try {
      const r = await importLabels(store, await f.text());
      await refreshLabels();
      renderLibrary();
      status(`Labels imported: ${r.merged} merged with known recordings, ${r.added} new.`);
    } catch (e) {
      status(`Not a labels file: ${e instanceof Error ? e.message : e}`);
    }
  });
  $('exportLabels').addEventListener('click', async () => download(await exportLabels(store), 'on-the-drop-labels.json'));
  $('exportMap').addEventListener('click', () => {
    if (current && label) download(exportMap(current, label.map), `${safeName(current.name)}.map.json`);
  });
  $('play').addEventListener('click', () => void togglePlay());
  const immersive = () => {
    const on = document.body.classList.toggle('immersive');
    $('immersive').setAttribute('aria-pressed', String(on));
    fireworks?.resize();
  };
  $('immersive').addEventListener('click', immersive);
  $('immersiveExit').addEventListener('click', immersive);
  $('libraryToggle').addEventListener('click', () => {
    const open = document.body.classList.toggle('library-open');
    $('libraryToggle').setAttribute('aria-expanded', String(open));
  });
  $('openSettings').addEventListener('click', () => ($('settings') as HTMLDialogElement).showModal());
  $('reduceFlashing').addEventListener('change', (e) => {
    settings.reduceFlashing = (e.target as HTMLInputElement).checked;
    applySettings();
  });
  $('latency').addEventListener('input', (e) => {
    settings.latencyMs = Number((e.target as HTMLInputElement).value);
    applySettings();
  });
  $('clearLibrary').addEventListener('click', async () => {
    if (!confirm('Remove all analysed tracks and labels from this browser? Export your labels first if you want to keep them.')) return;
    for (const t of await store.tracks()) await store.deleteTrack(t.key);
    await store.setLabels({} as Record<string, TrackLabels>);
    files.clear();
    current = null;
    label = null;
    player.load(null, 0);
    timeline.setTrack(null, null, []);
    restartShow();
    $('trackName').textContent = 'No track';
    renderLibrary();
  });

  // drag and drop: files and folders
  let depth = 0;
  window.addEventListener('dragenter', (e) => {
    if (!e.dataTransfer?.types.includes('Files')) return;
    depth++;
    $('dropOverlay').hidden = false;
  });
  window.addEventListener('dragleave', () => {
    if (--depth <= 0) {
      depth = 0;
      $('dropOverlay').hidden = true;
    }
  });
  window.addEventListener('dragover', (e) => e.preventDefault());
  window.addEventListener('drop', async (e) => {
    e.preventDefault();
    depth = 0;
    $('dropOverlay').hidden = true;
    const items = [...(e.dataTransfer?.items ?? [])];
    const entries = items.map((i) => i.webkitGetAsEntry?.()).filter((x): x is FileSystemEntry => !!x);
    const list = entries.length ? (await Promise.all(entries.map(entryFiles))).flat() : [...(e.dataTransfer?.files ?? [])];
    enqueue(list);
  });

  // keyboard: Space play/pause, arrows a bar, V visual only
  const typing = (t: EventTarget | null) => t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement || t instanceof HTMLSelectElement;
  window.addEventListener('keydown', (e) => {
    if (menuOpen() || document.querySelector('dialog[open]') || typing(e.target) || e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === ' ') {
      e.preventDefault();
      if (!e.repeat) void togglePlay();
    } else if (e.key === 'ArrowLeft' || e.key === 'ArrowRight') {
      e.preventDefault();
      seekBar(e.key === 'ArrowLeft' ? -1 : 1);
    } else if (e.key === 'v' || e.key === 'V') {
      immersive();
    } else if (e.key === 'Escape') {
      closeMenu();
    }
  });
  // a Space keyup must not also press the focused button
  window.addEventListener('keyup', (e) => {
    if (e.key === ' ' && !typing(e.target) && !menuOpen() && !document.querySelector('dialog[open]')) e.preventDefault();
  });
  window.addEventListener('resize', () => fireworks?.resize());
}

// --- the frame loop ------------------------------------------------------------------

let lastClock = '';
let lastSection = '';
function frame(now: number): void {
  const t = player.visualTime();
  const heard = t + player.userOffset;
  try {
    fireworks?.render(t, now / 1000, player.bands(), settings.reduceFlashing);
  } catch (e) {
    console.error(e);
    fireworks = null;
  }
  timeline.draw(Math.max(0, heard));
  if (label) {
    const clock = `${fmt(heard)} / ${fmt(player.duration)}`;
    if (clock !== lastClock) {
      lastClock = clock;
      $('clock').textContent = clock;
    }
    const i = sectionSpans(label.map).findIndex((s) => heard >= s.start && heard < s.end);
    const word = i >= 0 ? `${label.map.words[i]}${label.user[i] ? ' ✱' : ''}` : '';
    if (word !== lastSection) {
      lastSection = word;
      $('nowSection').textContent = word;
    }
  }
  requestAnimationFrame(frame);
}

async function main(): Promise<void> {
  wire();
  applySettings();
  try {
    fireworks = new Fireworks($('stage') as HTMLCanvasElement);
  } catch (e) {
    console.error(e);
    status('WebGL is not available: the editor works, the fireworks do not.');
  }
  await otd.ready();
  store = await Store.open();
  await renderLibrary();
  requestAnimationFrame(frame);
}

void main();
