//! Real-file check of the track fingerprint (not a unit test: the audio
//! is not in git).
//!
//!   track_id_check <orig.wav> <copy.wav>... -- <orig.wav> <copy.wav>... -- ...
//!
//! Each group: an original and copies of the same recording (e.g. an MP3
//! re-encode decoded by another decoder, a 48 kHz resample). Prints every
//! same-recording pair and every different-song pair (originals only), the
//! cover / score spread of both and whether the decision is right.
//! WAV: 16-bit PCM or 32-bit float, any rate and channel count.

use on_the_drop::track_id;

fn read_wav(path: &str) -> (Vec<f32>, u32, u32) {
    let b = std::fs::read(path).expect("read wav");
    assert!(&b[0..4] == b"RIFF" && &b[8..12] == b"WAVE", "{path}: not a WAV");
    let (mut fmt, mut rate, mut ch, mut bits) = (0u16, 0u32, 0u16, 0u16);
    let mut i = 12;
    while i + 8 <= b.len() {
        let id = &b[i..i + 4];
        let mut n = u32::from_le_bytes(b[i + 4..i + 8].try_into().unwrap()) as usize;
        let body = i + 8;
        if id == b"fmt " {
            fmt = u16::from_le_bytes([b[body], b[body + 1]]);
            ch = u16::from_le_bytes([b[body + 2], b[body + 3]]);
            rate = u32::from_le_bytes(b[body + 4..body + 8].try_into().unwrap());
            bits = u16::from_le_bytes([b[body + 14], b[body + 15]]);
            if fmt == 0xFFFE {
                fmt = u16::from_le_bytes([b[body + 24], b[body + 25]]);
            }
        } else if id == b"data" {
            n = n.min(b.len() - body);
            let d = &b[body..body + n];
            let pcm = match (fmt, bits) {
                (1, 16) => d.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0).collect(),
                (3, 32) => d.chunks_exact(4).map(|c| f32::from_le_bytes(c.try_into().unwrap())).collect(),
                _ => panic!("{path}: format {fmt} / {bits} bit not supported"),
            };
            return (pcm, rate, ch as u32);
        }
        i = body + n + (n & 1);
    }
    panic!("{path}: no data chunk");
}

struct Fp {
    name: String,
    fp: Vec<u32>,
    dur: f64,
}

fn fp(path: &str) -> Fp {
    let (pcm, rate, ch) = read_wav(path);
    let name = std::path::Path::new(path).file_stem().unwrap().to_string_lossy().into_owned();
    Fp { name, fp: track_id::fingerprint(&pcm, rate, ch), dur: pcm.len() as f64 / ch as f64 / rate as f64 }
}

fn spread(v: &mut [f64]) -> String {
    if v.is_empty() {
        return "-".into();
    }
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let q = |p: f64| v[((v.len() - 1) as f64 * p) as usize];
    format!("min {:.3} p10 {:.3} median {:.3} p90 {:.3} max {:.3} (n {})", q(0.0), q(0.1), q(0.5), q(0.9), q(1.0), v.len())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let groups: Vec<Vec<Fp>> = args.split(|a| a == "--").filter(|g| !g.is_empty()).map(|g| g.iter().map(|p| fp(p)).collect()).collect();
    let (mut same_c, mut same_s, mut diff_c, mut diff_s) = (vec![], vec![], vec![], vec![]);
    let mut wrong = 0;
    for g in &groups {
        for c in &g[1..] {
            let r = track_id::matches(&g[0].fp, g[0].dur, &c.fp, c.dur);
            println!("same {:<50} {:<28} match {} cover {:.3} score {:.2} offset {:+.3} s", g[0].name, c.name, r.matched, r.cover, r.score, r.offset);
            same_c.push(r.cover);
            same_s.push(r.score);
            wrong += !r.matched as usize;
        }
    }
    for i in 0..groups.len() {
        for j in 0..groups.len() {
            if i == j {
                continue;
            }
            let (a, b) = (&groups[i][0], &groups[j][0]);
            let r = track_id::matches(&a.fp, a.dur, &b.fp, b.dur);
            // the duration check off: the fingerprint alone must reject
            let r2 = track_id::matches(&a.fp, 0.0, &b.fp, 0.0);
            diff_c.push(r2.cover);
            diff_s.push(r2.score);
            if r.matched || r2.matched {
                wrong += 1;
                println!("FALSE MATCH {} vs {}: {r2:?}", a.name, b.name);
            }
        }
    }
    println!("same recording: cover {}", spread(&mut same_c));
    println!("same recording: score {}", spread(&mut same_s));
    println!("different songs (no duration check): cover {}", spread(&mut diff_c));
    println!("different songs (no duration check): score {}", spread(&mut diff_s));
    println!("thresholds: cover >= {}, segment score < {}, duration within {} s", track_id::COVER_MIN, track_id::SCORE_MAX, track_id::DURATION_TOL);
    println!("wrong decisions: {wrong}");
    std::process::exit(if wrong > 0 { 1 } else { 0 });
}
