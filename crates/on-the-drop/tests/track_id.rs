//! Track identity (src/track_id.rs): the same recording matches across
//! decoders, sample rates and lead-in; different songs do not.

mod common;

use common::synth::{self, Opt};
use on_the_drop::track_id::{self, TrackRecord, matches};
use std::sync::OnceLock;

fn song() -> &'static Vec<f32> {
    static S: OnceLock<Vec<f32>> = OnceLock::new();
    S.get_or_init(|| synth::make(&[0, 1, 2, 3], Opt::default()).pcm_44k())
}

/// Resamples by 4-point cubic (Catmull-Rom) interpolation.
fn resample(x: &[f32], from: f64, to: f64) -> Vec<f32> {
    let n = (x.len() as f64 * to / from) as usize;
    let at = |i: i64| x[i.clamp(0, x.len() as i64 - 1) as usize] as f64;
    (0..n)
        .map(|o| {
            let p = o as f64 * from / to;
            let i = p.floor() as i64;
            let t = p - i as f64;
            let (y0, y1, y2, y3) = (at(i - 1), at(i), at(i + 1), at(i + 2));
            let v = y1
                + 0.5 * t * (y2 - y0 + t * (2.0 * y0 - 5.0 * y1 + 4.0 * y2 - y3 + t * (3.0 * (y1 - y2) + y3 - y0)));
            v as f32
        })
        .collect()
}

fn dur(x: &[f32], rate: f64) -> f64 {
    x.len() as f64 / rate
}

#[test]
fn same_song_at_48k_matches() {
    let a = song();
    let b = resample(a, 44100.0, 48000.0);
    let fa = track_id::fingerprint(a, 44100, 1);
    let fb = track_id::fingerprint(&b, 48000, 1);
    let r = matches(&fa, dur(a, 44100.0), &fb, dur(&b, 48000.0));
    // Why: a browser decodes at its AudioContext rate (often 48 kHz) and the
    // Frame at 44.1 kHz; the same recording must keep one identity, or the
    // user's labels from one device are lost on the other.
    assert!(r.matched && r.offset.abs() < 0.2, "44.1 vs 48 kHz: {r:?}");
}

#[test]
fn quantized_and_noisy_copy_matches() {
    let a = song();
    let mut rng = synth::Pcg::new(7);
    // 16-bit quantised, plus white noise at about -60 dB full scale
    let b: Vec<f32> = a
        .iter()
        .map(|&x| ((x as f64 * 32767.0).round() / 32767.0 + 0.001 * rng.range(-1.0, 1.0)) as f32)
        .collect();
    let r = matches(&track_id::fingerprint(a, 44100, 1), dur(a, 44100.0), &track_id::fingerprint(&b, 44100, 1), dur(&b, 44100.0));
    // Why: two decoders differ in the low bits (Godot's MP3 decoder vs a
    // browser's decodeAudioData); low-level differences must not change the
    // identity.
    assert!(r.matched, "16-bit + -60 dB noise: {r:?}");
}

#[test]
fn leading_silence_matches_with_offset() {
    let a = song();
    let mut b = vec![0.0f32; 8820]; // 200 ms at 44.1 kHz
    b.extend_from_slice(a);
    let r = matches(&track_id::fingerprint(&b, 44100, 1), dur(&b, 44100.0), &track_id::fingerprint(a, 44100, 1), dur(a, 44100.0));
    // Why: copies of one recording differ in padding (an encoder's priming
    // samples, a rip with lead-in). They must match, and the offset must say
    // how far, so labels can later be shifted between the copies.
    assert!(r.matched, "200 ms lead-in: {r:?}");
    assert!((r.offset - 0.2).abs() <= track_id::item_seconds() + 1e-6, "offset {:.3} s, want 0.2 s", r.offset);
}

#[test]
fn different_songs_do_not_match() {
    // The same sounds in other forms and tempos: only the order and the
    // timing differ, the hardest case for a fingerprint.
    let others = [
        synth::make(&[0, 1, 2, 3], Opt { form: synth::DROPS_FORM, ..Default::default() }),
        synth::make(&[0, 2], Opt { bpm: 117.3, snare: Some(vec![2]), hats8: true, ..Default::default() }),
        synth::make(&[0, 1, 2, 3], Opt { form: synth::LOUD_BUILD_FORM, ..Default::default() }),
    ];
    let a = song();
    let fa = track_id::fingerprint(a, 44100, 1);
    for o in &others {
        let b = o.pcm_44k();
        let r = matches(&fa, dur(a, 44100.0), &track_id::fingerprint(&b, 44100, 1), dur(&b, 44100.0));
        // Why: a false match moves one song's labels onto another song.
        assert!(!r.matched, "different songs must not match: {r:?}");
        // ... also when the durations happen to agree (the duration check
        // alone must not be what keeps them apart).
        let r2 = matches(&fa, 100.0, &track_id::fingerprint(&b, 44100, 1), 100.0);
        assert!(!r2.matched, "different songs, equal durations: {r2:?}");
    }
}

#[test]
fn record_round_trip_and_tag_fallback() {
    let a = song();
    let rec = TrackRecord::from_pcm(&a[..44100 * 30], 44100, 1);
    let js = serde_json::to_string(&rec).unwrap();
    let back: TrackRecord = serde_json::from_str(&js).unwrap();
    // Why: the record is stored in the bundle; it must read back to the same
    // fingerprint and still match itself.
    assert_eq!(back, rec);
    assert!(track_id::same_track(&rec, &back));
    assert_eq!(back.fingerprint_version, track_id::FINGERPRINT_VERSION);
    // Why: a record of another fingerprint version cannot be compared bit
    // by bit; the tags and the duration decide then.
    let mut old = rec.clone();
    old.fingerprint_version = "old".into();
    assert!(!track_id::same_track(&rec, &old), "no tags: no match across versions");
    let mut x = rec.clone();
    x.artist = Some("Daft Punk".into());
    x.title = Some("One More Time".into());
    let mut y = old.clone();
    y.artist = Some("daft  punk".into());
    y.title = Some("One more time!".into());
    y.duration += 2.5;
    assert!(track_id::same_track(&x, &y), "normalised tags and a duration within 3 s match");
    y.duration += 1.0;
    assert!(!track_id::same_track(&x, &y), "a duration 3.5 s off does not");
}

/// FNV-1a 64 of the fingerprint items (little-endian).
fn fp_hash(fp: &[u32]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in fp.iter().flat_map(|x| x.to_le_bytes()) {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// The golden input, defined here so that nothing else can change it: 30 s
/// at 44.1 kHz, mono, a new three-note chord every 1.5 s (its pitches from
/// a fixed table) with a decaying noise click at each change.
fn golden_input() -> Vec<f32> {
    const NOTES: [f64; 8] = [220.0, 246.94, 277.18, 293.66, 329.63, 369.99, 415.30, 440.0];
    let mut rng = synth::Pcg::new(5);
    (0..44100 * 30)
        .map(|i| {
            let t = i as f64 / 44100.0;
            let c = (t / 1.5) as usize;
            let tc = t - c as f64 * 1.5;
            let mut v = 0.0;
            for k in 0..3 {
                let f = NOTES[(c * 3 + k * 2) % 8] * (1 + k) as f64 * 0.5;
                v += 0.2 * (std::f64::consts::TAU * f * t).sin();
            }
            v += 0.3 * (-tc / 0.02).exp() * rng.range(-1.0, 1.0);
            v as f32
        })
        .collect()
}

#[test]
fn golden_fingerprint() {
    let fp = track_id::fingerprint(&golden_input(), 44100, 1);
    // Why: saved labels are keyed by this fingerprint. Any change to it (the
    // front end, the preset, a rusty-chromaprint upgrade) orphans every
    // saved label. If this fails, the change must come with a new
    // FINGERPRINT_VERSION and a migration that keeps the old fingerprint.
    assert_eq!(track_id::FINGERPRINT_VERSION, "chromaprint-rs-0.3.0/test2/s16/1");
    let got = format!("{:016x}", fp_hash(&fp));
    assert_eq!((fp.len(), got.as_str()), GOLDEN, "the fingerprint changed");
}

const GOLDEN: (usize, &str) = (221, "512e0d437f327929");
