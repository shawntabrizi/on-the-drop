//! Track identity: which recording is this, on any device?
//!
//! The same recording gives different PCM on each client (Godot's MP3
//! decoder and a browser's decodeAudioData differ in the low bits; a
//! browser resamples to its AudioContext rate; the Frame caches WAVs made
//! from the Jellyfin stream). So the ID is an audio fingerprint, not a hash
//! of the samples: Chromaprint (AcoustID) through the `rusty-chromaprint`
//! crate (MIT, pure Rust), with one fixed configuration ([`PRESET`]).
//!
//! FROZEN. The fingerprint has its own front end (f32 PCM -> 16-bit ->
//! Chromaprint's own downmix, resampler to 11025 Hz and chroma features) and
//! shares no code with the analysis bins, which may be tuned later. User
//! labels are keyed by track; a changed fingerprint would orphan them.
//! **Changing this breaks all saved IDs. Bump the version only with a
//! migration that keeps the old fingerprint next to the new one.** The
//! version names the crate version and the preset ([`FINGERPRINT_VERSION`]);
//! the crate is pinned exactly (`=0.3.0`) and a golden test guards both.
//!
//! Matching ([`matches`]): Chromaprint's own matcher (best alignment, then
//! segments with a mean bit error per 32-bit item); a match needs the
//! durations within [`DURATION_TOL`] s and matched segments with a score
//! under [`SCORE_MAX`] covering at least [`COVER_MIN`] of the shorter
//! fingerprint. Without a fingerprint of the same version: normalised
//! artist + title + duration ([`matches_tags`]).

use rusty_chromaprint::{Configuration, Fingerprinter, match_fingerprints};
use serde::{Deserialize, Serialize};

/// The Chromaprint configuration: `preset_test2`, Chromaprint's default
/// algorithm (ALGORITHM_TEST2, the one AcoustID uses).
pub const PRESET: &str = "test2";
/// Crate version, preset and front end; part of every stored record.
pub const FINGERPRINT_VERSION: &str = "chromaprint-rs-0.3.0/test2/s16/1";
/// s: two copies of one recording may differ this much in length (padding, edits).
pub const DURATION_TOL: f64 = 3.0;
/// Matched segments must cover this share of the shorter fingerprint.
/// Calibration (examples/track_id_check.rs, 31 songs, each against an MP3
/// re-encode at 44.1 and at 48 kHz): same recording 1.000 in all 62 pairs;
/// different songs at most 0.034 (930 pairs). The synthetic test songs that
/// reuse sample-identical loops in another order reach 0.67, so 0.8.
pub const COVER_MIN: f64 = 0.8;
/// A segment counts when its mean bit error per 32-bit item is under this.
/// Same recording: 0.04..0.16 (bit error rate 0.1..0.5 %); Chromaprint
/// itself keeps only segments under 10.
pub const SCORE_MAX: f64 = 8.0;

fn config() -> Configuration {
    Configuration::preset_test2()
}

/// Seconds per fingerprint item.
pub fn item_seconds() -> f64 {
    config().item_duration_in_seconds() as f64
}

/// The 16-bit sample the front end feeds Chromaprint.
fn s16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// Chromaprint fingerprint of PCM: `channels` interleaved channels at
/// `rate` Hz. Empty when the rate is not usable or the audio is too short.
pub fn fingerprint(pcm: &[f32], rate: u32, channels: u32) -> Vec<u32> {
    if rate == 0 || channels == 0 {
        return Vec::new();
    }
    let mut p = Fingerprinter::new(&config());
    if p.start(rate, channels).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::with_capacity(65536);
    for chunk in pcm.chunks(65536 - 65536 % channels as usize) {
        buf.clear();
        buf.extend(chunk.iter().map(|&x| s16(x)));
        p.consume(&buf);
    }
    p.finish();
    p.fingerprint().to_vec()
}

/// The stored identity of a track.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TrackRecord {
    /// [`FINGERPRINT_VERSION`] of `fingerprint`.
    pub fingerprint_version: String,
    /// The fingerprint: base64 (standard, padded) of the u32 items,
    /// little-endian. (rusty-chromaprint 0.3.0 can compress to the AcoustID
    /// form but not decompress it, and matching needs the items.)
    pub fingerprint: String,
    /// s, of the analysed audio.
    pub duration: f64,
    /// Hz of the PCM the fingerprint was made from.
    pub sample_rate: u32,
    /// Fast lookup hint: [`exact_key`]. The match decides, not the key.
    pub key: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artist: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
}

/// FNV-1a 64 (a stable, dependency-free hash for the lookup key).
fn fnv1a(bytes: impl Iterator<Item = u8>) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Lookup key: a hash of the fingerprint items (top 16 bits of each: the
/// coarse part, so tiny differences can still share a key) and the
/// duration rounded to whole seconds. Only a hint.
pub fn exact_key(fp: &[u32], duration: f64) -> String {
    let d = duration.round() as i64;
    let h = fnv1a(fp.iter().flat_map(|&x| ((x >> 16) as u16).to_le_bytes()).chain(d.to_le_bytes()));
    format!("{h:016x}")
}

impl TrackRecord {
    /// A record from PCM (`channels` interleaved at `rate` Hz).
    pub fn from_pcm(pcm: &[f32], rate: u32, channels: u32) -> Self {
        let fp = fingerprint(pcm, rate, channels);
        let duration = if rate > 0 && channels > 0 { pcm.len() as f64 / channels as f64 / rate as f64 } else { 0.0 };
        Self::new(&fp, duration, rate)
    }

    pub fn new(fp: &[u32], duration: f64, sample_rate: u32) -> Self {
        TrackRecord {
            fingerprint_version: FINGERPRINT_VERSION.to_string(),
            fingerprint: encode(fp),
            duration,
            sample_rate,
            key: exact_key(fp, duration),
            artist: None,
            title: None,
            album: None,
        }
    }

    /// The fingerprint items (empty when missing or not decodable).
    pub fn items(&self) -> Vec<u32> {
        decode(&self.fingerprint).unwrap_or_default()
    }
}

pub fn encode(fp: &[u32]) -> String {
    use base64::Engine;
    let bytes: Vec<u8> = fp.iter().flat_map(|x| x.to_le_bytes()).collect();
    base64::engine::general_purpose::STANDARD.encode(bytes)
}

pub fn decode(s: &str) -> Option<Vec<u32>> {
    use base64::Engine;
    let b = base64::engine::general_purpose::STANDARD.decode(s).ok()?;
    if b.len() % 4 != 0 {
        return None;
    }
    Some(b.chunks_exact(4).map(|c| u32::from_le_bytes(c.try_into().unwrap())).collect())
}

/// How two fingerprints compare.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MatchResult {
    pub matched: bool,
    /// Share of the shorter fingerprint covered by segments under SCORE_MAX.
    pub cover: f64,
    /// Mean bit error per 32-bit item over those segments (0..32; 32 = none).
    pub score: f64,
    /// s: where `b` starts in `a` (a's time = b's time + offset; positive when
    /// `a` has more lead-in). From the longest matched segment.
    pub offset: f64,
}

/// Compares two fingerprints of the same version (durations in s).
pub fn matches(a: &[u32], da: f64, b: &[u32], db: f64) -> MatchResult {
    let none = MatchResult { matched: false, cover: 0.0, score: 32.0, offset: 0.0 };
    if a.is_empty() || b.is_empty() {
        return none;
    }
    let Ok(segs) = match_fingerprints(a, b, &config()) else {
        return none;
    };
    let mut items = 0usize;
    let mut bits = 0.0f64;
    let mut longest: Option<&rusty_chromaprint::Segment> = None;
    for s in segs.iter().filter(|s| s.score < SCORE_MAX) {
        items += s.items_count;
        bits += s.score * s.items_count as f64;
        if longest.is_none_or(|l| s.items_count > l.items_count) {
            longest = Some(s);
        }
    }
    let Some(l) = longest else {
        return none;
    };
    let cover = items as f64 / a.len().min(b.len()) as f64;
    let score = bits / items as f64;
    let offset = (l.offset1 as f64 - l.offset2 as f64) * item_seconds();
    let matched = (da - db).abs() <= DURATION_TOL && cover >= COVER_MIN;
    MatchResult { matched, cover, score, offset }
}

/// Lower case, letters and digits only, single spaces (for tag matching).
pub fn normalize(s: &str) -> String {
    let t: String = s.to_lowercase().chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect();
    t.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Fallback without a usable fingerprint: normalised artist and title
/// equal, durations within DURATION_TOL.
pub fn matches_tags(a: &TrackRecord, b: &TrackRecord) -> bool {
    let eq = |x: &Option<String>, y: &Option<String>| match (x, y) {
        (Some(x), Some(y)) => {
            let (x, y) = (normalize(x), normalize(y));
            !x.is_empty() && x == y
        }
        _ => false,
    };
    eq(&a.artist, &b.artist) && eq(&a.title, &b.title) && (a.duration - b.duration).abs() <= DURATION_TOL
}

/// Same recording? Fingerprints when both have one of the current version,
/// else the tag fallback.
pub fn same_track(a: &TrackRecord, b: &TrackRecord) -> bool {
    if a.fingerprint_version == FINGERPRINT_VERSION && b.fingerprint_version == FINGERPRINT_VERSION {
        let (fa, fb) = (a.items(), b.items());
        if !fa.is_empty() && !fb.is_empty() {
            return matches(&fa, a.duration, &fb, b.duration).matched;
        }
    }
    matches_tags(a, b)
}
