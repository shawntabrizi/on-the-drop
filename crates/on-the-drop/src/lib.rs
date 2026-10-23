//! on-the-drop: beatgrid, phrase and drop analysis.
//!
//! A port of frame-visualizer's whole-song analysis (GDScript, master
//! 2ae8592: `lib/song_overview.gd`, `lib/song_map.gd`, `lib/song_sections.gd`,
//! `lib/song_novelty.gd`, `lib/section_overrides.gd`) with the same results
//! bit for bit on the same input (see `parity/` and the README).
//!
//! Pipeline: decoded PCM (the caller decodes) -> band bins ([`overview`]) ->
//! the song map ([`SongMap`]). [`analyze_stereo`] / [`analyze_mono`] run
//! both; [`SongMap::analyse`] starts from bins.

// The port keeps GDScript's index loops and its NaN-exact comparisons
// (`!(a < b)`), so it reads line by line against the source.
#![allow(clippy::needless_range_loop, clippy::neg_cmp_op_on_partial_ord)]

pub mod format;
mod gd;
pub mod map;
pub mod novelty;
pub mod overrides;
pub mod overview;
pub mod sections;
pub mod track_id;

pub use map::SongMap;
pub use overview::Bins;

/// Analysis version: frame-visualizer `SongOverview.VERSION` this port
/// matches. A change that alters any result bumps it.
pub const ANALYSIS_VERSION: u32 = 11;

/// Crate version, so the Godot side can check which library it loaded.
pub fn version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// One analysed track: the bins, the overview strip and the map.
#[derive(Clone, Debug, PartialEq)]
pub struct Analysis {
    pub bins: Bins,
    /// 3 bytes per bin: low, mid, high (0..255), the overview waveform.
    pub levels: Vec<u8>,
    pub map: SongMap,
}

/// Analyses interleaved stereo frames (L, R, ...) at `rate` Hz. None when
/// there is not one full bin of audio.
pub fn analyze_stereo(interleaved: &[f32], rate: f64) -> Option<Analysis> {
    analyze_bins(overview::bins_stereo(interleaved, rate))
}

/// Analyses mono samples at `rate` Hz. None when there is not one full bin.
pub fn analyze_mono(samples: &[f32], rate: f64) -> Option<Analysis> {
    analyze_bins(overview::bins_mono(samples, rate))
}

/// Analyses band bins (from [`overview`] or another source).
pub fn analyze_bins(bins: Bins) -> Option<Analysis> {
    if bins.nb == 0 || !(bins.bin_s > 0.0) || bins.raw.len() < bins.nb * 3 {
        return None;
    }
    let map = SongMap::analyse(&bins.raw, bins.nb, bins.bin_s, &bins.air);
    let levels = overview::levels(&bins);
    Some(Analysis { bins, levels, map })
}
