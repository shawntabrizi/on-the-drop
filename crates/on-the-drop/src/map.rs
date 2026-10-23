//! The song map: one whole-song analysis with lookahead (port of
//! frame-visualizer `lib/song_map.gd`, `SongMap`). Input: the band bins
//! ([`crate::Bins`]). All times are song seconds.
//!
//! What it holds (see the GDScript source and frame-visualizer PLAN.md for
//! the reasons behind each rule):
//! - Beat grid: tempo from the autocorrelation of an onset envelope
//!   (log-normal prior around [`PRIOR_BPM`]); one constant grid
//!   ([`SongMap::fixed_tempo`]) unless the music drifts off it
//!   ([`DRIFT_MAX`]), then a dynamic-programming grid (Ellis 2007) anchored on
//!   the kicks with a robust local line fit.
//! - Bars: one global downbeat phase (4/4), where the bass comes in.
//! - Sections: `sections::analyse`, each change moved onto a downbeat; one
//!   whole-song drop pass; changes snapped onto `novelty` part boundaries
//!   (drops do not move); sections split at the part boundaries inside them.
//! - Drops with a strength 0..1, the finale, phrases, the energy curve and
//!   kick / snare / hat onset events.

use crate::gd::{self, at, bsearch, clampi, int, maxf, minf, pct, posmod, sort_f32};
use crate::novelty;
use crate::sections::{self, BREAKDOWN, BUILD, CALM, DROP};

pub const MIN_BPM: f64 = 70.0;
pub const MAX_BPM: f64 = 180.0;
pub const PRIOR_BPM: f64 = 120.0;
pub const PRIOR_OCT: f64 = 0.9;
pub const TIGHTNESS: f64 = 100.0;
pub const FIT_BEATS: i64 = 8;
pub const FIT_TOL: f64 = 0.035;
pub const SUPPORT_WIN: f64 = 0.05;
pub const ANCHOR_AMP: u8 = 76;
pub const ANCHOR_WIN: i64 = 16;
pub const MIN_ANCHORS: i64 = 32;
pub const GAP_TOL: f64 = 0.04;
pub const AUDIBLE_DB: f64 = 40.0;
pub const FLOOR_DB: f64 = -60.0;
pub const ENERGY_S: f64 = 0.1;
pub const ENERGY_RANGE: f64 = 18.0;
pub const ENERGY_WIN: f64 = 0.4;
pub const PHRASE_BARS: i64 = 8;
pub const BAR_TOP: f64 = 0.0625;
pub const BAR_KICK: f64 = 0.5;
pub const SNAP_BARS: f64 = 0.75;
pub const BASS_ENTRY_DB: f64 = 6.0;
pub const LATE_GAP_DB: f64 = 8.0;
pub const LATE_JUMP_DB: f64 = 10.0;
pub const LATE_LOUD_DB: f64 = 6.0;
pub const LATE_NOVEL_DB: f64 = 4.0;
pub const LATE_LIFT_DB: f64 = 0.5;
pub const LATE_MIN_BARS: i64 = 4;
pub const LATE_MAX_BARS: i64 = 16;
pub const REP_BARS: i64 = 8;
pub const REP_MIN_BARS: i64 = 4;
pub const REP_NEAR_DB: f64 = 1.5;
pub const REP_FAR_DB: f64 = 3.0;
pub const REP_FLOOR_DB: f64 = -40.0;
pub const WEAK_SHARE: f64 = 0.5;
pub const STRENGTH_BARS: i64 = 8;
pub const BUILD_BARS: i64 = 4;
pub const STRENGTH_BUILD: f64 = 0.5;
pub const STRENGTH_FINAL: f64 = 1.1;
pub const STRENGTH_FLOOR: f64 = 0.2;
pub const NOVEL_SNAP_BARS: f64 = 1.0;
pub const PART_MIN_BARS: i64 = 4;
pub const SEARCH: f64 = 0.04;
pub const STEP: f64 = 0.0005;
pub const FINE_STEP: f64 = 0.00005;
pub const PHASE_BINS: i64 = 48;
pub const LS_WIN: f64 = 0.05;
pub const DRIFT_BEATS: i64 = 16;
pub const DRIFT_SEARCH: f64 = 0.17;
pub const DRIFT_MAX: f64 = 0.04;
pub const FINALE_DB: f64 = 1.5;
pub const PEAK_BINS: i64 = 3;
pub const MEAN_S: f64 = 0.75;
pub const KICK_DB: f64 = 7.0;
pub const SNARE_MID_DB: f64 = 5.0;
pub const KICK_LEVEL_DB: f64 = -12.0;
pub const SNARE_HIGH_DB: f64 = 5.0;
pub const HAT_DB: f64 = 6.0;
pub const SNARE_SUSTAIN_DB: f64 = 9.0;
pub const HAT_MID_MAX: f64 = 3.0;
pub const GATE_DB: f64 = -30.0;
pub const ONSET_LAG: f64 = 0.006;
pub const KICK_LAG: f64 = 0.005;
pub const KICK_ON_BEAT: f64 = 0.05;
pub const FLOOR_BAR_KICKS: i64 = 3;
pub const DANCE_COVER: f64 = 0.4;
pub const DANCE_STEP_DB: f64 = 5.0;
pub const STEP_S: f64 = 8.0;
pub const SONG_PEAK_DB: f64 = 2.0;

/// Track character: only the timeline words depend on it.
pub const SONG: i32 = 0;
pub const DANCE: i32 = 1;
pub const CHARACTER_NAMES: [&str; 2] = ["song", "dance"];
pub const DANCE_WORDS: [&str; 4] = ["Intro", "Build", "Drop", "Breakdown"];
pub const SONG_WORDS: [&str; 4] = ["Low", "Rising", "Peak", "Dip"];

/// Event kinds for the queries.
pub const KICK: i32 = 0;
pub const SNARE: i32 = 1;
pub const HAT: i32 = 2;
pub const BEAT: i32 = 3;
pub const DOWNBEAT: i32 = 4;
pub const PHRASE: i32 = 5;
pub const DROP_EV: i32 = 6;
pub const SECTION: i32 = 7;

/// The whole-song map. Field meanings as in `SongMap` (GDScript).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SongMap {
    pub duration: f64,
    /// Whole-song tempo (median beat interval).
    pub bpm: f64,
    pub beats: Vec<f32>,
    /// Per beat: 0 = downbeat .. 3.
    pub beat_bar: Vec<u8>,
    /// Per beat: 0..255 onsets on the grid around it.
    pub beat_support: Vec<u8>,
    pub downbeats: Vec<f32>,
    pub phrases: Vec<f32>,
    /// `[t0, id0, t1, id1, ...]` (section ids).
    pub sections: Vec<f32>,
    pub drops: Vec<f32>,
    pub drop_strength: Vec<f32>,
    pub finale_start: f64,
    pub finale_end: f64,
    /// Per ENERGY_S frame: overall, low, mid, high (0..255).
    pub energy: Vec<u8>,
    pub kicks: Vec<f32>,
    pub snares: Vec<f32>,
    pub hats: Vec<f32>,
    pub kick_amp: Vec<u8>,
    pub snare_amp: Vec<u8>,
    pub hat_amp: Vec<u8>,
    pub character: i32,
    pub kick_cover: f64,
    pub four_floor: f64,
    pub drop_step: f64,
    pub fixed_tempo: bool,
    pub grid_drift: f64,
    /// SongNovelty's part boundaries (analysis only, not cached).
    pub part_bounds: Vec<f32>,
    pub section_starts: Vec<f32>,
    /// First and last audible time (f32, as the GDScript Vector2).
    audible: (f32, f32),
}

/// A GDScript Vector2 / Vector3: f32 components.
#[inline]
fn v32(x: f64) -> f64 {
    x as f32 as f64
}

impl SongMap {
    pub fn new() -> Self {
        SongMap {
            finale_start: -1.0,
            finale_end: -1.0,
            audible: (f32::INFINITY, f32::NEG_INFINITY),
            ..Default::default()
        }
    }

    // --- queries -----------------------------------------------------------

    pub fn times(&self, kind: i32) -> &[f32] {
        match kind {
            KICK => &self.kicks,
            SNARE => &self.snares,
            HAT => &self.hats,
            BEAT => &self.beats,
            DOWNBEAT => &self.downbeats,
            PHRASE => &self.phrases,
            DROP_EV => &self.drops,
            SECTION => &self.section_starts,
            _ => &[],
        }
    }

    /// The first n events of `kind` strictly after t.
    pub fn next_events(&self, kind: i32, t: f64, n: usize) -> Vec<f32> {
        let a = self.times(kind);
        let i = bsearch(a, t, false) as usize;
        a[i..(i + n).min(a.len())].to_vec()
    }

    /// Time of the next event of `kind` after t, or INF.
    pub fn next_time(&self, kind: i32, t: f64) -> f64 {
        let a = self.times(kind);
        let i = bsearch(a, t, false) as usize;
        if i < a.len() { a[i] as f64 } else { f64::INFINITY }
    }

    /// Events of `kind` with t0 < time <= t1 (empty when t1 <= t0).
    pub fn events_between(&self, kind: i32, t0: f64, t1: f64) -> Vec<f32> {
        if t1 <= t0 {
            return Vec::new();
        }
        let a = self.times(kind);
        a[bsearch(a, t0, false) as usize..bsearch(a, t1, false) as usize].to_vec()
    }

    /// Strength 0..1 of event i of kind KICK / SNARE / HAT (1 otherwise).
    pub fn amp(&self, kind: i32, i: i64) -> f64 {
        let a: &[u8] = match kind {
            KICK => &self.kick_amp,
            SNARE => &self.snare_amp,
            HAT => &self.hat_amp,
            _ => &[],
        };
        if i >= 0 && (i as usize) < a.len() { a[i as usize] as f64 / 255.0 } else { 1.0 }
    }

    /// Strength 0..1 of the drop nearest to t (0 without drops).
    pub fn drop_strength_at(&self, t: f64) -> f64 {
        if self.drops.is_empty() || self.drop_strength.len() != self.drops.len() {
            return 0.0;
        }
        let d = &self.drops;
        let mut i = clampi(bsearch(d, t, true), 0, d.len() as i64 - 1);
        if i > 0 && (at(d, i - 1) - t).abs() < (at(d, i) - t).abs() {
            i -= 1;
        }
        self.drop_strength[i as usize] as f64
    }

    /// Index of the last beat at or before t (-1 before the first).
    pub fn beat_index_at(&self, t: f64) -> i64 {
        bsearch(&self.beats, t, false) - 1
    }

    pub fn beat_phase(&self, t: f64) -> f64 {
        let i = self.beat_index_at(t);
        if i < 0 || i + 1 >= self.beats.len() as i64 {
            return 0.0;
        }
        let b = &self.beats;
        gd::clampf((t - at(b, i)) / (at(b, i + 1) - at(b, i)), 0.0, 1.0)
    }

    pub fn bar_phase(&self, t: f64) -> f64 {
        let i = self.beat_index_at(t);
        if i < 0 {
            return 0.0;
        }
        (self.beat_bar[i as usize] as f64 + self.beat_phase(t)) / 4.0
    }

    pub fn bpm_at(&self, t: f64) -> f64 {
        let b = &self.beats;
        let i = clampi(self.beat_index_at(t), 0, b.len() as i64 - 2);
        if b.len() < 2 {
            return self.bpm;
        }
        60.0 / maxf(at(b, i + 1) - at(b, i), 0.2)
    }

    pub fn support_at(&self, t: f64) -> f64 {
        let i = self.beat_index_at(t);
        if i < 0 || i >= self.beats.len() as i64 {
            return 0.0;
        }
        self.beat_support[i as usize] as f64 / 255.0
    }

    pub fn phrase_index_at(&self, t: f64) -> i64 {
        bsearch(&self.phrases, t, false) - 1
    }

    pub fn phrase_phase(&self, t: f64) -> f64 {
        let i = self.phrase_index_at(t);
        if i < 0 {
            return 0.0;
        }
        let p = &self.phrases;
        let end = if i + 1 < p.len() as i64 { at(p, i + 1) } else { self.duration };
        gd::clampf((t - at(p, i)) / maxf(end - at(p, i), 0.1), 0.0, 1.0)
    }

    pub fn phrase_bar(&self, t: f64) -> i64 {
        let i = self.phrase_index_at(t);
        if i < 0 {
            return 0;
        }
        (bsearch(&self.downbeats, t, false) - bsearch(&self.downbeats, at(&self.phrases, i), true) - 1).max(0)
    }

    /// Section id at t (CALM before the first change).
    pub fn section_at(&self, t: f64) -> i32 {
        let i = bsearch(&self.section_starts, t, false) - 1;
        if i >= 0 { self.sections[i as usize * 2 + 1] as i32 } else { CALM }
    }

    /// Id of the section after the one at t (-1 if it is the last).
    pub fn next_section(&self, t: f64) -> i32 {
        let i = bsearch(&self.section_starts, t, false);
        if (i as usize) < self.section_starts.len() { self.sections[i as usize * 2 + 1] as i32 } else { -1 }
    }

    pub fn in_cooldown(&self, t: f64) -> bool {
        self.finale_end >= 0.0 && t >= self.finale_end
    }

    /// Energy 0..1 at t (linear between frames). band: -1 overall, 0 low, 1 mid, 2 high.
    pub fn energy_at(&self, t: f64, band: i64) -> f64 {
        let n = (self.energy.len() / 4) as i64;
        if n == 0 {
            return 0.0;
        }
        let x = gd::clampf(t / ENERGY_S - 0.5, 0.0, n as f64 - 1.0);
        let i = if n > 1 { int(x).min(n - 2) } else { 0 };
        let f = x - i as f64;
        let c = band + 1;
        let a = self.energy[(i * 4 + c) as usize] as f64;
        let b = self.energy[((i + 1).min(n - 1) * 4 + c) as usize] as f64;
        (a + (b - a) * f) / 255.0
    }

    /// Mean energy over [t0, t1].
    pub fn energy_mean(&self, t0: f64, t1: f64, band: i64) -> f64 {
        let n = (self.energy.len() / 4) as i64;
        if n == 0 {
            return 0.0;
        }
        let i0 = clampi(int(t0 / ENERGY_S), 0, n - 1);
        let i1 = clampi(int(t1 / ENERGY_S), i0, n - 1);
        let mut s: i64 = 0;
        for i in i0..=i1 {
            s += self.energy[(i * 4 + band + 1) as usize] as i64;
        }
        s as f64 / 255.0 / (i1 - i0 + 1) as f64
    }

    /// Index of the first section of each run of same-id sections.
    pub fn run_starts(secs: &[f32]) -> Vec<usize> {
        let mut out = Vec::new();
        for k in 0..secs.len() / 2 {
            if k == 0 || secs[k * 2 + 1] as i32 != secs[k * 2 - 1] as i32 {
                out.push(k);
            }
        }
        out
    }

    /// The display words of all sections of `secs` (the map's own when empty).
    /// A run of same-id sections shows the run's word.
    pub fn section_words(&self, secs: &[f32]) -> Vec<String> {
        let secs = if secs.is_empty() { &self.sections[..] } else { secs };
        let runs = Self::run_starts(secs);
        let mut merged: Vec<f32> = Vec::new();
        for &k in &runs {
            merged.push(secs[k * 2]);
            merged.push(secs[k * 2 + 1]);
        }
        let mut out = Vec::new();
        let mut r: i64 = -1;
        let mut w = String::new();
        for k in 0..secs.len() / 2 {
            if ((r + 1) as usize) < runs.len() && runs[(r + 1) as usize] == k {
                r += 1;
                w = self.run_word(&merged, r as usize);
            }
            out.push(w.clone());
        }
        out
    }

    /// Word of section k of a list without same-id neighbours.
    fn run_word(&self, secs: &[f32], k: usize) -> String {
        let n = secs.len() / 2;
        let id = secs[k * 2 + 1] as i32;
        if self.character == DANCE {
            if id != CALM {
                return DANCE_WORDS[id as usize].into();
            }
            if k == 0 {
                return "Intro".into();
            }
            for j in k + 1..n {
                if secs[j * 2 + 1] as i32 == DROP {
                    return "Breakdown".into();
                }
            }
            return "Outro".into();
        }
        if id == CALM {
            return if n > 1 && k == 0 {
                "Intro"
            } else if n > 1 && k == n - 1 {
                "Outro"
            } else {
                "Low"
            }
            .into();
        }
        if id != BUILD || self.energy.is_empty() {
            return SONG_WORDS[id as usize].into();
        }
        let a = secs[k * 2] as f64;
        let b = if k + 1 < n { secs[k * 2 + 2] as f64 } else { self.duration };
        let third = (b - a) / 3.0;
        let mut rise = f64::NEG_INFINITY;
        for band in [-1i64, 0, 1, 2] {
            rise = maxf(rise, (self.energy_mean(b - third, b, band) - self.energy_mean(a, a + third, band)) * ENERGY_RANGE);
        }
        if rise >= sections::RISE_DB {
            return "Rising".into();
        }
        let mut peak = f64::INFINITY;
        for j in 0..n {
            if secs[j * 2 + 1] as i32 == DROP {
                let e = if j + 1 < n { secs[j * 2 + 2] as f64 } else { self.duration };
                peak = minf(peak, self.energy_mean(secs[j * 2] as f64, e, -1));
            }
        }
        if (self.energy_mean(a, b, -1) - peak) * ENERGY_RANGE >= -SONG_PEAK_DB { "Peak" } else { "Low" }.into()
    }

    /// The words this map's character can show, in timeline legend order.
    pub fn vocabulary(&self) -> Vec<&'static str> {
        if self.character == DANCE {
            vec!["Intro", "Build", "Drop", "Breakdown", "Outro"]
        } else {
            vec!["Intro", "Low", "Rising", "Peak", "Dip", "Outro"]
        }
    }

    // --- analysis ----------------------------------------------------------

    /// Fills a new map from the band bins (`SongMap.analyse`). Pure and
    /// deterministic. `air` is optional (empty, or one value per bin).
    pub fn analyse(raw: &[f32], nb: usize, bin_s: f64, air: &[f32]) -> SongMap {
        let mut m = SongMap::new();
        m.run(raw, nb, bin_s, air);
        m
    }

    fn run(&mut self, raw: &[f32], nb: usize, bin_s: f64, air: &[f32]) {
        self.duration = nb as f64 * bin_s;
        let nbands = if air.len() == nb { 4 } else { 3 };
        // 1. band level in dB under its loud level, and its rise over 2 bins
        let mut lv: Vec<Vec<f32>> = Vec::new();
        let mut rise: Vec<Vec<f32>> = Vec::new();
        for b in 0..nbands {
            let mut l = vec![0.0f32; nb];
            for i in 0..nb {
                let v = if b == 3 { air[i] } else { raw[i * 3 + b] } as f64;
                l[i] = (8.685889638 * gd::log(v + 1e-6)) as f32;
            }
            let top = pct(&l, 0.98);
            let mut r = vec![0.0f32; nb];
            for x in l.iter_mut() {
                *x = maxf(*x as f64 - top, FLOOR_DB) as f32;
            }
            for i in 2..nb {
                r[i] = maxf(l[i] as f64 - l[i - 2] as f64, 0.0) as f32;
            }
            lv.push(l);
            rise.push(r);
        }
        // 2. onset envelope (all bands; highs weigh half), minus its local mean
        let hb = if nbands == 4 { 3 } else { 2 };
        let env: Vec<f32> = (0..nb)
            .map(|i| {
                (rise[0][i] as f64 + rise[1][i] as f64 + 0.5 * (rise[2][i] as f64 + rise[hb][i] as f64)) as f32
            })
            .collect();
        let env = minus_mean(&env, int(0.5 / bin_s));
        // 3. tempo
        let period = tempo(&env, bin_s);
        // 4. onset events, 5. the grid
        self.events(&lv, &rise, nb, bin_s, hb);
        self.energy_curve(raw, nb, bin_s);
        let fit = self.fixed_grid(&env, &lv[0], bin_s, period);
        self.grid_drift = fit.2;
        if fit.1 > 0.0 && fit.2 <= DRIFT_MAX {
            self.fixed_tempo = true;
            self.beats.clear();
            let k0 = int((-fit.0 / fit.1).ceil());
            let mut k = k0;
            while fit.0 + k as f64 * fit.1 < self.duration {
                self.beats.push((fit.0 + k as f64 * fit.1) as f32);
                k += 1;
            }
        } else {
            self.fixed_tempo = false;
            self.track_beats(&env, nb, bin_s, period);
            self.refine();
        }
        self.trim();
        self.set_bpm();
        self.support();
        // 6. sections, bars, phrases, finale
        self.bars(&lv, &rise, bin_s);
        let power: Vec<Vec<f32>> = (0..nbands)
            .map(|b| {
                (0..nb)
                    .map(|i| {
                        let v = if b == 3 { air[i] } else { raw[i * 3 + b] } as f64;
                        (v * v) as f32
                    })
                    .collect()
            })
            .collect();
        self.part_bounds = novelty::boundaries(&power, bin_s, &self.beats, &self.beat_bar);
        let secs = sections::analyse(raw, nb, bin_s, &self.beats);
        self.set_sections(&secs, &lv, bin_s);
        self.phrases_pass();
        self.finale();
        self.character_pass();
    }

    /// Constant grid: (time of beat 0, period, drift), each rounded to f32
    /// (a GDScript Vector3); period 0 when it fails.
    fn fixed_grid(&self, env: &[f32], low: &[f32], bin_s: f64, p0: f64) -> (f64, f64, f64) {
        let mut pt: Vec<f32> = Vec::new();
        let mut pw: Vec<f32> = Vec::new();
        let n = env.len();
        for i in 2..n.saturating_sub(2) {
            let v = env[i] as f64;
            let (e1, e2, f1, f2) = (env[i - 1] as f64, env[i - 2] as f64, env[i + 1] as f64, env[i + 2] as f64);
            if v <= 0.0 || v < e1 || v <= f1 || v < e2 || v <= f2 {
                continue;
            }
            let den = e1 - 2.0 * v + f1;
            let off = if den < 0.0 { gd::clampf(0.5 * (e1 - f1) / den, -0.5, 0.5) } else { 0.0 };
            pt.push(((i as f64 + off + 0.5) * bin_s - ONSET_LAG) as f32);
            pw.push(v as f32);
        }
        if pt.len() < 16 {
            return (0.0, 0.0, f64::INFINITY);
        }
        let mut best_p = p0;
        let mut best_s = -1.0f64;
        let mut hist = vec![0.0f32; PHASE_BINS as usize];
        for pass_i in 0..2 {
            let span = if pass_i == 0 { SEARCH } else { STEP * 1.5 };
            let step = if pass_i == 0 { STEP } else { FINE_STEP };
            let c = best_p;
            let m = int((span / step).round());
            for q in -m..=m {
                let p = c * (1.0 + q as f64 * step);
                let s = fold_peak(&pt, &pw, p, 0.0, f64::INFINITY, &mut hist, 0.0, 0.5).1;
                if s > best_s {
                    best_s = s;
                    best_p = p;
                }
            }
        }
        let pk = fold_peak(&pt, &pw, best_p, 0.0, f64::INFINITY, &mut hist, 0.0, 0.5);
        let mut a = pk.0;
        let mut b = best_p;
        // the beat or the half beat (see the GDScript source)
        let mut k_on = 0.0f64;
        let mut k_off = 0.0f64;
        for e in 0..self.kicks.len() {
            let mut r = (self.kicks[e] as f64 - a) / b;
            r -= r.round();
            if r.abs() < 0.125 {
                k_on += self.kick_amp[e] as f64;
            } else if r.abs() > 0.375 {
                k_off += self.kick_amp[e] as f64;
            }
        }
        if k_off > k_on {
            let mut half: Vec<f32> = Vec::new();
            let mut k = (-a / b).ceil();
            while a + k * b < self.duration {
                half.push((a + k * b) as f32);
                half.push((a + (k + 0.5) * b) as f32);
                k += 1.0;
            }
            let ent = entry_steps(low, bin_s, &half, 4.0 * b, 8);
            let e = |i: usize| ent[i] as f64;
            let e_on = maxf(maxf(e(0), e(2)), maxf(e(4), e(6)));
            let e_off = maxf(maxf(e(1), e(3)), maxf(e(5), e(7)));
            if e_off > e_on {
                a += 0.5 * b;
            }
        }
        // least-squares line through the onsets near the grid
        for _ in 0..3 {
            let (mut sw, mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
            for j in 0..pt.len() {
                let t = pt[j] as f64;
                let k = ((t - a) / b).round();
                if (t - a - k * b).abs() > LS_WIN {
                    continue;
                }
                let w = pw[j] as f64;
                sw += w;
                sx += w * k;
                sy += w * t;
                sxx += w * k * k;
                sxy += w * k * t;
            }
            let dd = sw * sxx - sx * sx;
            if sw <= 0.0 || dd.abs() < 1e-9 {
                break;
            }
            b = (sw * sxy - sx * sy) / dd;
            a = (sy - b * sx) / sw;
        }
        // drift: the local phase offset to this grid per window
        let mut offs: Vec<f32> = Vec::new();
        let mut hs: Vec<f32> = Vec::new();
        let win = DRIFT_BEATS as f64 * b;
        let mut t = 0.0f64;
        while t + win <= self.duration {
            let lp = fold_peak(&pt, &pw, b, t, t + win, &mut hist, a, DRIFT_SEARCH);
            offs.push(lp.0 as f32);
            hs.push(lp.1 as f32);
            t += win * 0.5;
        }
        let min_h = pct(&hs, 0.5) * 0.5;
        let mut good: Vec<f32> = Vec::new();
        for k in 0..offs.len() {
            if hs[k] as f64 >= min_h && hs[k] > 0.0 {
                good.push(offs[k]);
            }
        }
        let mut drift = f64::INFINITY;
        if good.len() >= 4 {
            sort_f32(&mut good);
            let g = (good.len() - 1) as f64;
            drift = good[int(0.9 * g) as usize] as f64 - good[int(0.1 * g) as usize] as f64;
        }
        (v32(a), v32(b), v32(drift))
    }

    /// Ellis-style DP over the envelope.
    fn track_beats(&mut self, env: &[f32], nb: usize, bin_s: f64, period: f64) {
        self.beats.clear();
        let p = period / bin_s;
        let w0 = int((p * 0.7).floor());
        let w1 = int((p * 1.4).ceil());
        let mut pen = vec![0.0f32; (w1 + 1) as usize];
        for d in w0..=w1 {
            pen[d as usize] = (-TIGHTNESS * gd::pow(gd::log(d as f64 / p), 2.0)) as f32;
        }
        let mut sd = 0.0f64;
        for &v in env {
            sd += v as f64 * v as f64;
        }
        sd = gd::sqrt(sd / maxf(nb as f64, 1.0)) + 1e-9;
        let mut score = vec![0.0f32; nb];
        let mut back = vec![0i32; nb];
        for t in 0..nb as i64 {
            let loc = env[t as usize] as f64 / sd;
            let mut best = f64::NEG_INFINITY;
            let mut bi: i64 = -1;
            let lo = (t - w1).max(0);
            for k in lo..t - w0 + 1 {
                let s = score[k as usize] as f64 + pen[(t - k) as usize] as f64;
                if s > best {
                    best = s;
                    bi = k;
                }
            }
            if bi < 0 {
                best = 0.0;
            }
            score[t as usize] = (loc + best) as f32;
            back[t as usize] = bi as i32;
        }
        let nbi = nb as i64;
        let mut end = nbi - 1;
        for t in (nbi - int(p) - 1).max(0)..nbi {
            if score[t as usize] > score[end as usize] {
                end = t;
            }
        }
        let mut idx: Vec<i64> = Vec::new();
        let mut t = end;
        while t >= 0 {
            idx.push(t);
            t = back[t as usize] as i64;
        }
        idx.reverse();
        if idx.len() < 4 {
            return;
        }
        self.beats = idx.iter().map(|&i| ((i as f64 + 0.5) * bin_s - ONSET_LAG) as f32).collect();
    }

    /// Kick anchors, even gap fill and a robust local line fit (`_refine`).
    fn refine(&mut self) {
        let n = self.beats.len();
        if n < 4 {
            return;
        }
        let mut anc = vec![0u8; n];
        let mut count = 0i64;
        for pass_kind in 0..2 {
            let (ev, am) = if pass_kind == 0 { (&self.kicks, &self.kick_amp) } else { (&self.snares, &self.snare_amp) };
            for e in 0..ev.len() {
                if am[e] < ANCHOR_AMP {
                    continue;
                }
                let i = bsearch(&self.beats, ev[e] as f64, true);
                for j in [i - 1, i] {
                    if j >= 0 && (j as usize) < n {
                        let ju = j as usize;
                        if (self.beats[ju] as f64 - ev[e] as f64).abs() <= SUPPORT_WIN && anc[ju] == 0 {
                            anc[ju] = 1;
                            count += 1;
                        }
                    }
                }
            }
            if count >= MIN_ANCHORS {
                break;
            }
        }
        let mut ai: Vec<i32> = (0..n).filter(|&i| anc[i] == 1).map(|i| i as i32).collect();
        for _ in 0..2 {
            let mut good: Vec<i32> = Vec::new();
            for k in 0..ai.len() as i64 {
                let l = self.line(&ai, k - ANCHOR_WIN, k + ANCHOR_WIN);
                let x = ai[k as usize];
                if (self.beats[x as usize] as f64 - (l.0 + l.1 * x as f64)).abs() <= FIT_TOL {
                    good.push(x);
                }
            }
            ai = good;
        }
        if ai.len() >= 4 {
            let na = ai.len() as i64;
            let p = self.line(&ai, 0, na).1;
            let mut out: Vec<f32> = Vec::new();
            let bt = |i: i32| self.beats[i as usize] as f64;
            let t0 = bt(ai[0]);
            let ph = self.line(&ai, 0, ANCHOR_WIN * 2).1;
            let k0 = int((t0 / ph).floor());
            let mut j = k0;
            while j > 0 {
                out.push((t0 - j as f64 * ph) as f32);
                j -= 1;
            }
            for k in 0..ai.len() {
                out.push(self.beats[ai[k] as usize]);
                if k + 1 == ai.len() {
                    break;
                }
                let a = ai[k];
                let c = ai[k + 1];
                if c - a == 1 {
                    continue;
                }
                let gap = bt(c) - bt(a);
                let ki = k as i64;
                let mut pl = self.line(&ai, ki + 1 - ANCHOR_WIN, ki + 1).1;
                let mut pr = self.line(&ai, ki + 1, ki + 1 + ANCHOR_WIN).1;
                if (pl / p - 1.0).abs() > 0.1 || (pr / p - 1.0).abs() > 0.1 {
                    pl = p;
                    pr = p;
                }
                let pg = 0.5 * (pl + pr);
                let m = gd::roundi(gap / pg);
                if m >= 1 && (gap / m as f64 - pg).abs() <= GAP_TOL * pg {
                    let mf = m as f64;
                    let mut sum = 0.0f64;
                    for jj in 0..m {
                        sum += pl + (pr - pl) * (jj as f64 + 0.5) / mf;
                    }
                    let mut acc = 0.0f64;
                    for jj in 1..m {
                        acc += pl + (pr - pl) * (jj as f64 - 0.5) / mf;
                        out.push((bt(a) + acc * gap / sum) as f32);
                    }
                } else {
                    for jj in a + 1..c {
                        out.push(self.beats[jj as usize]);
                    }
                }
            }
            let tl = bt(ai[ai.len() - 1]);
            let ptl = self.line(&ai, na - ANCHOR_WIN * 2, na).1;
            let mut jj = 1i64;
            while tl + jj as f64 * ptl < self.duration {
                out.push((tl + jj as f64 * ptl) as f32);
                jj += 1;
            }
            self.beats = out;
        }
        // robust local line fit
        let raw_t = self.beats.clone();
        let mut keep = vec![1u8; raw_t.len()];
        let mut fit = raw_t.clone();
        for _ in 0..2 {
            let out: Vec<f32> = (0..raw_t.len()).map(|i| local_fit(&raw_t, &keep, i as i64) as f32).collect();
            for i in 0..raw_t.len() {
                keep[i] = if (raw_t[i] as f64 - out[i] as f64).abs() <= FIT_TOL { 1 } else { 0 };
            }
            fit = out;
        }
        self.beats = fit;
    }

    /// Least-squares line time = x + y * beat index through beats[ai[k]],
    /// k in [k0, k1) (clamped); f32 components (a GDScript Vector2).
    fn line(&self, ai: &[i32], k0: i64, k1: i64) -> (f64, f64) {
        let (mut sw, mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        let na = ai.len() as i64;
        let x0 = ai[clampi(k0, 0, na - 1) as usize] as f64;
        for k in k0.max(0)..k1.min(na) {
            let x = ai[k as usize] as f64 - x0;
            let y = self.beats[ai[k as usize] as usize] as f64;
            sw += 1.0;
            sx += x;
            sy += y;
            sxx += x * x;
            sxy += x * y;
        }
        let den = sw * sxx - sx * sx;
        if sw < 2.0 || den.abs() < 1e-9 {
            return (v32(self.beats[ai[0] as usize] as f64 - ai[0] as f64 * 0.5), 0.5);
        }
        let slope = (sw * sxy - sx * sy) / den;
        (v32((sy - slope * sx) / sw - slope * x0), v32(slope))
    }

    fn set_bpm(&mut self) {
        let iv: Vec<f32> = (1..self.beats.len())
            .map(|i| (self.beats[i] as f64 - self.beats[i - 1] as f64) as f32)
            .collect();
        self.bpm = if !iv.is_empty() { 60.0 / pct(&iv, 0.5) } else { 0.0 };
    }

    /// Onset events per class: peaks of the band rises above their local mean.
    fn events(&mut self, lv: &[Vec<f32>], rise: &[Vec<f32>], nb: usize, bin_s: f64, hb: usize) {
        let mw = int(MEAN_S / bin_s);
        let m: Vec<Vec<f32>> = rise.iter().map(|r| running_mean(r, mw)).collect();
        let (rl, rm, rh, ra) = (&rise[0], &rise[1], &rise[2], &rise[hb]);
        let mut k: Vec<(f32, f32)> = Vec::new();
        let mut s: Vec<(f32, f32)> = Vec::new();
        let mut h: Vec<(f32, f32)> = Vec::new();
        let ll = &lv[0];
        let hl = &lv[2];
        for i in 2..nb.saturating_sub(1) {
            let tt = (i as f64 + 0.5) * bin_s - ONSET_LAG;
            let xl = rl[i] as f64 - m[0][i] as f64;
            if xl >= KICK_DB && maxf(ll[i] as f64, ll[i + 1] as f64) >= KICK_LEVEL_DB && is_peak(rl, i) {
                k.push(((tt - KICK_LAG) as f32, xl as f32));
            }
            let xm = maxf(rm[i] as f64, rm[i + 1] as f64) - m[1][i] as f64;
            let xh = rh[i] as f64 - m[2][i] as f64;
            let sustain = hl[(i + 3).min(nb - 1)] as f64 >= maxf(hl[i] as f64, hl[i + 1] as f64) - SNARE_SUSTAIN_DB;
            if xh >= SNARE_HIGH_DB && xm >= SNARE_MID_DB && sustain && is_peak(rh, i) && lv[1][i] as f64 >= GATE_DB {
                s.push((tt as f32, (xm + xh) as f32));
            }
            let xa = ra[i] as f64 - m[hb][i] as f64;
            if xa >= HAT_DB && xm < HAT_MID_MAX && is_peak(ra, i) && lv[hb][i] as f64 >= GATE_DB {
                h.push((tt as f32, xa as f32));
            }
        }
        let put = |ev: &[(f32, f32)]| -> (Vec<f32>, Vec<u8>) {
            let st: Vec<f32> = ev.iter().map(|e| e.1).collect();
            let top = if !st.is_empty() { maxf(pct(&st, 0.9), 1.0) } else { 1.0 };
            let tms = ev.iter().map(|e| e.0).collect();
            let am = ev.iter().map(|e| clampi(int(e.1 as f64 / top * 255.0), 1, 255) as u8).collect();
            (tms, am)
        };
        (self.kicks, self.kick_amp) = put(&k);
        (self.snares, self.snare_amp) = put(&s);
        (self.hats, self.hat_amp) = put(&h);
    }

    /// Keep the beats from half a beat before the music is audible to its end.
    fn trim(&mut self) {
        if self.audible.0.is_infinite() || self.beats.len() < 2 {
            return;
        }
        let p = self.beats[1] as f64 - self.beats[0] as f64;
        let a = bsearch(&self.beats, self.audible.0 as f64 + 0.5 * ENERGY_WIN - 0.5 * p, true) as usize;
        let b = bsearch(&self.beats, self.audible.1 as f64 - 0.5 * ENERGY_WIN - 0.25 * p, true) as usize;
        // PackedFloat32Array.slice(a, b) with b < a is empty
        self.beats = if b > a { self.beats[a..b].to_vec() } else { Vec::new() };
    }

    /// Share of onsets on the grid around each beat (+-4 beats).
    fn support(&mut self) {
        let nbt = self.beats.len();
        let mut hit = vec![0u8; nbt];
        for ev in [&self.kicks, &self.snares] {
            for &x in ev.iter() {
                let i = bsearch(&self.beats, x as f64, true);
                for j in [i - 1, i] {
                    if j >= 0 && (j as usize) < nbt && (self.beats[j as usize] as f64 - x as f64).abs() <= SUPPORT_WIN {
                        hit[j as usize] = 1;
                    }
                }
            }
        }
        self.beat_support = (0..nbt as i64)
            .map(|i| {
                let mut c = 0i64;
                let mut n = 0i64;
                for j in (i - 4).max(0)..(i + 5).min(nbt as i64) {
                    c += hit[j as usize] as i64;
                    n += 1;
                }
                int(255.0 * c as f64 / maxf(n as f64, 1.0)) as u8
            })
            .collect();
    }

    /// Energy frames (ENERGY_S hop), total and per band.
    fn energy_curve(&mut self, raw: &[f32], nb: usize, bin_s: f64) {
        let mut cs: Vec<Vec<f64>> = vec![vec![0.0; nb + 1]; 4];
        for i in 0..nb {
            let mut tot = 0.0f64;
            for b in 0..3 {
                let v = raw[i * 3 + b] as f64;
                tot += v * v;
                cs[b + 1][i + 1] = cs[b + 1][i] + v * v;
            }
            cs[0][i + 1] = cs[0][i] + tot;
        }
        let nf = int((nb as f64 * bin_s / ENERGY_S).ceil()).max(0) as usize;
        let half = 1i64.max(int(ENERGY_WIN * 0.5 / bin_s));
        self.energy = vec![0u8; nf * 4];
        let nbi = nb as i64;
        for c in 0..4 {
            let mut d = vec![0.0f32; nf];
            for f in 0..nf {
                let mid = int((f as f64 + 0.5) * ENERGY_S / bin_s);
                let mut a0 = clampi(mid - half, 0, nbi);
                let a1 = clampi(mid + half, a0 + 1, nbi);
                a0 = a0.min(a1 - 1);
                d[f] = (4.342944819
                    * gd::log((cs[c][a1 as usize] - cs[c][a0 as usize]) / (a1 - a0) as f64 + 1e-12))
                    as f32;
            }
            let top = pct(&d, 0.98);
            for f in 0..nf {
                self.energy[f * 4 + c] = clampi(int((1.0 + (d[f] as f64 - top) / ENERGY_RANGE) * 255.0), 0, 255) as u8;
            }
            if c == 0 {
                self.audible = (f32::INFINITY, f32::NEG_INFINITY);
                for f in 0..nf {
                    if d[f] as f64 >= top - AUDIBLE_DB {
                        self.audible = (
                            minf(self.audible.0 as f64, f as f64 * ENERGY_S) as f32,
                            ((f + 1) as f64 * ENERGY_S) as f32,
                        );
                    }
                }
            }
        }
    }

    /// One global downbeat phase (4/4).
    fn bars(&mut self, lv: &[Vec<f32>], rise: &[Vec<f32>], bin_s: f64) {
        let nbt = self.beats.len();
        self.beat_bar = vec![0u8; nbt];
        self.downbeats.clear();
        if nbt == 0 {
            return;
        }
        let nb = lv[0].len() as i64;
        let mut kick = [0.0f32; 4];
        for i in 0..nbt {
            let c = clampi(int(self.beats[i] as f64 / bin_s), 0, nb - 1);
            let mut k = 0.0f64;
            for j in (c - 2).max(0)..(c + 3).min(nb) {
                k = maxf(k, rise[0][j as usize] as f64);
            }
            kick[i % 4] = (kick[i % 4] as f64 + k) as f32;
        }
        let top = entry_steps(&lv[0], bin_s, &self.beats, 4.0 * 60.0 / maxf(self.bpm, 30.0), 4);
        let t = |i: usize| top[i] as f64;
        let kk = |i: usize| kick[i] as f64;
        let mt = maxf(maxf(t(0), t(1)), maxf(t(2), t(3))) + 1e-9;
        let mk = maxf(maxf(kk(0), kk(1)), maxf(kk(2), kk(3))) + 1e-9;
        let mut best = 0i64;
        let mut bv = -1.0f64;
        for s in 0..4 {
            let v = t(s) / mt + BAR_KICK * kk(s) / mk;
            if v > bv {
                bv = v;
                best = s as i64;
            }
        }
        for i in 0..nbt {
            self.beat_bar[i] = posmod(i as i64 - best, 4) as u8;
            if self.beat_bar[i] == 0 {
                self.downbeats.push(self.beats[i]);
            }
        }
    }

    /// Sections from SongSections, each change moved onto a downbeat.
    fn set_sections(&mut self, secs: &[f32], lv: &[Vec<f32>], bin_s: f64) {
        let low = &lv[0];
        self.sections = secs.to_vec();
        let nb = low.len() as i64;
        let mut e = vec![0.0f64; nb as usize + 1];
        for i in 0..nb as usize {
            e[i + 1] = e[i] + low[i] as f64;
        }
        let beat = 60.0 / maxf(self.bpm, 30.0);
        let w = int(beat / bin_s).max(1);
        let mut k = 2;
        while k < self.sections.len() {
            let t = self.sections[k] as f64;
            let id = self.sections[k + 1] as i32;
            let mut best = t;
            let mut bv = if id != DROP && id != BREAKDOWN { f64::INFINITY } else { f64::NEG_INFINITY };
            let db = &self.downbeats;
            let mut j = bsearch(db, t - SNAP_BARS * 4.0 * beat, true) as usize;
            while j < db.len() && db[j] as f64 <= t + SNAP_BARS * 4.0 * beat {
                let d = db[j] as f64;
                let c = clampi(int(d / bin_s), w, nb - w);
                let (cu, wu) = (c as usize, w as usize);
                let step = (e[cu + wu] - e[cu] - (e[cu] - e[cu - wu])) / w as f64;
                if id == DROP && step > bv {
                    bv = step;
                    best = d;
                } else if id == BREAKDOWN && -step > bv {
                    bv = -step;
                    best = d;
                } else if id != DROP && id != BREAKDOWN && (d - t).abs() < bv {
                    bv = (d - t).abs();
                    best = d;
                }
                j += 1;
            }
            self.sections[k] = best as f32;
            k += 2;
        }
        self.drop_pass(lv, bin_s);
        self.split_parts();
        self.section_starts = self.sections.iter().step_by(2).copied().collect();
    }

    /// The whole-song drop pass (see the GDScript source).
    fn drop_pass(&mut self, lv: &[Vec<f32>], bin_s: f64) {
        let n = self.sections.len() / 2;
        let nbt = self.beats.len();
        let nbin = lv[0].len();
        self.drops.clear();
        self.drop_strength.clear();
        if n == 0 || nbin == 0 || nbt < 2 || self.downbeats.len() < 2 {
            for k in 0..n {
                if self.sections[k * 2 + 1] as i32 == DROP {
                    self.drops.push(self.sections[k * 2]);
                    self.drop_strength.push(1.0);
                }
            }
            return;
        }
        let bar = 4.0 * 60.0 / maxf(self.bpm, 30.0);
        let nbands = lv.len();
        let p: Vec<Vec<f64>> = lv
            .iter()
            .map(|l| {
                let mut q = vec![0.0f64; nbin + 1];
                for i in 0..nbin {
                    q[i + 1] = q[i] + gd::pow(10.0, l[i] as f64 / 10.0);
                }
                q
            })
            .collect();
        let loud = pct(&lv[0], 0.9);
        let mut lev = vec![0.0f32; (nbt - 1) * nbands];
        for b in 0..nbands {
            for j in 0..nbt - 1 {
                lev[j * nbands + b] =
                    maxf(pw(&p[b], bin_s, self.beats[j] as f64, self.beats[j + 1] as f64), REP_FLOOR_DB) as f32;
            }
        }
        let span = 4 * REP_BARS;
        let dist = |i: i64, j: i64| -> f64 {
            let s = span.min(nbt as i64 - 1 - i.max(j));
            if i < 0 || j < 0 || s < 4 * REP_MIN_BARS {
                return f64::INFINITY;
            }
            let nbd = nbands as i64;
            let mut v = 0.0f64;
            for q in 0..s * nbd {
                v += (lev[(i * nbd + q) as usize] as f64 - lev[(j * nbd + q) as usize] as f64).abs();
            }
            v / (s * nbd) as f64
        };
        // 1. the anchors: each DROP start and each gap entry
        let mut ends: Vec<f32> = Vec::new();
        let mut at_b: Vec<i64> = Vec::new();
        let mut ak: Vec<usize> = Vec::new();
        let mut start_bass = vec![0u8; n];
        for k in 0..n {
            let a = self.sections[k * 2] as f64;
            ends.push(if k + 1 < n { self.sections[k * 2 + 2] } else { self.duration as f32 });
            if self.sections[k * 2 + 1] as i32 != DROP {
                continue;
            }
            let mut bass = a >= bar && pw(&p[0], bin_s, a, a + 4.0 * bar) - pw(&p[0], bin_s, a - 4.0 * bar, a) >= BASS_ENTRY_DB;
            let j = bsearch(&self.downbeats, a - 0.05, true);
            bass = bass
                || ((j as usize) < self.downbeats.len()
                    && (at(&self.downbeats, j) - a).abs() < 0.05
                    && self.bass_entry(&p[0], bin_s, j, bar, loud));
            start_bass[k] = bass as u8;
            at_b.push(self.beat_near(a));
            ak.push(k);
            for d in self.inner(a, ends[k] as f64, bar) {
                let dd = d as f64;
                if self.bass_entry(&p[0], bin_s, bsearch(&self.downbeats, dd - 0.05, true), bar, loud) {
                    at_b.push(self.beat_near(dd));
                    ak.push(k);
                }
            }
        }
        let near = |t: f64, k: usize| -> f64 {
            let mut v = f64::INFINITY;
            for q in 0..at_b.len() {
                if ak[q] != k {
                    v = minf(v, dist(self.beat_near(t), at_b[q]));
                }
            }
            v
        };
        // 2. each DROP section's drop: rank by (bass entry, repeat)
        let mut pick: Vec<f32> = Vec::new();
        let mut how: Vec<u8> = Vec::new();
        let mut bass_of: Vec<u8> = Vec::new();
        let mut rep_of: Vec<u8> = Vec::new();
        for k in 0..n {
            pick.push(-1.0);
            how.push(0);
            bass_of.push(0);
            rep_of.push(0);
            if self.sections[k * 2 + 1] as i32 != DROP {
                continue;
            }
            let a = self.sections[k * 2] as f64;
            let mut bt = a;
            let mut bb = start_bass[k] as i64;
            let na = near(a, k);
            let mut br: i64 = if na <= REP_NEAR_DB { 1 } else { 0 };
            let mut bd = f64::INFINITY;
            let cands = if bb == 0 || na >= REP_FAR_DB { self.inner(a, ends[k] as f64, bar) } else { Vec::new() };
            for d in cands {
                let d = d as f64;
                let g: i64 = self.bass_entry(&p[0], bin_s, bsearch(&self.downbeats, d - 0.05, true), bar, loud) as i64;
                let nd = near(d, k);
                let r: i64 = if nd <= REP_NEAR_DB { 1 } else { 0 };
                if r > br || (r == br && g > bb) || (r == 1 && br == 1 && g == bb && bt != a && nd < bd) {
                    bt = d;
                    bb = g;
                    br = r;
                    bd = nd;
                    how[k] = if g == 1 { 1 } else { 2 };
                }
            }
            pick[k] = bt as f32;
            bass_of[k] = bb as u8;
            rep_of[k] = br as u8;
        }
        let mut cluster = f64::INFINITY;
        for k in 0..n {
            if pick[k] >= 0.0 && bass_of[k] == 1 && rep_of[k] == 1 {
                cluster = minf(cluster, self.drop_size(&p, bin_s, pick[k] as f64));
            }
        }
        // 3. fake drops out, moved drops split their section
        let mut real = vec![0u8; n];
        for k in 0..n {
            let weak = bass_of[k] == 0 || self.drop_size(&p, bin_s, pick[k] as f64) < WEAK_SHARE * cluster;
            real[k] = (pick[k] >= 0.0 && (cluster == f64::INFINITY || rep_of[k] == 1 || !weak)) as u8;
        }
        let mut out: Vec<f32> = Vec::new();
        for k in 0..n {
            let mut id = self.sections[k * 2 + 1] as i32;
            let before = (0..k).any(|q| real[q] == 1);
            let after = (k + 1..n).any(|q| real[q] == 1);
            if id == DROP && real[k] == 0 {
                id = if before && after { BREAKDOWN } else { CALM };
            }
            out.push(self.sections[k * 2]);
            if id == DROP && pick[k] > self.sections[k * 2] {
                out.push(if how[k] == 1 || !before { BUILD } else { BREAKDOWN } as f32);
                out.push(pick[k]);
            }
            out.push(id as f32);
        }
        self.sections.clear();
        for k in (0..out.len()).step_by(2) {
            if self.sections.is_empty() || self.sections[self.sections.len() - 1] as i32 != out[k + 1] as i32 {
                self.sections.push(out[k]);
                self.sections.push(out[k + 1]);
            }
        }
        // 4. each change into a calm, build or breakdown section onto the
        // nearest part boundary within NOVEL_SNAP_BARS (drops never move)
        let mut k = 2;
        while k < self.sections.len() {
            if self.sections[k + 1] as i32 == DROP {
                k += 2;
                continue;
            }
            let t = self.sections[k] as f64;
            let mut best = t;
            let mut bd = NOVEL_SNAP_BARS * bar + 0.05;
            for &d in &self.part_bounds {
                let d = d as f64;
                if (d - t).abs() <= bd {
                    bd = (d - t).abs();
                    best = d;
                }
            }
            let lo = self.sections[k - 2] as f64 + (LATE_MIN_BARS as f64 - 0.25) * bar;
            let hi_end = if k + 2 < self.sections.len() { self.sections[k + 2] as f64 } else { self.duration };
            let hi = hi_end - (LATE_MIN_BARS as f64 - 0.25) * bar;
            if best >= lo && best <= hi {
                self.sections[k] = best as f32;
            }
            k += 2;
        }
        // 5. rank the drops
        for k in (0..self.sections.len()).step_by(2) {
            if self.sections[k + 1] as i32 == DROP {
                self.drops.push(self.sections[k]);
            }
        }
        self.drop_strengths(&p, bin_s);
    }

    /// Each section split at the part boundaries inside it (same id).
    fn split_parts(&mut self) {
        if self.part_bounds.is_empty() || self.sections.is_empty() {
            return;
        }
        let room = (PART_MIN_BARS as f64 - 0.25) * 4.0 * 60.0 / maxf(self.bpm, 30.0);
        let mut pb = self.part_bounds.clone();
        sort_f32(&mut pb);
        let mut out: Vec<f32> = Vec::new();
        let n = self.sections.len() / 2;
        for k in 0..n {
            let a = self.sections[k * 2] as f64;
            let b = if k + 1 < n { self.sections[k * 2 + 2] as f64 } else { self.duration };
            let id = self.sections[k * 2 + 1];
            out.push(a as f32);
            out.push(id);
            let mut last = a;
            for &d in &pb {
                let d = d as f64;
                if d - last >= room && b - d >= room {
                    out.push(d as f32);
                    out.push(id);
                    last = d;
                }
            }
        }
        self.sections = out;
    }

    /// The downbeats LATE_MIN_BARS..LATE_MAX_BARS bars after a, and at least
    /// LATE_MIN_BARS before b.
    fn inner(&self, a: f64, b: f64, bar: f64) -> Vec<f32> {
        let mut v = Vec::new();
        let db = &self.downbeats;
        let mut j = bsearch(db, a + (LATE_MIN_BARS as f64 - 0.25) * bar, true) as usize;
        while j < db.len() {
            let d = db[j] as f64;
            if d > a + (LATE_MAX_BARS as f64 + 0.25) * bar || d > b - (LATE_MIN_BARS as f64 - 0.25) * bar {
                break;
            }
            v.push(db[j]);
            j += 1;
        }
        v
    }

    fn drop_strengths(&mut self, p: &[Vec<f64>], bin_s: f64) {
        self.drop_strength.clear();
        let nd = self.drops.len();
        if nd == 0 {
            return;
        }
        let mut raw: Vec<f32> = Vec::new();
        let mut best = 0.0f64;
        for i in 0..nd {
            let v = self.drop_size(p, bin_s, self.drops[i] as f64) * if i == nd - 1 { STRENGTH_FINAL } else { 1.0 };
            raw.push(v as f32);
            best = maxf(best, v);
        }
        for i in 0..nd {
            let s = if best > 0.0 { STRENGTH_FLOOR + (1.0 - STRENGTH_FLOOR) * raw[i] as f64 / best } else { 1.0 };
            self.drop_strength.push(s as f32);
        }
    }

    /// The size of a drop at a (dB, at least 0).
    fn drop_size(&self, p: &[Vec<f64>], bin_s: f64, a: f64) -> f64 {
        let bar = 4.0 * 60.0 / maxf(self.bpm, 30.0);
        let span = STRENGTH_BARS as f64 * bar;
        let mut v = 0.0f64;
        if a >= bar {
            v = pw(&p[0], bin_s, a, a + span) - pw(&p[0], bin_s, maxf(a - span, 0.0), a);
        }
        let mut build = 0.0f64;
        for band in [1usize, 2] {
            build += 0.5 * (pw(&p[band], bin_s, maxf(a - BUILD_BARS as f64 * bar, 0.0), a) - pw(&p[band], bin_s, a, a + span));
        }
        maxf(v + STRENGTH_BUILD * build, 0.0)
    }

    /// Index of the beat nearest to t.
    fn beat_near(&self, t: f64) -> i64 {
        let b = &self.beats;
        let mut i = bsearch(b, t, true);
        if i > 0 && (i >= b.len() as i64 || t - at(b, i - 1) < at(b, i) - t) {
            i -= 1;
        }
        i
    }

    /// True when downbeat j is a bass entry after a gap.
    fn bass_entry(&self, p: &[f64], bin_s: f64, j: i64, bar: f64, loud: f64) -> bool {
        let d = at(&self.downbeats, j);
        let h = bar / 8.0;
        let gap = pw(p, bin_s, d - h, d);
        let depth = pw(p, bin_s, d - 2.0 * bar, d - h) - gap;
        let entry = pw(p, bin_s, d, d + h);
        if depth < LATE_GAP_DB || entry - gap < LATE_JUMP_DB || entry < loud - LATE_LOUD_DB {
            return false;
        }
        for q in (j - 4).max(0)..j {
            let e = at(&self.downbeats, q);
            if depth - (pw(p, bin_s, e - 2.0 * bar, e - h) - pw(p, bin_s, e - h, e)) < LATE_NOVEL_DB {
                return false;
            }
        }
        pw(p, bin_s, d, d + 4.0 * bar) - pw(p, bin_s, d - 4.0 * bar, d - h) >= LATE_LIFT_DB
    }

    /// Phrases: from each section start every PHRASE_BARS bars.
    fn phrases_pass(&mut self) {
        self.phrases.clear();
        if self.downbeats.is_empty() {
            return;
        }
        let mut starts = self.section_starts.clone();
        starts[0] = self.downbeats[0];
        let db = &self.downbeats;
        for s in 0..starts.len() {
            let i0 = bsearch(db, starts[s] as f64 - 0.01, true);
            let i1 = if s + 1 < starts.len() { bsearch(db, starts[s + 1] as f64 - 0.01, true) } else { db.len() as i64 };
            let mut i = i0;
            while i < i1 {
                if i == i0 || i1 - i >= PHRASE_BARS / 2 {
                    self.phrases.push(db[i as usize]);
                }
                i += PHRASE_BARS;
            }
        }
    }

    /// The grand finale: the last DROP run within FINALE_DB of the loudest.
    pub fn finale(&mut self) {
        self.finale_start = -1.0;
        self.finale_end = -1.0;
        let runs = Self::run_starts(&self.sections);
        let nr = runs.len();
        let mut mean = vec![0.0f32; nr];
        let mut top = f64::NEG_INFINITY;
        let any_drop = !self.drops.is_empty();
        let ss = &self.section_starts;
        let cand = |r: usize| -> bool {
            if any_drop { self.sections[runs[r] * 2 + 1] as i32 == DROP } else { r > 0 || nr == 1 }
        };
        for r in 0..nr {
            let a = ss[runs[r]] as f64;
            let b = if r + 1 < nr { ss[runs[r + 1]] as f64 } else { self.duration };
            mean[r] = (self.energy_mean(a, b, -1) * ENERGY_RANGE) as f32;
            if cand(r) {
                top = maxf(top, mean[r] as f64);
            }
        }
        if top == f64::NEG_INFINITY {
            return;
        }
        let (mut fs, mut fe) = (-1.0, -1.0);
        for r in 0..nr {
            if cand(r) && mean[r] as f64 >= top - FINALE_DB {
                fs = ss[runs[r]] as f64;
                fe = if r + 1 < nr { ss[runs[r + 1]] as f64 } else { self.duration };
            }
        }
        self.finale_start = fs;
        self.finale_end = fe;
    }

    /// The track's energy character (kick cover, drop step, fixed tempo).
    fn character_pass(&mut self) {
        let mut bars = 0i64;
        let mut kick_bars = 0i64;
        let mut floor_bars = 0i64;
        let nbt = self.beats.len() as i64;
        for i in 0..(nbt - 3).max(0) {
            if self.beat_bar[i as usize] != 0 {
                continue;
            }
            bars += 1;
            let mut c = 0i64;
            for j in i..i + 4 {
                let bj = self.beats[j as usize] as f64;
                let k = bsearch(&self.kicks, bj, true);
                for q in [k - 1, k] {
                    if q >= 0 && (q as usize) < self.kicks.len() && (at(&self.kicks, q) - bj).abs() <= KICK_ON_BEAT {
                        c += 1;
                        break;
                    }
                }
            }
            kick_bars += (c > 0) as i64;
            floor_bars += (c >= FLOOR_BAR_KICKS) as i64;
        }
        self.kick_cover = if bars > 0 { kick_bars as f64 / bars as f64 } else { 0.0 };
        self.four_floor = if kick_bars > 0 { floor_bars as f64 / kick_bars as f64 } else { 0.0 };
        self.drop_step = 0.0;
        let mut k = 2;
        while k < self.sections.len() {
            if self.sections[k + 1] as i32 != DROP || self.sections[k - 1] as i32 == DROP {
                k += 2;
                continue;
            }
            let d = self.sections[k] as f64;
            let before = self.energy_mean(maxf(d - STEP_S, 0.0), d - ENERGY_S, 0);
            let after = self.energy_mean(d, minf(d + STEP_S, self.duration), 0);
            self.drop_step = maxf(self.drop_step, (after - before) * ENERGY_RANGE);
            k += 2;
        }
        self.character = if self.fixed_tempo && self.kick_cover >= DANCE_COVER && self.drop_step >= DANCE_STEP_DB { DANCE } else { SONG };
    }

    /// Hook for the user's section labels: section k gets id ids[k], then the
    /// drops, their strengths and the finale follow. A DROP right after a DROP
    /// is one drop. `strength`: drop time -> strength (a drop not in it: 1.0).
    pub fn set_section_ids(&mut self, ids: &[i32], strength: &[(f32, f32)]) {
        for k in 0..ids.len().min(self.sections.len() / 2) {
            self.sections[k * 2 + 1] = ids[k] as f32;
        }
        self.section_starts.clear();
        self.drops.clear();
        self.drop_strength.clear();
        for k in (0..self.sections.len()).step_by(2) {
            self.section_starts.push(self.sections[k]);
            if self.sections[k + 1] as i32 == DROP && (k == 0 || self.sections[k - 1] as i32 != DROP) {
                let t = self.sections[k];
                self.drops.push(t);
                let s = strength.iter().find(|(x, _)| *x == t).map(|(_, s)| *s).unwrap_or(1.0);
                self.drop_strength.push(s);
            }
        }
        self.finale();
    }

    /// Rebuilds the derived fields after a load (`SongMap.from_dict`):
    /// downbeats from beat_bar, section starts and drops from the sections.
    pub fn derive(&mut self) {
        self.downbeats = (0..self.beats.len()).filter(|&i| self.beat_bar[i] == 0).map(|i| self.beats[i]).collect();
        self.section_starts.clear();
        self.drops.clear();
        for k in (0..self.sections.len()).step_by(2) {
            self.section_starts.push(self.sections[k]);
            if self.sections[k + 1] as i32 == DROP && (k == 0 || self.sections[k - 1] as i32 != DROP) {
                self.drops.push(self.sections[k]);
            }
        }
    }
}

/// Beat period (s) from the autocorrelation of the onset envelope.
fn tempo(env: &[f32], bin_s: f64) -> f64 {
    let n = env.len() / 2;
    let e: Vec<f32> = (0..n).map(|i| (env[2 * i] as f64 + env[2 * i + 1] as f64) as f32).collect();
    let hs = 2.0 * bin_s;
    let l0 = int(60.0 / MAX_BPM / hs) - 1;
    let l1 = int(60.0 / MIN_BPM / hs) + 2;
    let la = 2 * l1 + 2;
    let mut ac = vec![0.0f32; (la + 1) as usize];
    let ni = n as i64;
    for l in l0.max(1)..=la {
        let mut s = 0.0f64;
        for i in 0..(ni - l).max(0) {
            s += e[i as usize] as f64 * e[(i + l) as usize] as f64;
        }
        ac[l as usize] = s as f32;
    }
    let mut best: i64 = -1;
    let mut bv = 0.0f64;
    let a = |l: i64| ac[l as usize] as f64;
    for l in l0.max(2)..l1 {
        let p = l as f64 * hs;
        let w = gd::exp(-0.5 * gd::pow(gd::log(60.0 / p / PRIOR_BPM) / gd::log(2.0) / PRIOR_OCT, 2.0));
        if a(l) >= a(l - 1) && a(l) >= a(l + 1) {
            let sc = a(l) + ac_max(&ac, 2 * l);
            if sc * w > bv {
                bv = sc * w;
                best = l;
            }
        }
    }
    if best < 0 {
        return 0.5;
    }
    let den = a(best - 1) - 2.0 * a(best) + a(best + 1);
    let off = if den < 0.0 { gd::clampf(0.5 * (a(best - 1) - a(best + 1)) / den, -0.5, 0.5) } else { 0.0 };
    gd::clampf((best as f64 + off) * hs, 60.0 / MAX_BPM, 60.0 / MIN_BPM)
}

/// Largest autocorrelation within +-1 lag of l (0 outside).
fn ac_max(ac: &[f32], l: i64) -> f64 {
    let mut v = 0.0f64;
    for j in (l - 1).max(0)..(l + 2).min(ac.len() as i64) {
        v = maxf(v, ac[j as usize] as f64);
    }
    v
}

/// Folds the onsets in [t0, t1) onto one period p: (phase of the peak, its
/// height), each rounded to f32 (a GDScript Vector2).
#[allow(clippy::too_many_arguments)]
fn fold_peak(pt: &[f32], pw: &[f32], p: f64, t0: f64, t1: f64, hist: &mut [f32], rf: f64, search: f64) -> (f64, f64) {
    hist.fill(0.0);
    let inv = 1.0 / p;
    let j0 = bsearch(pt, t0, true) as usize;
    let j1 = if t1 < f64::INFINITY { bsearch(pt, t1, true) as usize } else { pt.len() };
    let pb = PHASE_BINS as f64;
    for j in j0..j1 {
        let ph = (pt[j] as f64 - rf) * inv;
        let idx = (int((ph - ph.floor()) * pb) % PHASE_BINS) as usize;
        hist[idx] = (hist[idx] as f64 + pw[j] as f64) as f32;
    }
    let mut bi = 0i64;
    let mut bv = -1.0f64;
    let reach = int(search * pb);
    for kk in -reach..=reach {
        let k = posmod(kk, PHASE_BINS);
        let v = hist[((k + PHASE_BINS - 1) % PHASE_BINS) as usize] as f64
            + 2.0 * hist[k as usize] as f64
            + hist[((k + 1) % PHASE_BINS) as usize] as f64;
        if v > bv {
            bv = v;
            bi = k;
        }
    }
    let c = (bi as f64 + 0.5) / pb;
    let mut sw = 0.0f64;
    let mut sd = 0.0f64;
    for j in j0..j1 {
        let ph = (pt[j] as f64 - rf) * inv;
        let mut d = ph - ph.floor() - c;
        d -= d.round();
        if d.abs() <= 1.5 / pb {
            sw += pw[j] as f64;
            sd += pw[j] as f64 * d;
        }
    }
    let mut phase = c + if sw > 0.0 { sd / sw } else { 0.0 };
    phase -= phase.round();
    (v32(phase * p), v32(bv * 0.25))
}

/// Bass entries per slot: per grid time the low band's step; per slot the
/// sum of its BAR_TOP largest steps.
fn entry_steps(low: &[f32], bin_s: f64, grid: &[f32], span: f64, slots: usize) -> Vec<f32> {
    let nb = low.len() as i64;
    let mut e = vec![0.0f64; nb as usize + 1];
    for i in 0..nb as usize {
        e[i + 1] = e[i] + low[i] as f64;
    }
    let w = int(span / bin_s);
    let mut steps: Vec<Vec<f32>> = vec![Vec::new(); slots];
    for i in 0..grid.len() {
        let c = clampi(int(grid[i] as f64 / bin_s), 0, nb - 1);
        let a0 = (c - w).max(0);
        let a1 = (c + w).min(nb);
        let mut step = 0.0f64;
        if c - a0 > 0 && a1 - c > 0 {
            let (a0, a1, cu) = (a0 as usize, a1 as usize, c as usize);
            step = (e[a1] - e[cu]) / (a1 - cu) as f64 - (e[cu] - e[a0]) / (cu - a0) as f64;
        }
        steps[i % slots].push(maxf(step, 0.0) as f32);
    }
    let mut top = vec![0.0f32; slots];
    for s in 0..slots {
        let v = &mut steps[s];
        sort_f32(v);
        let vl = v.len() as i64;
        for j in (vl - 4i64.max(int(vl as f64 * BAR_TOP))).max(0)..vl {
            top[s] = (top[s] as f64 + v[j as usize] as f64) as f32;
        }
    }
    top
}

/// Mean power (dB) over [t0, t1) from a prefix sum of the power per bin.
fn pw(p: &[f64], bin_s: f64, t0: f64, t1: f64) -> f64 {
    let nb = p.len() as i64 - 1;
    let i0 = clampi(int(t0 / bin_s), 0, nb - 1);
    let i1 = clampi(int(t1 / bin_s), i0 + 1, nb);
    10.0 * gd::log((p[i1 as usize] - p[i0 as usize]) / (i1 - i0) as f64 + 1e-12) / gd::log(10.0)
}

fn is_peak(r: &[f32], i: usize) -> bool {
    let v = r[i];
    let ii = i as i64;
    for j in (ii - PEAK_BINS).max(0)..(ii + PEAK_BINS + 1).min(r.len() as i64) {
        let rj = r[j as usize];
        if rj > v || (rj == v && j < ii) {
            return false;
        }
    }
    true
}

fn local_fit(t: &[f32], keep: &[u8], i: i64) -> f64 {
    let (mut sw, mut sx, mut sy, mut sxx, mut sxy) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for j in (i - FIT_BEATS).max(0)..(i + FIT_BEATS + 1).min(t.len() as i64) {
        if keep[j as usize] == 0 {
            continue;
        }
        let x = (j - i) as f64;
        let y = t[j as usize] as f64;
        sw += 1.0;
        sx += x;
        sy += y;
        sxx += x * x;
        sxy += x * y;
    }
    let den = sw * sxx - sx * sx;
    if sw < 3.0 || den.abs() < 1e-9 {
        return t[i as usize] as f64;
    }
    let slope = (sw * sxy - sx * sy) / den;
    (sy - slope * sx) / sw
}

/// Mean over +-r (prefix sums), f32 out.
fn running_mean(v: &[f32], r: i64) -> Vec<f32> {
    let n = v.len() as i64;
    let mut c = vec![0.0f64; v.len() + 1];
    for i in 0..v.len() {
        c[i + 1] = c[i] + v[i] as f64;
    }
    (0..n)
        .map(|i| {
            let a = (i - r).max(0);
            let b = (i + r + 1).min(n);
            ((c[b as usize] - c[a as usize]) / (b - a) as f64) as f32
        })
        .collect()
}

/// v minus its running mean (+-r), clipped at 0.
fn minus_mean(v: &[f32], r: i64) -> Vec<f32> {
    let m = running_mean(v, r);
    v.iter().zip(&m).map(|(&x, &y)| maxf(x as f64 - y as f64, 0.0) as f32).collect()
}
