//! Godot 4.7 GDExtension for on-the-drop. Thin wrapper: all logic lives in
//! the `on-the-drop` crate. Decoding stays on the Godot side.

use godot::prelude::*;
use on_the_drop::format::MapData;
use on_the_drop::{Analysis, track_id};

struct OnTheDropExtension;

#[gdextension]
unsafe impl ExtensionLibrary for OnTheDropExtension {}

/// Song analysis entry point. All functions are static, e.g.
/// `OnTheDrop.analyze(frames, 44100.0)`.
#[derive(GodotClass)]
#[class(init, base = RefCounted)]
pub struct OnTheDrop {
    base: Base<RefCounted>,
}

fn f32s(v: &[f32]) -> Variant {
    PackedFloat32Array::from(v).to_variant()
}

fn bytes(v: &[u8]) -> Variant {
    PackedByteArray::from(v).to_variant()
}

fn text(s: &str) -> Variant {
    GString::from(s).to_variant()
}

/// The map in frame-visualizer's `SongMap.to_dict` shape (packed arrays,
/// finale as a Vector2), plus the overview: "levels" (3 bytes per bin),
/// "bins", "bin_s" and "analysis" (the analysis version); the diagnostics
/// "part_bounds", "fixed_tempo", "grid_drift"; and the bins "bin_raw",
/// "bin_air" (the analyze_bins input).
fn analysis_dict(a: &Analysis) -> VarDictionary {
    let m = MapData::from(&a.map);
    let mut d = VarDictionary::new();
    d.set("duration", &m.duration.to_variant());
    d.set("bpm", &m.bpm.to_variant());
    d.set("beats", &f32s(&m.beats));
    d.set("beat_bar", &bytes(&m.beat_bar));
    d.set("beat_support", &bytes(&m.beat_support));
    d.set("phrases", &f32s(&m.phrases));
    d.set("sections", &f32s(&m.sections));
    d.set("finale", &Vector2::new(m.finale[0], m.finale[1]).to_variant());
    d.set("energy", &bytes(&m.energy));
    d.set("kicks", &f32s(&m.kicks));
    d.set("snares", &f32s(&m.snares));
    d.set("hats", &f32s(&m.hats));
    d.set("kick_amp", &bytes(&m.kick_amp));
    d.set("snare_amp", &bytes(&m.snare_amp));
    d.set("hat_amp", &bytes(&m.hat_amp));
    d.set("character", &(m.character as i64).to_variant());
    d.set("kick_cover", &m.kick_cover.to_variant());
    d.set("four_floor", &m.four_floor.to_variant());
    d.set("drop_step", &m.drop_step.to_variant());
    d.set("drop_strength", &f32s(&m.drop_strength));
    d.set("levels", &bytes(&a.levels));
    d.set("bins", &(a.bins.nb as i64).to_variant());
    d.set("bin_s", &a.bins.bin_s.to_variant());
    d.set("analysis", &(on_the_drop::ANALYSIS_VERSION as i64).to_variant());
    // Analysis-only diagnostics (not in the map file): reports and tests.
    d.set("part_bounds", &f32s(&a.map.part_bounds));
    d.set("fixed_tempo", &a.map.fixed_tempo.to_variant());
    d.set("grid_drift", &a.map.grid_drift.to_variant());
    // The band bins themselves (3 per bin, low / mid / high; and the air
    // band, 1 per bin), for tools that cache them and re-run analyze_bins.
    d.set("bin_raw", &f32s(&a.bins.raw));
    d.set("bin_air", &f32s(&a.bins.air));
    d
}

fn interleave(frames: &PackedVector2Array) -> Vec<f32> {
    frames.as_slice().iter().flat_map(|v| [v.x, v.y]).collect()
}

fn rate_ok(rate: f64) -> bool {
    rate > 0.0 && rate <= 1_000_000.0
}

fn result(a: Option<Analysis>) -> VarDictionary {
    a.map_or_else(VarDictionary::new, |a| analysis_dict(&a))
}

#[godot_api]
impl OnTheDrop {
    /// Version of the Rust core library.
    #[func]
    fn version() -> GString {
        GString::from(on_the_drop::version())
    }

    /// Analysis version (the frame-visualizer `SongOverview.VERSION` matched).
    #[func]
    fn analysis_version() -> i64 {
        on_the_drop::ANALYSIS_VERSION as i64
    }

    /// Analyses decoded stereo frames (one Vector2 per frame, as
    /// `AudioStreamPlaybackResampled` / `AudioEffectCapture` give them) at
    /// `rate` Hz. Empty when there is not one full bin or the rate is bad.
    #[func]
    fn analyze(frames: PackedVector2Array, rate: f64) -> VarDictionary {
        if !rate_ok(rate) {
            return VarDictionary::new();
        }
        result(on_the_drop::analyze_stereo(&interleave(&frames), rate))
    }

    /// As `analyze`, for mono samples.
    #[func]
    fn analyze_mono(samples: PackedFloat32Array, rate: f64) -> VarDictionary {
        if !rate_ok(rate) {
            return VarDictionary::new();
        }
        result(on_the_drop::analyze_mono(samples.as_slice(), rate))
    }

    /// Analyses band bins (`raw`: 3 per bin, low / mid / high; `air`: 1 per
    /// bin; `nb` bins of `bin_s` s each). Empty when they do not fit.
    #[func]
    fn analyze_bins(raw: PackedFloat32Array, nb: i64, bin_s: f64, air: PackedFloat32Array) -> VarDictionary {
        if nb <= 0 || raw.len() < nb as usize * 3 || air.len() < nb as usize {
            return VarDictionary::new();
        }
        let bins = on_the_drop::Bins {
            raw: raw.as_slice().to_vec(),
            air: air.as_slice().to_vec(),
            nb: nb as usize,
            bin_s,
        };
        result(on_the_drop::analyze_bins(bins))
    }

    /// The raw section changes of band bins on a beat grid (the first
    /// stage of the map, before the drop pass and the part snapping):
    /// `[t0, id0, t1, id1, ...]` (frame-visualizer `SongSections.analyse`).
    #[func]
    fn sections(raw: PackedFloat32Array, nb: i64, bin_s: f64, beats: PackedFloat32Array) -> PackedFloat32Array {
        if nb < 0 || raw.len() < nb as usize * 3 || !(bin_s > 0.0) {
            return PackedFloat32Array::new();
        }
        let s = on_the_drop::sections::analyse(raw.as_slice(), nb as usize, bin_s, beats.as_slice());
        PackedFloat32Array::from(s.as_slice())
    }

    /// The track record of decoded stereo frames at `rate` Hz:
    /// "fingerprint_version", "fingerprint" (base64), "duration",
    /// "sample_rate", "key". Empty for a bad rate.
    #[func]
    fn fingerprint(frames: PackedVector2Array, rate: i64) -> VarDictionary {
        if !(1..=1_000_000).contains(&rate) {
            return VarDictionary::new();
        }
        let r = track_id::TrackRecord::from_pcm(&interleave(&frames), rate as u32, 2);
        let mut d = VarDictionary::new();
        d.set("fingerprint_version", &text(&r.fingerprint_version));
        d.set("fingerprint", &text(&r.fingerprint));
        d.set("duration", &r.duration.to_variant());
        d.set("sample_rate", &(r.sample_rate as i64).to_variant());
        d.set("key", &text(&r.key));
        d
    }

    /// Compares two fingerprints (base64, the same fingerprint_version) with
    /// their durations in s: "matched", "cover", "score" and "offset" (s: how
    /// much later the music starts in `a` than in `b`).
    #[func]
    fn matches(a: GString, duration_a: f64, b: GString, duration_b: f64) -> VarDictionary {
        let fa = track_id::decode(&a.to_string()).unwrap_or_default();
        let fb = track_id::decode(&b.to_string()).unwrap_or_default();
        let r = track_id::matches(&fa, duration_a, &fb, duration_b);
        let mut d = VarDictionary::new();
        d.set("matched", &r.matched.to_variant());
        d.set("cover", &r.cover.to_variant());
        d.set("score", &r.score.to_variant());
        d.set("offset", &r.offset.to_variant());
        d
    }
}
