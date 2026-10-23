//! Part boundaries from a beat-synchronous self-similarity matrix (port of
//! frame-visualizer `lib/song_novelty.gd`, `SongNovelty`): where the sound
//! of the song changes, also when its loudness does not.
//!
//! Method (Foote 2000; beat-synchronous features as in Bartsch & Wakefield
//! 2001 and McFee & Ellis 2014; multi-scale kernels as in Kaiser & Peeters
//! 2013):
//! 1. Features per beat: each band's mean power over the beat in dB under the
//!    band's [`BEAT_TOP`] percentile, clamped at [`FLOOR_DB`], z-scored.
//! 2. A banded SSM of Euclidean distances (|i - j| < 2 x the largest kernel).
//! 3. Novelty at each downbeat for Gaussian-tapered checkerboard kernels.
//! 4. Scales [`KERNELS`], each over its median, then their mean.
//! 5. Peaks: local maxima within [`PEAK_WIN`] downbeats, >= [`PEAK_REL`] x the
//!    median, >= [`NOV_ABS`] plain novelty, [`MIN_BARS`] bars apart.

use crate::gd::{self, clampi, int, maxf, sort_f32};

/// Beats each side (4 and 8 bars of 4/4).
pub const KERNELS: [usize; 2] = [16, 32];
pub const BEAT_TOP: f64 = 0.98;
pub const FLOOR_DB: f64 = -40.0;
pub const PEAK_WIN: i64 = 2;
pub const PEAK_REL: f64 = 1.5;
pub const MIN_BARS: i64 = 4;
pub const SD_MIN_DB: f64 = 1.5;
pub const NOV_ABS: f64 = 0.25;

/// Band levels per beat, z-scored per band: (beats - 1) x bands values
/// (beat j, band b at j * bands + b). power[b]: per-bin power of band b.
pub fn beat_features(power: &[Vec<f32>], bin_s: f64, beats: &[f32]) -> Vec<f32> {
    let nbands = power.len();
    let n = beats.len() as i64 - 1;
    if n < 2 || nbands == 0 {
        return Vec::new();
    }
    let nu = n as usize;
    let mut out = vec![0.0f32; nu * nbands];
    let nb = power[0].len() as i64;
    for b in 0..nbands {
        let pw = &power[b];
        let mut col = vec![0.0f32; nu];
        for j in 0..nu {
            let i0 = clampi(int(beats[j] as f64 / bin_s), 0, nb - 1);
            let i1 = clampi(int(beats[j + 1] as f64 / bin_s), i0 + 1, nb);
            let mut s = 0.0f64;
            for i in i0..i1 {
                s += pw[i as usize] as f64;
            }
            col[j] = (4.342944819 * gd::log(s / (i1 - i0) as f64 + 1e-12)) as f32;
        }
        let mut srt = col.clone();
        sort_f32(&mut srt);
        let top = srt[clampi(int(BEAT_TOP * (n - 1) as f64), 0, n - 1) as usize] as f64;
        let mut mean = 0.0f64;
        for j in 0..nu {
            col[j] = maxf(col[j] as f64 - top, FLOOR_DB) as f32;
            mean += col[j] as f64;
        }
        mean /= n as f64;
        let mut v = 0.0f64;
        for j in 0..nu {
            v += (col[j] as f64 - mean) * (col[j] as f64 - mean);
        }
        let sd = maxf(gd::sqrt(v / n as f64), SD_MIN_DB);
        for j in 0..nu {
            out[j * nbands + b] = ((col[j] as f64 - mean) / sd) as f32;
        }
    }
    out
}

/// Novelty at each beat index in `at`: (the mean over KERNELS of each scale
/// over its median, the plain mean over KERNELS in feature units).
pub fn novelty(f: &[f32], nbands: usize, at: &[i32]) -> (Vec<f32>, Vec<f32>) {
    let n = (f.len() / nbands) as i64;
    let lmax = *KERNELS.iter().max().unwrap() as i64;
    let w = 2 * lmax;
    let mut d = vec![0.0f32; (n * w) as usize];
    for i in 0..n {
        for lag in 1..w.min(n - i) {
            let mut s = 0.0f64;
            for b in 0..nbands {
                let x = f[i as usize * nbands + b] as f64 - f[(i + lag) as usize * nbands + b] as f64;
                s += x * x;
            }
            d[(i * w + lag) as usize] = gd::sqrt(s) as f32;
        }
    }
    let nk = KERNELS.len() as f64;
    let mut total = vec![0.0f32; at.len()];
    let mut plain = vec![0.0f32; at.len()];
    for &l in KERNELS.iter() {
        let li = l as i64;
        let mut g = vec![0.0f32; l];
        let mut gs = 0.0f64;
        for k in 0..l {
            g[k] = gd::exp(-0.5 * gd::pow((k as f64 + 0.5) / (0.5 * l as f64), 2.0)) as f32;
            gs += g[k] as f64;
        }
        let mut nov = vec![0.0f32; at.len()];
        let mut pos: Vec<f32> = Vec::new();
        for q in 0..at.len() {
            let i = at[q] as i64;
            if i < li || i > n - li {
                continue;
            }
            let mut cross = 0.0f64;
            let mut within = 0.0f64;
            for ka in 0..li {
                let a = i - 1 - ka;
                let ga = g[ka as usize] as f64;
                for kb in 0..li {
                    cross += ga * g[kb as usize] as f64 * d[(a * w + (i + kb - a)) as usize] as f64;
                }
                for kc in ka + 1..li {
                    within += ga * g[kc as usize] as f64 * d[((i - 1 - kc) * w + (kc - ka)) as usize] as f64;
                }
                for kc in ka + 1..li {
                    within += ga * g[kc as usize] as f64 * d[((i + ka) * w + (kc - ka)) as usize] as f64;
                }
            }
            nov[q] = ((2.0 * cross - 2.0 * within) / (gs * gs)) as f32;
            plain[q] = (plain[q] as f64 + nov[q] as f64 / nk) as f32;
            if nov[q] > 0.0 {
                pos.push(nov[q]);
            }
        }
        if pos.is_empty() {
            continue;
        }
        sort_f32(&mut pos);
        let med = maxf(pos[pos.len() / 2] as f64, 1e-9);
        for q in 0..at.len() {
            total[q] = (total[q] as f64 + nov[q] as f64 / med / nk) as f32;
        }
    }
    (total, plain)
}

/// The boundaries (s, on downbeats, sorted) of a song: power per band per
/// bin, the beat grid and its downbeat flags (beat_bar 0).
pub fn boundaries(power: &[Vec<f32>], bin_s: f64, beats: &[f32], beat_bar: &[u8]) -> Vec<f32> {
    let mut out = Vec::new();
    let f = beat_features(power, bin_s, beats);
    if f.is_empty() {
        return out;
    }
    let mut at: Vec<i32> = Vec::new();
    for i in 0..(beats.len() - 1).min(beat_bar.len()) {
        if beat_bar[i] == 0 {
            at.push(i as i32);
        }
    }
    let (nov, plain) = novelty(&f, power.len(), &at);
    let mut pos: Vec<f32> = nov.iter().copied().filter(|&v| v > 0.0).collect();
    if pos.is_empty() {
        return out;
    }
    sort_f32(&mut pos);
    let thr = PEAK_REL * pos[pos.len() / 2] as f64;
    let mut cand: Vec<usize> = Vec::new();
    let nn = nov.len() as i64;
    for q in 0..nov.len() {
        if nov[q] <= 0.0 || (nov[q] as f64) < thr || (plain[q] as f64) < NOV_ABS {
            continue;
        }
        let qi = q as i64;
        let top = ((qi - PEAK_WIN).max(0)..(qi + PEAK_WIN + 1).min(nn)).all(|r| nov[r as usize] <= nov[q]);
        if top {
            cand.push(q);
        }
    }
    cand.sort_by(|&x, &y| nov[y].partial_cmp(&nov[x]).unwrap().then(x.cmp(&y)));
    let mut keep: Vec<usize> = Vec::new();
    for &q in &cand {
        if keep.iter().all(|&k| (at[q] as i64 - at[k] as i64).abs() >= 4 * MIN_BARS) {
            keep.push(q);
        }
    }
    keep.sort();
    for q in keep {
        out.push(beats[at[q] as usize]);
    }
    out
}
