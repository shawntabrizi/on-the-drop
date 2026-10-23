#![allow(dead_code)]

pub mod synth;

use on_the_drop::SongMap;

pub const HIT_TOL: f64 = 0.03;

/// The map of a synthetic track as the app builds it (decoded at 44.1 kHz).
pub fn build(s: &synth::Synth) -> SongMap {
    on_the_drop::analyze_mono(&s.pcm_44k(), 44100.0).expect("the synthetic track decodes").map
}

/// Signed offset of the nearest time in `a` to t (a[j] - t), INF if empty.
pub fn nearest(a: &[f32], t: f64) -> f64 {
    let i = a.partition_point(|&x| x < t as f32) as i64;
    let mut best = f64::INFINITY;
    for j in [i - 1, i] {
        if j >= 0 && (j as usize) < a.len() && (a[j as usize] as f64 - t).abs() < best.abs() {
            best = a[j as usize] as f64 - t;
        }
    }
    best
}

/// (matched, false, median |error|, max |error|) of detected vs true times.
pub fn match_times(det: &[f32], truth: &[f32]) -> (usize, usize, f64, f64) {
    let mut errs: Vec<f64> = truth
        .iter()
        .map(|&t| nearest(det, t as f64).abs())
        .filter(|e| *e <= HIT_TOL)
        .collect();
    let fp = det.iter().filter(|&&d| nearest(truth, d as f64).abs() > HIT_TOL).count();
    errs.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let med = if errs.is_empty() { f64::INFINITY } else { errs[errs.len() / 2] };
    let max = errs.last().copied().unwrap_or(f64::INFINITY);
    (errs.len(), fp, med, max)
}
