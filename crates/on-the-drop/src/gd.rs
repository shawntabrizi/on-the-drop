//! GDScript number semantics, so the port gives the same bits as
//! frame-visualizer's GDScript analysis.
//!
//! - GDScript `float` scalars are f64; `PackedFloat32Array` elements and
//!   `Vector2` / `Vector3` components are f32. The port keeps the same
//!   types: every store into a packed array or vector rounds to f32.
//! - `maxf` / `minf` / `clampf` are Godot's `MAX` / `MIN` / `CLAMP`
//!   (plain comparisons, not IEEE max/min).
//! - `PackedFloat32Array.bsearch(value)` converts `value` to f32 first.
//! - Transcendental functions go through [`std::hint::black_box`] so LLVM
//!   cannot constant-fold them or rewrite `pow(10, x)` as `exp10(x)`: Godot
//!   calls the C library at run time, and so must we.

use std::hint::black_box;

#[inline]
pub fn maxf(a: f64, b: f64) -> f64 {
    if a > b { a } else { b }
}

#[inline]
pub fn minf(a: f64, b: f64) -> f64 {
    if a < b { a } else { b }
}

#[inline]
pub fn clampf(x: f64, lo: f64, hi: f64) -> f64 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
pub fn clampi(x: i64, lo: i64, hi: i64) -> i64 {
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

#[inline]
pub fn posmod(x: i64, y: i64) -> i64 {
    let mut v = x % y;
    if (v < 0 && y > 0) || (v > 0 && y < 0) {
        v += y;
    }
    v
}

/// GDScript `int(x)`: truncation toward zero.
#[inline]
pub fn int(x: f64) -> i64 {
    x as i64
}

/// GDScript `roundi(x)` / `int(round(x))`: half away from zero.
#[inline]
pub fn roundi(x: f64) -> i64 {
    x.round() as i64
}

#[inline]
pub fn log(x: f64) -> f64 {
    black_box(x).ln()
}

#[inline]
pub fn exp(x: f64) -> f64 {
    black_box(x).exp()
}

#[inline]
pub fn pow(b: f64, e: f64) -> f64 {
    black_box(b).powf(black_box(e))
}

#[inline]
pub fn sqrt(x: f64) -> f64 {
    x.sqrt()
}

#[inline]
pub fn sin(x: f64) -> f64 {
    black_box(x).sin()
}

#[inline]
pub fn cos(x: f64) -> f64 {
    black_box(x).cos()
}

/// `PackedFloat32Array.bsearch(v, before)`: lower bound (`before`) or upper
/// bound of `v` rounded to f32.
pub fn bsearch(a: &[f32], v: f64, before: bool) -> i64 {
    let v = v as f32;
    if before {
        a.partition_point(|&x| x < v) as i64
    } else {
        a.partition_point(|&x| !(v < x)) as i64
    }
}

/// Sorts f32 values ascending as `PackedFloat32Array.sort()` (no NaN here).
pub fn sort_f32(v: &mut [f32]) {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
}

/// `SongMap._pct`: the q-quantile (index `int(q * (n - 1))`) of a sorted copy;
/// 0 when empty.
pub fn pct(v: &[f32], q: f64) -> f64 {
    if v.is_empty() {
        return 0.0;
    }
    let mut s = v.to_vec();
    sort_f32(&mut s);
    let n = s.len() as i64;
    s[clampi(int(q * (n - 1) as f64), 0, n - 1) as usize] as f64
}

/// Godot `is_equal_approx(double, double)`.
pub fn is_equal_approx(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let mut tol = 0.00001 * a.abs();
    if tol < 0.00001 {
        tol = 0.00001;
    }
    (a - b).abs() < tol
}

/// Slice index helper: `a[i]` with an i64 index.
#[inline]
pub fn at(a: &[f32], i: i64) -> f64 {
    a[i as usize] as f64
}
