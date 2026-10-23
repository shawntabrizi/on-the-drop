//! Band bins from decoded PCM (port of frame-visualizer `lib/song_overview.gd`,
//! `SongOverview.build` / `set_levels`).
//!
//! Per [`BIN_FRAMES`] samples: the RMS of the low (< [`LOW_HZ`]), mid and high
//! (> [`HIGH_HZ`]) band and of the "air" band (the high band's first
//! difference, for hats). Bands: 2nd-order Butterworth biquads (RBJ audio EQ
//! cookbook), direct form I, all state in f64 as in GDScript; bin values are
//! f32. A trailing partial bin is dropped. Decoding is not part of the core:
//! the caller passes the frames its decoder produced.

use crate::gd;

/// Samples per bin (11.6 ms at 44.1 kHz).
pub const BIN_FRAMES: usize = 512;
pub const LOW_HZ: f64 = 200.0;
pub const HIGH_HZ: f64 = 2500.0;
/// Per-band percentile the overview levels are normalised to.
pub const NORM_PCT: f64 = 0.98;

/// Band bins: the input of [`crate::SongMap::analyse`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bins {
    /// RMS per bin: low, mid, high (3 values per bin).
    pub raw: Vec<f32>,
    /// RMS per bin of the air band (one per bin).
    pub air: Vec<f32>,
    /// Number of bins.
    pub nb: usize,
    /// Seconds per bin (`BIN_FRAMES / rate`).
    pub bin_s: f64,
}

/// 2nd-order Butterworth low- or high-pass coefficients (RBJ, Q = 1/sqrt 2).
struct Biquad {
    b0: f64,
    b1: f64,
    a1: f64,
    a2: f64,
}

impl Biquad {
    fn new(high: bool, hz: f64, rate: f64) -> Self {
        let w = std::f64::consts::TAU * hz / rate;
        let alpha = gd::sin(w) / (2.0 * gd::sqrt(0.5));
        let c = gd::cos(w);
        let a0 = 1.0 + alpha;
        let (b0, b1) = if high {
            ((1.0 + c) * 0.5 / a0, -(1.0 + c) / a0)
        } else {
            ((1.0 - c) * 0.5 / a0, (1.0 - c) / a0)
        };
        Biquad { b0, b1, a1: -2.0 * c / a0, a2: (1.0 - alpha) / a0 }
    }
}

/// Bins from mono samples (`x` is used as the mono mix).
pub fn bins_mono(samples: &[f32], rate: f64) -> Bins {
    bins_from(samples.iter().map(|&x| x as f64), rate)
}

/// Bins from interleaved stereo frames (L, R, L, R, ...): the mono mix is
/// `(l + r) * 0.5` in f64, as `SongOverview.build` mixes Godot's frames.
pub fn bins_stereo(interleaved: &[f32], rate: f64) -> Bins {
    bins_from(
        interleaved.chunks_exact(2).map(|f| (f[0] as f64 + f[1] as f64) * 0.5),
        rate,
    )
}

fn bins_from(mono: impl Iterator<Item = f64>, rate: f64) -> Bins {
    let lo = Biquad::new(false, LOW_HZ, rate);
    let mh = Biquad::new(true, LOW_HZ, rate);
    let ml = Biquad::new(false, HIGH_HZ, rate);
    let hi = Biquad::new(true, HIGH_HZ, rate);
    let (lb0, lb1, la1, la2) = (lo.b0, lo.b1, lo.a1, lo.a2);
    let (hb0, hb1, ha1, ha2) = (hi.b0, hi.b1, hi.a1, hi.a2);
    let (pb0, pb1, pa1, pa2) = (mh.b0, mh.b1, mh.a1, mh.a2);
    let (qb0, qb1, qa1, qa2) = (ml.b0, ml.b1, ml.a1, ml.a2);
    let (mut x1, mut x2) = (0.0f64, 0.0f64);
    let (mut l1, mut l2) = (0.0f64, 0.0f64);
    let (mut h1, mut h2) = (0.0f64, 0.0f64);
    let (mut p1, mut p2) = (0.0f64, 0.0f64);
    let (mut q1, mut q2) = (0.0f64, 0.0f64);
    let (mut sl, mut sm, mut sh, mut sa) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    let mut n = 0usize;
    let mut raw = Vec::new();
    let mut air = Vec::new();
    for x in mono {
        // b2 = b0 for both low- and high-pass.
        let l = lb0 * (x + x2) + lb1 * x1 - la1 * l1 - la2 * l2;
        let h = hb0 * (x + x2) + hb1 * x1 - ha1 * h1 - ha2 * h2;
        let p = pb0 * (x + x2) + pb1 * x1 - pa1 * p1 - pa2 * p2;
        let m = qb0 * (p + p2) + qb1 * p1 - qa1 * q1 - qa2 * q2;
        let r = h - h1;
        x2 = x1;
        x1 = x;
        l2 = l1;
        l1 = l;
        h2 = h1;
        h1 = h;
        p2 = p1;
        p1 = p;
        q2 = q1;
        q1 = m;
        sl += l * l;
        sm += m * m;
        sh += h * h;
        sa += r * r;
        n += 1;
        if n == BIN_FRAMES {
            let nf = n as f64;
            raw.push(gd::sqrt(sl / nf) as f32);
            raw.push(gd::sqrt(sm / nf) as f32);
            raw.push(gd::sqrt(sh / nf) as f32);
            air.push(gd::sqrt(sa / nf) as f32);
            sl = 0.0;
            sm = 0.0;
            sh = 0.0;
            sa = 0.0;
            n = 0;
        }
    }
    let nb = air.len();
    Bins { raw, air, nb, bin_s: BIN_FRAMES as f64 / rate }
}

/// The overview strip: per bin and band (low, mid, high) a byte 0..255,
/// normalised per band to its [`NORM_PCT`] percentile (`set_levels`).
pub fn levels(bins: &Bins) -> Vec<u8> {
    let nb = bins.nb;
    let mut out = vec![0u8; nb * 3];
    if nb == 0 {
        return out;
    }
    for b in 0..3 {
        let mut col: Vec<f32> = (0..nb).map(|i| bins.raw[i * 3 + b]).collect();
        gd::sort_f32(&mut col);
        let n = nb as i64;
        let top = gd::maxf(col[gd::clampi(gd::int(NORM_PCT * (n - 1) as f64), 0, n - 1) as usize] as f64, 1e-6);
        for i in 0..nb {
            out[i * 3 + b] = gd::clampi(gd::int(bins.raw[i * 3 + b] as f64 / top * 255.0), 0, 255) as u8;
        }
    }
    out
}
