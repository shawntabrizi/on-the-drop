//! Song structure from the 3-band levels: calm / build / drop / breakdown
//! (port of frame-visualizer `lib/song_sections.gd`, `SongSections`).
//!
//! 1. Frames of [`FRAME_S`]: each band in dB under its own 95th percentile.
//! 2. A running median over [`MED_S`] per band (fills vanish, steps stay).
//! 3. Novelty (distance of the band means of the [`NOV_S`] before and after
//!    each frame); peaks >= [`NOV_MIN`] dB at least [`MIN_SEG_S`] apart are
//!    boundaries, moved to the sharpest energy step nearby, then onto a beat.
//! 4. Labels from each segment's energy, low band and trend against the
//!    song's own loud level (see the GDScript source for the full rules).

use crate::gd::{self, at, clampi, int, maxf, sort_f32};

pub const FRAME_S: f64 = 0.5;
pub const FLOOR: f64 = -40.0;
pub const TOP_PCT: f64 = 0.95;
pub const MED_S: f64 = 7.0;
pub const NOV_S: f64 = 6.0;
pub const NOV_MIN: f64 = 3.0;
pub const MIN_SEG_S: f64 = 8.0;
pub const SNAP_S: f64 = 2.0;
/// s: a boundary moves onto the nearest beat where beats are closer.
pub const MAX_BEAT_GAP: f64 = 1.0;
pub const LOUD_PCT: f64 = 0.9;
pub const HIGH_DB: f64 = 3.5;
pub const LOW_DB: f64 = 6.0;
pub const JUMP_DB: f64 = 3.5;
pub const LOW_JUMP_DB: f64 = 6.0;
pub const RISE_DB: f64 = 1.5;
/// A rise of the high band counts half (3 dB of highs = a build).
pub const HIGH_RISE: f64 = 0.5;
/// The later HIGH segment's low band is this much over the earlier one's.
pub const BASS_IN_DB: f64 = 3.0;
/// ... and the low band steps up this much at the boundary.
pub const BASS_STEP_DB: f64 = 2.0;
pub const BASS_STEP_BARS: f64 = 2.0;
/// A HIGH segment the bass entered by this much is itself the entry.
pub const ENTRY_DB: f64 = 12.0;

/// Section ids (= frame-visualizer `BandMath.SectionTracker` ids).
pub const CALM: i32 = 0;
pub const BUILD: i32 = 1;
pub const DROP: i32 = 2;
pub const BREAKDOWN: i32 = 3;

const GRADUAL: i32 = 99;

struct Seg {
    a: i64,
    b: i64,
    m: f64,
    lo: f64,
    rise: f64,
    high: bool,
    foot: i64,
}

/// raw: RMS per bin, 3 per bin (low, mid, high); bin_s: s per bin; beats: s.
/// Returns the section changes `[t0, id0, t1, id1, ...]`, t0 = 0.
pub fn analyse(raw: &[f32], nb: usize, bin_s: f64, beats: &[f32]) -> Vec<f32> {
    let per = 1i64.max(gd::roundi(FRAME_S / bin_s));
    let fs = per as f64 * bin_s;
    let n = nb as i64 / per;
    if n < 4 {
        return vec![0.0, CALM as f32];
    }
    let nu = n as usize;
    // 1. band dB per frame, under the band's 95th percentile
    let mut d: Vec<Vec<f32>> = vec![vec![0.0; nu]; 3];
    for b in 0..3 {
        for f in 0..nu {
            let mut p = 0.0f64;
            for k in 0..per as usize {
                let v = raw[(f * per as usize + k) * 3 + b] as f64;
                p += v * v;
            }
            d[b][f] = (10.0 * gd::log(p / per as f64 + 1e-12) / gd::log(10.0)) as f32;
        }
        let top = pct(&d[b], TOP_PCT);
        for f in 0..nu {
            d[b][f] = gd::clampf(d[b][f] as f64 - top, FLOOR, 3.0) as f32;
        }
    }
    let e_raw: Vec<f32> = (0..nu)
        .map(|f| ((d[0][f] as f64 + d[1][f] as f64 + d[2][f] as f64) / 3.0) as f32)
        .collect();
    // 2. running median
    let med = int(MED_S / fs / 2.0);
    let sm: Vec<Vec<f32>> = (0..3).map(|b| median(&d[b], med)).collect();
    let e: Vec<f32> = (0..nu)
        .map(|f| ((sm[0][f] as f64 + sm[1][f] as f64 + sm[2][f] as f64) / 3.0) as f32)
        .collect();
    // 3. novelty and boundaries
    let w = 2i64.max(int(NOV_S / fs));
    let min_seg = 2i64.max(int(MIN_SEG_S / fs));
    let mut nov = vec![0.0f32; nu];
    for f in w..n - w {
        let mut s2 = 0.0f64;
        for b in 0..3 {
            let mut diff = 0.0f64;
            for k in 0..w {
                diff += sm[b][(f + k) as usize] as f64 - sm[b][(f - 1 - k) as usize] as f64;
            }
            s2 += (diff / w as f64) * (diff / w as f64);
        }
        nov[f as usize] = gd::sqrt(s2) as f32;
    }
    let mut cand: Vec<i64> = Vec::new();
    for f in 1..n - 1 {
        let (v, l, r) = (nov[f as usize], nov[f as usize - 1], nov[f as usize + 1]);
        if v as f64 >= NOV_MIN && v >= l && v >= r {
            cand.push(f);
        }
    }
    cand.sort_by(|&x, &y| {
        let (a, b) = (nov[x as usize], nov[y as usize]);
        b.partial_cmp(&a).unwrap().then(x.cmp(&y))
    });
    let mut bounds: Vec<i64> = Vec::new();
    for &f in &cand {
        if f < min_seg / 2 || f > n - min_seg / 2 {
            continue;
        }
        if bounds.iter().all(|&g| (f - g).abs() >= min_seg) {
            bounds.push(f);
        }
    }
    bounds.sort();
    // move each boundary to the sharpest step of the raw energy nearby
    let snap = int(SNAP_S / fs);
    let mut edges: Vec<i64> = vec![0];
    for &f in &bounds {
        let up = mean(&e, f, f + w) > mean(&e, f - w, f);
        let mut best = f;
        let mut bv = f64::NEG_INFINITY;
        for j in 1i64.max(f - snap)..n.min(f + snap + 1) {
            let (ju, jp) = (j as usize, j as usize - 1);
            let mut step = e_raw[ju] as f64 - e_raw[jp] as f64 + d[0][ju] as f64 - d[0][jp] as f64;
            if !up {
                step = -step;
            }
            if step > bv {
                bv = step;
                best = j;
            }
        }
        if best - *edges.last().unwrap() >= min_seg / 2 && n - best >= min_seg / 2 {
            edges.push(best);
        }
    }
    edges.push(n);
    // 4. segments and labels
    let loud = pct(&e, LOUD_PCT);
    let low_loud = pct(&sm[0], LOUD_PCT);
    let mut segs: Vec<Seg> = Vec::new();
    for i in 0..edges.len() - 1 {
        let (a, b) = (edges[i], edges[i + 1]);
        let k = 1i64.max((b - a) / 3);
        let m = mean(&e, a, b);
        let lo = mean(&sm[0], a, b);
        let rise = maxf(
            mean(&e, b - k, b) - mean(&e, a, a + k),
            (mean(&sm[2], b - k, b) - mean(&sm[2], a, a + k)) * HIGH_RISE,
        );
        segs.push(Seg { a, b, m, lo, rise, high: m >= loud - HIGH_DB && lo >= low_loud - LOW_DB, foot: -1 });
    }
    let ns = segs.len();
    let bar = if beats.len() > 1 {
        4.0 * (beats[beats.len() - 1] as f64 - beats[0] as f64) / (beats.len() - 1) as f64
    } else {
        2.0
    };
    let step_w = 1i64.max(int(BASS_STEP_BARS * bar / fs));
    let mut lab = vec![-1i32; ns];
    if segs[0].high {
        lab[0] = GRADUAL;
    }
    for i in 0..ns {
        if !segs[i].high || i == 0 {
            continue;
        }
        let (s, p) = (&segs[i], &segs[i - 1]);
        let jump = s.m - p.m >= JUMP_DB || s.lo - p.lo >= LOW_JUMP_DB;
        // short-circuit as GDScript: the step is only computed when the low band rose
        let bass_in = s.lo - p.lo >= BASS_IN_DB
            && mean(&d[0], s.a, s.a + step_w) - mean(&d[0], s.a - step_w, s.a) >= BASS_STEP_DB;
        let entry = i >= 2 && p.lo - segs[i - 2].lo >= ENTRY_DB;
        if p.high && bass_in && !entry {
            lab[i - 1] = BUILD;
            lab[i] = DROP;
        } else if p.high {
            lab[i] = if lab[i - 1] == DROP || jump { DROP } else { lab[i - 1] };
        } else {
            lab[i] = if jump || p.rise >= RISE_DB { DROP } else { GRADUAL };
        }
    }
    let mut dropped = false;
    for i in 0..ns {
        if lab[i] == GRADUAL {
            let later = lab[i + 1..].iter().any(|&l| l == DROP);
            lab[i] = if later && !dropped { BUILD } else { DROP };
        }
        dropped = dropped || lab[i] == DROP;
    }
    let mut first = ns as i64;
    let mut last = -1i64;
    for i in 0..ns {
        if lab[i] == DROP {
            first = first.min(i as i64);
            last = i as i64;
        }
    }
    for i in (0..ns).rev() {
        if lab[i] != -1 {
            continue;
        }
        let nxt = if i + 1 < ns { lab[i + 1] } else { -1 };
        if i > 0 && (nxt == DROP || nxt == BUILD) && (segs[i].rise >= RISE_DB || lab[i - 1] == BUILD) {
            lab[i] = BUILD;
        } else {
            let ii = i as i64;
            lab[i] = if first < ii && ii < last { BREAKDOWN } else { CALM };
        }
    }
    // a long build starts at the knee of its rise
    for i in 0..ns {
        let s = &segs[i];
        if lab[i] != BUILD || s.high || s.rise < RISE_DB || s.b - s.a <= 2 * min_seg {
            continue;
        }
        let k = (s.b - s.a) / 3;
        let mut band = 0usize;
        let mut most = f64::NEG_INFINITY;
        for bd in 0..3 {
            let r = mean(&sm[bd], s.b - k, s.b) - mean(&sm[bd], s.a, s.a + k);
            if r > most {
                most = r;
                band = bd;
            }
        }
        let j = knee(&sm[band], s.a, s.b, min_seg / 2);
        if j > s.a {
            segs[i].foot = j;
        }
    }
    // changes (start frame, id), merged, then timed on a beat
    let mut ch: Vec<(i64, i32)> = Vec::new();
    for i in 0..ns {
        let s = &segs[i];
        if s.foot >= 0 {
            let pre = if first < i as i64 {
                BREAKDOWN
            } else if i > 0 && lab[i - 1] == BUILD {
                BUILD
            } else {
                CALM
            };
            ch.push((s.a, pre));
            ch.push((s.foot, lab[i]));
        } else {
            ch.push((s.a, lab[i]));
        }
    }
    let mut out: Vec<f32> = Vec::new();
    for (f, id) in ch {
        if out.is_empty() || int(out[out.len() - 1] as f64) as i32 != id {
            let t = if out.is_empty() { 0.0 } else { on_beat(f as f64 * fs, beats) };
            out.push(t as f32);
            out.push(id as f32);
        }
    }
    out
}

/// Split j of v[a, b) that best fits "flat on [a, j), then a rising line on
/// [j, b)"; a if none rises.
fn knee(v: &[f32], a: i64, b: i64, margin: i64) -> i64 {
    let n = (b - a) as usize;
    let mut se = vec![0.0f64; n + 1];
    let mut see = vec![0.0f64; n + 1];
    let mut sx = vec![0.0f64; n + 1];
    let mut sxx = vec![0.0f64; n + 1];
    let mut sxe = vec![0.0f64; n + 1];
    for k in 0..n {
        let y = v[a as usize + k] as f64;
        se[k + 1] = se[k] + y;
        see[k + 1] = see[k] + y * y;
        sx[k + 1] = sx[k] + k as f64;
        sxx[k + 1] = sxx[k] + (k * k) as f64;
        sxe[k + 1] = sxe[k] + k as f64 * y;
    }
    let mut best = a;
    let mut best_cost = f64::INFINITY;
    let n = n as i64;
    for j in margin..n - margin {
        let ju = j as usize;
        let nu = n as usize;
        let m1 = j as f64;
        let c1 = see[ju] - se[ju] * se[ju] / m1;
        let m2 = (n - j) as f64;
        let e2 = se[nu] - se[ju];
        let x2 = sx[nu] - sx[ju];
        let vxx = sxx[nu] - sxx[ju] - x2 * x2 / m2;
        let vxe = sxe[nu] - sxe[ju] - x2 * e2 / m2;
        if vxx <= 0.0 || vxe <= 0.0 {
            continue;
        }
        let c2 = see[nu] - see[ju] - e2 * e2 / m2 - vxe * vxe / vxx;
        if c1 + c2 < best_cost {
            best_cost = c1 + c2;
            best = a + j;
        }
    }
    best
}

/// t moved onto the nearest beat when the beat grid covers t, else t.
fn on_beat(t: f64, beats: &[f32]) -> f64 {
    let mut best: i64 = -1;
    for i in 0..beats.len() as i64 {
        if best < 0 || (at(beats, i) - t).abs() < (at(beats, best) - t).abs() {
            best = i;
        }
    }
    if best < 0 {
        return t;
    }
    let mut gap = f64::INFINITY;
    if best > 0 {
        gap = at(beats, best) - at(beats, best - 1);
    }
    if best + 1 < beats.len() as i64 {
        gap = gd::minf(gap, at(beats, best + 1) - at(beats, best));
    }
    if gap > MAX_BEAT_GAP || (at(beats, best) - t).abs() > 0.5 * gap + 0.01 {
        return t;
    }
    at(beats, best)
}

fn pct(v: &[f32], q: f64) -> f64 {
    let mut s = v.to_vec();
    sort_f32(&mut s);
    let n = s.len() as i64;
    s[clampi(int(q * (n - 1) as f64), 0, n - 1) as usize] as f64
}

fn mean(v: &[f32], a: i64, b: i64) -> f64 {
    let a = a.max(0);
    let b = b.min(v.len() as i64);
    if b <= a {
        return v[clampi(a, 0, v.len() as i64 - 1) as usize] as f64;
    }
    let mut s = 0.0f64;
    for i in a..b {
        s += v[i as usize] as f64;
    }
    s / (b - a) as f64
}

/// Running median, r frames each side (clamped at the ends).
fn median(v: &[f32], r: i64) -> Vec<f32> {
    let n = v.len() as i64;
    let mut out = vec![0.0f32; v.len()];
    let mut win: Vec<f32> = Vec::new();
    for i in 0..n {
        win.clear();
        win.extend_from_slice(&v[0.max(i - r) as usize..n.min(i + r + 1) as usize]);
        sort_f32(&mut win);
        out[i as usize] = win[win.len() / 2];
    }
    out
}
