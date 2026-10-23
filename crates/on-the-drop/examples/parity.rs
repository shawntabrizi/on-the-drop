//! Parity harness, Rust side (see `parity/README.md`).
//!
//!   parity bins <in.bins> <out.json>              Stage A: map from the GDScript bins
//!   parity pcm  <in.pcm> <out.json> <out.bins>    Stage B: bins and map from the PCM
//!
//! File formats are written by `parity/fv_dump.gd`. The JSON has the same
//! fields as the GDScript dump, so `parity/compare.py` can diff them.

use on_the_drop::{Analysis, Bins, SongMap, overview};
use serde_json::{Value, json};
use std::time::Instant;

fn read_bins(path: &str) -> Bins {
    let b = std::fs::read(path).expect("read bins");
    assert_eq!(&b[0..8], b"OTDBIN1\0", "not a bins file");
    let nb = i64::from_le_bytes(b[8..16].try_into().unwrap()) as usize;
    let bin_s = f64::from_le_bytes(b[16..24].try_into().unwrap());
    let na = i64::from_le_bytes(b[24..32].try_into().unwrap()) as usize;
    let f = |o: usize, n: usize| -> Vec<f32> {
        (0..n).map(|i| f32::from_le_bytes(b[o + i * 4..o + i * 4 + 4].try_into().unwrap())).collect()
    };
    let raw = f(32, nb * 3);
    let air = f(32 + nb * 12, na);
    Bins { raw, air, nb, bin_s }
}

fn write_bins(bins: &Bins, path: &str) {
    let mut b = Vec::new();
    b.extend_from_slice(b"OTDBIN1\0");
    b.extend_from_slice(&(bins.nb as i64).to_le_bytes());
    b.extend_from_slice(&bins.bin_s.to_le_bytes());
    b.extend_from_slice(&(bins.air.len() as i64).to_le_bytes());
    for v in bins.raw.iter().chain(&bins.air) {
        b.extend_from_slice(&v.to_le_bytes());
    }
    std::fs::write(path, b).expect("write bins");
}

fn read_pcm(path: &str) -> (f64, Vec<f32>) {
    let b = std::fs::read(path).expect("read pcm");
    assert_eq!(&b[0..8], b"OTDPCM1\0", "not a pcm file");
    let rate = f64::from_le_bytes(b[8..16].try_into().unwrap());
    let frames = i64::from_le_bytes(b[16..24].try_into().unwrap()) as usize;
    let pcm = (0..frames * 2).map(|i| f32::from_le_bytes(b[24 + i * 4..28 + i * 4].try_into().unwrap())).collect();
    (rate, pcm)
}

fn f32s(v: &[f32]) -> Value {
    Value::Array(v.iter().map(|&x| json!(x as f64)).collect())
}

fn map_json(m: &SongMap) -> serde_json::Map<String, Value> {
    let v = json!({
        "duration": m.duration, "bpm": m.bpm, "beats": f32s(&m.beats), "beat_bar": m.beat_bar,
        "beat_support": m.beat_support, "phrases": f32s(&m.phrases), "sections": f32s(&m.sections),
        "finale": [m.finale_start, m.finale_end], "energy": m.energy,
        "kicks": f32s(&m.kicks), "snares": f32s(&m.snares), "hats": f32s(&m.hats),
        "kick_amp": m.kick_amp, "snare_amp": m.snare_amp, "hat_amp": m.hat_amp,
        "character": m.character, "kick_cover": m.kick_cover, "four_floor": m.four_floor,
        "drop_step": m.drop_step, "drop_strength": f32s(&m.drop_strength),
        "downbeats": f32s(&m.downbeats), "drops": f32s(&m.drops), "part_bounds": f32s(&m.part_bounds),
        "words": if m.sections.is_empty() { vec![] } else { m.section_words(&[]) },
        "fixed_tempo": m.fixed_tempo, "grid_drift": m.grid_drift,
    });
    match v {
        Value::Object(o) => o,
        _ => unreachable!(),
    }
}

fn main() {
    let a: Vec<String> = std::env::args().collect();
    match a.get(1).map(String::as_str) {
        Some("bins") => {
            let bins = read_bins(&a[2]);
            let t = Instant::now();
            let m = SongMap::analyse(&bins.raw, bins.nb, bins.bin_s, &bins.air);
            let ms = t.elapsed().as_secs_f64() * 1000.0;
            let mut o = map_json(&m);
            o.insert("ms_analyse".into(), json!(ms));
            std::fs::write(&a[3], Value::Object(o).to_string()).expect("write json");
        }
        Some("pcm") => {
            let (rate, pcm) = read_pcm(&a[2]);
            let t = Instant::now();
            let bins = overview::bins_stereo(&pcm, rate);
            let ms_bins = t.elapsed().as_secs_f64() * 1000.0;
            let t = Instant::now();
            let an: Analysis = on_the_drop::analyze_bins(bins).expect("no audio");
            let ms_map = t.elapsed().as_secs_f64() * 1000.0;
            let mut o = map_json(&an.map);
            o.insert("levels".into(), json!(an.levels));
            o.insert("bins".into(), json!(an.bins.nb));
            o.insert("ms_bins".into(), json!(ms_bins));
            o.insert("ms_analyse".into(), json!(ms_map));
            std::fs::write(&a[3], Value::Object(o).to_string()).expect("write json");
            write_bins(&an.bins, &a[4]);
        }
        _ => {
            eprintln!("usage: parity bins <in.bins> <out.json> | parity pcm <in.pcm> <out.json> <out.bins>");
            std::process::exit(2);
        }
    }
}
