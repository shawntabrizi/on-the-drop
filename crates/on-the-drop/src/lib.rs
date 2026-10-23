//! on-the-drop: beatgrid, phrase and drop analysis.
//!
//! Spike status: only the toolchain is proven. The analysis is not ported yet.

/// Crate version, so the Godot side can check which library it loaded.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Upper edge of the bass band, in Hz.
pub const BASS_HZ: f32 = 150.0;

/// Bass-band energy per hop.
///
/// Runs a one-pole low-pass filter at [`BASS_HZ`] over mono `samples` at `rate` Hz,
/// then returns the mean square of each full block of `hop` samples. A trailing
/// partial block is dropped. Returns an empty vector if `rate` or `hop` is 0.
pub fn band_energy(samples: &[f32], rate: u32, hop: usize) -> Vec<f32> {
    if rate == 0 || hop == 0 {
        return Vec::new();
    }
    let dt = 1.0 / rate as f32;
    let rc = 1.0 / (2.0 * std::f32::consts::PI * BASS_HZ);
    let alpha = dt / (rc + dt);
    let mut y = 0.0f32;
    samples
        .chunks_exact(hop)
        .map(|block| {
            let mut sum = 0.0f32;
            for &x in block {
                y += alpha * (x - y);
                sum += y * y;
            }
            sum / hop as f32
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine(hz: f32, rate: u32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    // Why: the bass band drives the large, slow forms in the visualizer. A kick
    // (~50 Hz) must register strongly and a hi-hat (~5 kHz) at the same level
    // must not, or the large forms would react to cymbals.
    #[test]
    fn bass_passes_and_treble_is_rejected() {
        let rate = 48_000;
        let hop = 1024;
        let bass = band_energy(&sine(50.0, rate, rate as usize), rate, hop);
        let treble = band_energy(&sine(5_000.0, rate, rate as usize), rate, hop);
        // Skip the first block: the filter is still settling there.
        let b: f32 = bass[1..].iter().sum::<f32>() / (bass.len() - 1) as f32;
        let t: f32 = treble[1..].iter().sum::<f32>() / (treble.len() - 1) as f32;
        assert!(b > 0.3, "50 Hz sine should keep most of its energy (0.5), got {b}");
        assert!(t < b / 100.0, "5 kHz should be at least 20 dB below 50 Hz: {t} vs {b}");
    }

    // Why: callers map block index to time as index * hop / rate, so the
    // output must have exactly one value per full hop, and no value for a
    // partial block at the end.
    #[test]
    fn one_value_per_full_hop() {
        assert_eq!(band_energy(&[0.0; 2500], 48_000, 1000).len(), 2);
        assert!(band_energy(&[0.0; 999], 48_000, 1000).is_empty());
    }

    // Why: GDScript callers can pass 0 by mistake; that must not panic
    // (a panic inside the extension would take the whole app down).
    #[test]
    fn zero_rate_or_hop_is_empty() {
        assert!(band_energy(&[1.0; 100], 0, 10).is_empty());
        assert!(band_energy(&[1.0; 100], 48_000, 0).is_empty());
    }
}
