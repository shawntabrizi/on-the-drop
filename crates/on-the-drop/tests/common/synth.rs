//! A synthetic EDM track with known beats, hits and form, for the song map
//! tests (port of frame-visualizer `test/song_map_synth.gd`). 124 BPM, 4/4,
//! 48 bars (~93 s):
//!   intro      8 bars  pad + off-beat hats              calm
//!   drop 1     8 bars  kick on every beat, snare on 2 and 4, hats, bass
//!   breakdown  6 bars  pad + hats, no kick, no bass
//!   build      6 bars  snare roll (8ths, then 16ths) and a noise riser
//!   finale    12 bars  drop 1 + a lead, 3 dB louder: the song's peak
//!   outro      8 bars  quiet pad only (the cooldown)
//! The song starts LEAD s after the file start (silence). Sounds: kick =
//! 150 -> 50 Hz sweep with a click, snare = noise burst + 190 Hz body, hat =
//! differenced noise (> 5 kHz) of 25 ms, pad = 3 sines (mid band), bass =
//! 82 Hz. The other forms (LATE_FORM, REPEAT_FORM, DROPS_FORM,
//! LOUD_BUILD_FORM, ENTRY_FORM) as in the GDScript source.
//!
//! The noise is Godot's RandomNumberGenerator (PCG32, seed 11), so the
//! 22050 Hz samples are the GDScript synth's. [`Synth::pcm_44k`] upsamples
//! them to 44.1 kHz with the cubic interpolation of Godot's resampler (the
//! app decodes at 44.1 kHz); that step is close to Godot's, not bit equal.

#![allow(dead_code)]

use std::collections::HashMap;

pub const RATE: usize = 22050;
pub const BPM: f64 = 124.0;
pub const LEAD: f64 = 0.37;
pub type Form = &'static [(i64, &'static str)];
pub const FORM: Form = &[(8, "intro"), (8, "drop1"), (6, "breakdown"), (6, "build"), (12, "finale"), (8, "outro")];
pub const LATE_FORM: Form = &[(8, "intro"), (8, "verse"), (7, "chorus"), (1, "gap"), (12, "drop"), (8, "outro")];
pub const FILL_FORM: Form = &[(8, "intro"), (7, "drop1"), (1, "gap"), (8, "drop"), (8, "outro")];
pub const REPEAT_FORM: Form = &[(8, "intro"), (8, "chorus"), (6, "breakdown"), (8, "groove"), (8, "drop"), (8, "outro")];
pub const DROPS_FORM: Form =
    &[(8, "intro"), (8, "chorus"), (6, "dip"), (8, "groove"), (6, "breakdown"), (8, "verse"), (8, "drop"), (8, "outro")];
pub const LOUD_BUILD_FORM: Form = &[(8, "intro"), (8, "verse"), (16, "lbuild"), (16, "bigdrop"), (8, "outro")];
pub const ENTRY_FORM: Form = &[(8, "intro"), (8, "thin"), (16, "bigdrop"), (8, "outro")];
const KICK_PARTS: &[&str] = &["drop1", "finale", "verse", "chorus", "drop", "groove", "lbuild", "bigdrop", "thin"];
const SNARE_PARTS: &[&str] = &["drop1", "finale", "chorus", "drop", "groove", "bigdrop", "thin"];
const BASS_PARTS: &[&str] = &["drop1", "finale", "verse", "chorus", "drop", "groove", "dip", "lbuild", "bigdrop", "thin"];
const LEAD_PARTS: &[&str] = &["finale", "chorus", "gap", "drop", "bigdrop", "thin"];
const HAT_PARTS: &[&str] = &["intro", "drop1", "breakdown", "finale", "chorus", "gap", "drop", "lbuild", "bigdrop", "thin"];

fn kick_gain(name: &str) -> f64 {
    match name {
        "lbuild" | "bigdrop" | "thin" => 0.55,
        _ => 0.9,
    }
}

fn bass_gain(name: &str) -> f64 {
    match name {
        "lbuild" | "thin" => 0.08,
        "bigdrop" => 0.4,
        _ => 0.18,
    }
}

fn pad(name: &str) -> f64 {
    match name {
        "intro" => 0.12,
        "drop1" | "breakdown" => 0.08,
        "build" => 0.06,
        "finale" | "chorus" | "gap" | "drop" | "groove" | "lbuild" | "bigdrop" | "thin" => 0.1,
        "outro" | "verse" | "dip" => 0.03,
        _ => panic!("unknown part {name}"),
    }
}

/// Godot's RandomNumberGenerator: PCG32 (O'Neill, pcg-c-basic) with Godot's
/// default stream and its randf().
pub struct Pcg {
    state: u64,
    inc: u64,
}

impl Pcg {
    pub fn new(seed: u64) -> Self {
        const INC: u64 = 1442695040888963407; // PCG_DEFAULT_INC_64
        let mut p = Pcg { state: 0, inc: (INC << 1) | 1 };
        p.next();
        p.state = p.state.wrapping_add(seed);
        p.next();
        p
    }

    pub fn next(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(6364136223846793005).wrapping_add(self.inc | 1);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    pub fn randf(&mut self) -> f32 {
        let proto = self.next();
        if proto == 0 {
            return 0.0;
        }
        let sig = (self.next() | 0x80000001) as f32;
        sig * 2f32.powi(-32 - proto.leading_zeros() as i32)
    }

    /// `randf_range(from, to)` in f32 (RandomPCG::random(float, float)).
    pub fn range(&mut self, from: f32, to: f32) -> f64 {
        (self.randf() * (to - from) + from) as f64
    }
}

pub struct Opt {
    pub bpm: f64,
    pub drift: f64,
    pub snare: Option<Vec<i64>>,
    pub hats8: bool,
    pub form: Form,
}

impl Default for Opt {
    fn default() -> Self {
        Opt { bpm: BPM, drift: 0.0, snare: None, hats8: false, form: FORM }
    }
}

pub struct Synth {
    /// 16-bit samples at RATE (the GDScript AudioStreamWAV data).
    pub s16: Vec<i16>,
    pub beats: Vec<f32>,
    pub downbeats: Vec<f32>,
    pub kicks: Vec<f32>,
    pub snares: Vec<f32>,
    pub hats: Vec<f32>,
    /// Part name -> start (s).
    pub starts: HashMap<&'static str, f64>,
    pub dur: f64,
    pub bpm: f64,
}

fn add(x: &mut [f32], s: &[f32], t: f64, g: f64) {
    let i0 = (t * RATE as f64).round() as i64;
    for (i, &v) in s.iter().enumerate() {
        let j = i0 + i as i64;
        if (j as usize) < x.len() {
            x[j as usize] = (x[j as usize] as f64 + g * v as f64) as f32;
        }
    }
}

fn kick() -> Vec<f32> {
    let mut s = Vec::new();
    let mut ph = 0.0f64;
    for i in 0..(0.3 * RATE as f64) as usize {
        let t = i as f64 / RATE as f64;
        let f = 50.0 + 100.0 * (-t / 0.03).exp();
        ph += std::f64::consts::TAU * f / RATE as f64;
        let mut v = ph.sin() * (-t / 0.12).exp();
        if i < 40 {
            v += 0.3 * (1.0 - i as f64 / 40.0) * if i % 2 == 0 { 1.0 } else { -1.0 };
        }
        s.push(v as f32);
    }
    s
}

fn snare(rng: &mut Pcg) -> Vec<f32> {
    (0..(0.2 * RATE as f64) as usize)
        .map(|i| {
            let t = i as f64 / RATE as f64;
            let w = rng.range(-1.0, 1.0);
            (w * (-t / 0.05).exp() + 0.5 * (std::f64::consts::TAU * 190.0 * t).sin() * (-t / 0.06).exp()) as f32
        })
        .collect()
}

fn hat(rng: &mut Pcg) -> Vec<f32> {
    let (mut p1, mut p2) = (0.0f64, 0.0f64);
    (0..(0.06 * RATE as f64) as usize)
        .map(|i| {
            let t = i as f64 / RATE as f64;
            let w = rng.range(-1.0, 1.0);
            let d1 = w - p1;
            p1 = w;
            let d2 = d1 - p2;
            p2 = d1;
            (0.5 * d2 * (-t / 0.025).exp()) as f32
        })
        .collect()
}

/// kick_beats: the beats of the bar with a kick in the drops ([0, 1, 2, 3]
/// = four-on-the-floor; [0, 2] = kick on 1 and 3).
pub fn make(kick_beats: &[i64], opt: Opt) -> Synth {
    let mut rng = Pcg::new(11);
    let bpm = opt.bpm;
    let drift = opt.drift;
    let snare_beats: Vec<i64> = opt.snare.clone().unwrap_or_else(|| [1, 3].into_iter().filter(|q| kick_beats.contains(q)).collect());
    let form = opt.form;
    let bars: i64 = form.iter().map(|p| p.0).sum();
    let mut song_s = 0.0f64;
    for b in 0..bars {
        song_s += 4.0 * 60.0 / (bpm * (1.0 + drift * b as f64 / bars as f64));
    }
    let dur = LEAD + song_s + 1.0;
    let n = (dur * RATE as f64) as usize;
    let mut x = vec![0.0f32; n];
    let kk = kick();
    let sn = snare(&mut rng);
    let ht = hat(&mut rng);
    let mut s = Synth {
        s16: Vec::new(),
        beats: vec![],
        downbeats: vec![],
        kicks: vec![],
        snares: vec![],
        hats: vec![],
        starts: HashMap::new(),
        dur,
        bpm,
    };
    let mut t = LEAD;
    let mut pad_amp: Vec<f32> = Vec::new();
    let mut bass_on: Vec<f64> = Vec::new();
    let mut lead_on: Vec<bool> = Vec::new();
    for &(nbars, name) in form {
        s.starts.insert(name, t);
        for b in 0..nbars {
            let u = b as f64 / nbars as f64;
            let beat = 60.0 / (bpm * (1.0 + drift * s.downbeats.len() as f64 / bars as f64));
            s.downbeats.push(t as f32);
            let loud = if name == "finale" { 1.41 } else { 1.0 };
            for q in 0..4i64 {
                let tb = t + q as f64 * beat;
                s.beats.push(tb as f32);
                if KICK_PARTS.contains(&name) && kick_beats.contains(&q) {
                    add(&mut x, &kk, tb, kick_gain(name) * loud);
                    s.kicks.push(tb as f32);
                }
                if SNARE_PARTS.contains(&name) && snare_beats.contains(&q) {
                    add(&mut x, &sn, tb, 0.5 * loud);
                    s.snares.push(tb as f32);
                }
                if opt.hats8 && (name == "drop1" || name == "finale") {
                    add(&mut x, &ht, tb, 0.3 * loud);
                    s.hats.push(tb as f32);
                }
                if HAT_PARTS.contains(&name) {
                    let g = if name == "intro" || name == "breakdown" { 0.25 } else { 0.35 };
                    add(&mut x, &ht, tb + beat * 0.5, g * loud);
                    s.hats.push((tb + beat * 0.5) as f32);
                }
                let roll = match name {
                    "build" => Some(if b < 4 { 2 } else { 4 }),
                    "lbuild" => Some(if b < 8 { 2 } else { 4 }),
                    _ => None,
                };
                if let Some(div) = roll {
                    for k in 0..div {
                        let tk = tb + k as f64 * beat / div as f64;
                        add(&mut x, &sn, tk, 0.12 + 0.3 * u);
                        s.snares.push(tk as f32);
                    }
                }
            }
            pad_amp.push(pad(name) as f32);
            bass_on.push(if BASS_PARTS.contains(&name) { bass_gain(name) } else { 0.0 });
            lead_on.push(LEAD_PARTS.contains(&name));
            t += 4.0 * beat;
        }
    }
    // pad, bass, riser, lead: per sample (rotating phasors, f32 as Vector2)
    let nbt = s.beats.len();
    let mut bt = s.beats.clone();
    bt.push(t as f32);
    let mut osc = [(1.0f32, 0.0f32); 5];
    let rot: Vec<(f32, f32)> = [220.0, 277.2, 329.6, 82.4, 659.3]
        .iter()
        .map(|&f: &f64| {
            let w = std::f64::consts::TAU * f / RATE as f64;
            (w.cos() as f32, w.sin() as f32)
        })
        .collect();
    let get = |a: &str, b: &str| *s.starts.get(a).or(s.starts.get(b)).unwrap_or(&-1.0);
    let b_start = get("build", "lbuild");
    let b_end = get("finale", "bigdrop");
    let mut bi: i64 = -1;
    for i in 0..n {
        let ts = i as f64 / RATE as f64;
        while bi + 1 <= nbt as i64 && bt[(bi + 1) as usize] as f64 <= ts {
            bi += 1;
        }
        let bar = bi / 4;
        if bi < 0 || bar as usize >= pad_amp.len() {
            continue;
        }
        let bar = bar as usize;
        for k in 0..5 {
            let (ox, oy) = (osc[k].0 as f64, osc[k].1 as f64);
            let (rx, ry) = (rot[k].0 as f64, rot[k].1 as f64);
            osc[k] = ((ox * rx - oy * ry) as f32, (ox * ry + oy * rx) as f32);
        }
        let mut v = pad_amp[bar] as f64 * (osc[0].1 as f64 + osc[1].1 as f64 + osc[2].1 as f64) / 3.0;
        if bass_on[bar] > 0.0 {
            let b0 = bt[bi as usize] as f64;
            let ph = (ts - b0) / (bt[bi as usize + 1] as f64 - b0);
            v += bass_on[bar] * (ph * 4.0).min(1.0) * osc[3].1 as f64;
        }
        if ts >= b_start && ts < b_end {
            v += 0.08 * (ts - b_start) / (b_end - b_start) * rng.range(-1.0, 1.0);
        }
        if lead_on[bar] {
            v += 0.05 * osc[4].1 as f64;
        }
        x[i] = (x[i] as f64 + v) as f32;
    }
    s.s16 = x.iter().map(|&v| ((v as f64).clamp(-1.0, 1.0) * 32000.0) as i16).collect();
    s
}

impl Synth {
    /// Mono f32 at 44.1 kHz: the 16-bit samples upsampled x2 with the
    /// 4-point cubic of Godot's AudioStreamPlaybackResampled (3 frames of
    /// zero history, so the output lags 2 source frames as in Godot).
    pub fn pcm_44k(&self) -> Vec<f32> {
        let src: Vec<f32> = self.s16.iter().map(|&v| v as f32 / 32768.0).collect();
        let at = |i: i64| if i >= 0 && (i as usize) < src.len() { src[i as usize] } else { 0.0 };
        let total = (self.dur * 44100.0) as usize;
        let mut out = Vec::with_capacity(total);
        for o in 0..total {
            let j = (o / 2) as i64; // source index of y3 is j, y1 = j - 2
            let mu: f32 = if o % 2 == 0 { 0.0 } else { 0.5 };
            let (y0, y1, y2, y3) = (at(j - 3), at(j - 2), at(j - 1), at(j));
            let mu2 = mu * mu;
            let h11 = mu2 * (mu - 1.0);
            let z = mu2 - h11;
            let h01 = z - h11;
            let h10 = mu - z;
            out.push(y1 + (y2 - y1) * h01 + ((y2 - y0) * h10 + (y3 - y1) * h11) * 0.5);
        }
        out
    }
}
