//! Tests for the song map (port of frame-visualizer `test/test_song_map.gd`).
//! The synthetic track (tests/common/synth.rs) is an EDM form with known
//! beats, hits and sections: intro, drop, breakdown, build, a louder finale,
//! then a quiet outro (the cooldown). Not ported: the live Analyzer's
//! planned beats and the scene API (frame-visualizer's real-time code, not
//! part of this crate).

mod common;

use common::synth::{self, Opt, Synth};
use common::{build, match_times, nearest};
use on_the_drop::map::{self, DANCE, DOWNBEAT, DROP_EV, KICK, SONG, SongMap};
use on_the_drop::sections::{BREAKDOWN, BUILD, CALM, DROP};
use std::sync::OnceLock;

fn default() -> &'static (Synth, SongMap) {
    static D: OnceLock<(Synth, SongMap)> = OnceLock::new();
    D.get_or_init(|| {
        let s = synth::make(&[0, 1, 2, 3], Opt::default());
        let m = build(&s);
        (s, m)
    })
}

fn half_time() -> &'static (Synth, SongMap) {
    static D: OnceLock<(Synth, SongMap)> = OnceLock::new();
    D.get_or_init(|| {
        let s = synth::make(&[0, 2], Opt { bpm: 117.3, snare: Some(vec![2]), hats8: true, ..Default::default() });
        let m = build(&s);
        (s, m)
    })
}

fn drifting() -> &'static (Synth, SongMap) {
    static D: OnceLock<(Synth, SongMap)> = OnceLock::new();
    D.get_or_init(|| {
        let s = synth::make(&[0, 1, 2, 3], Opt { drift: 0.03, ..Default::default() });
        let m = build(&s);
        (s, m)
    })
}

fn form(f: synth::Form) -> (Synth, SongMap) {
    let s = synth::make(&[0, 1, 2, 3], Opt { form: f, ..Default::default() });
    let m = build(&s);
    (s, m)
}

fn st(s: &Synth, name: &str) -> f64 {
    s.starts[name]
}

#[test]
fn beat_grid() {
    let (s, m) = default();
    let b = match_times(&m.beats, &s.beats);
    // Why: the tempo comes from the whole song, so it is right from the
    // first second (the live clock needs ~5 matching kicks to lock).
    assert!((m.bpm - synth::BPM).abs() < 0.3, "tempo from the whole song ({:.2} BPM)", m.bpm);
    // Why: scenes schedule on the grid; a beat that is 1-2 frames off reads
    // as late. Every true beat, from the first one (no lock-in delay), even
    // in the intro and breakdown where only off-beat hats play (a grid that
    // follows the hats would sit half a beat off there).
    assert!(b.0 == s.beats.len() && b.1 == 0, "every beat found, none extra ({} of {}, {} extra)", b.0, s.beats.len(), b.1);
    assert!(b.3 <= 0.012, "beat error <= 12 ms (median {:.1}, max {:.1} ms)", b.2 * 1000.0, b.3 * 1000.0);
    assert!((m.beats[0] as f64 - synth::LEAD).abs() < 0.012, "the grid starts on the music's first beat ({:.3} s)", m.beats[0]);
    let d = match_times(&m.downbeats, &s.downbeats);
    // Why: bars and phrases hang off beat 1; a downbeat on beat 2 puts every
    // phrase change and every "on the bar" burst a beat early.
    assert!(d.0 == s.downbeats.len() && d.1 == 0, "every downbeat on beat 1 ({} of {}, {} wrong)", d.0, s.downbeats.len(), d.1);
}

/// Half-time EDM (Flume "Never Be Like You" style): kick on 1 and 3 only,
/// the snare on 3, hats on every 8th, at a tempo that is not a whole number.
#[test]
fn half_time_grid() {
    let (h, m) = half_time();
    let bpm = 117.3;
    // Why: the grid must not need a kick on every beat. With kicks on half
    // the beats the old kick-anchored grid ran 0.6 % slow on Flume, so the
    // drops sat up to a beat off. A 0.1 % error drifts a quarter beat over
    // a 4-minute song.
    assert!(m.fixed_tempo && (m.bpm / bpm - 1.0).abs() < 0.001, "half-time: tempo within 0.1 % ({:.3} BPM of {bpm})", m.bpm);
    let b = match_times(&m.beats, &h.beats);
    assert!(b.0 == h.beats.len() && b.1 == 0 && b.3 <= 0.012, "half-time: every beat within 12 ms ({:?})", b);
    // Why: kick on 1 and 3 looks the same on beat 1 and beat 3; the bar
    // starts where the bass and the drums come in. A downbeat on 3 puts every
    // bar-timed burst half a bar off.
    let d = match_times(&m.downbeats, &h.downbeats);
    assert!(d.0 == h.downbeats.len() && d.1 == 0, "half-time: every downbeat on beat 1 ({:?})", d);
    // Why: the fireworks hero shell bursts on the drop time: it must be the
    // downbeat where the drop comes in, not the section frame near it.
    assert!(
        m.drops.len() == 2
            && (m.drops[0] as f64 - st(h, "drop1")).abs() < 0.012
            && (m.drops[1] as f64 - st(h, "finale")).abs() < 0.012,
        "half-time: the drops land on their downbeats ({:?})",
        m.drops
    );
    assert!(
        m.drops.iter().all(|&t| nearest(&m.downbeats, t as f64).abs() < 1e-4),
        "half-time: each drop is one of the map's downbeats"
    );
}

/// A live band speeding up: +3 % tempo over the song (124 -> 127.7 BPM).
#[test]
fn drift() {
    let (h, m) = drifting();
    // Why: one constant grid would be up to ~0.17 s off this song in the
    // middle and at the ends; a drifting track must keep the beat-by-beat grid.
    assert!(!m.fixed_tempo && m.grid_drift > map::DRIFT_MAX, "drifting track: no constant grid (drift {:.0} ms)", m.grid_drift * 1000.0);
    // The outro is a pad without any beat: only the beats up to it count.
    let end = st(h, "outro") - 0.1;
    let tb: Vec<f32> = h.beats.iter().copied().filter(|&x| (x as f64) < end).collect();
    let db: Vec<f32> = m.beats.iter().copied().filter(|&x| (x as f64) < end).collect();
    let b = match_times(&db, &tb);
    assert!(b.0 == tb.len() && b.1 == 0 && b.3 <= 0.025, "drifting track: the grid follows the drift ({:?} of {})", b, tb.len());
    // Why: the steady test track must take the constant grid (the default
    // for programmed music).
    let (_, d) = default();
    assert!(d.fixed_tempo && d.grid_drift < 0.01, "the steady track has one constant grid (drift {:.1} ms)", d.grid_drift * 1000.0);
}

#[test]
fn events() {
    let (s, m) = default();
    // Why: scenes bind one element per drum (a kick ring, a snare puff, hat
    // sparkle); a class that also fires on the others' hits binds wrongly.
    for (name, det, truth, share) in
        [("kick", &m.kicks, &s.kicks, 0.95), ("snare", &m.snares, &s.snares, 0.85), ("hat", &m.hats, &s.hats, 0.95)]
    {
        let r = match_times(det, truth);
        let n = truth.len() as f64;
        assert!(r.0 as f64 >= share * n, "{name}: found {} of {n}", r.0);
        assert!(r.1 as f64 <= 0.08 * n, "{name}: {} false (of {} detected)", r.1, det.len());
        // Why: "land exactly on a hit" (a firework burst on the snare) needs
        // the hit's time within about one 90 Hz frame.
        assert!(r.2 <= 0.008 && r.3 <= 0.016, "{name}: time error median {:.1} ms, max {:.1} ms", r.2 * 1000.0, r.3 * 1000.0);
    }
    assert!(m.amp(KICK, 0) > 0.0 && m.amp(KICK, 0) <= 1.0, "event strengths are 0..1");
}

#[test]
fn song_form() {
    let (s, m) = default();
    let beat = 60.0 / synth::BPM;
    // Why: the drop must fire on its downbeat (one frame late is visible as
    // a stutter; one beat off is wrong music).
    assert!(
        m.drops.len() == 2
            && (m.drops[0] as f64 - st(s, "drop1")).abs() < 0.012
            && (m.drops[1] as f64 - st(s, "finale")).abs() < 0.012,
        "drops at the drop and the finale, on the downbeat ({:?})",
        m.drops
    );
    assert_eq!(m.section_at(st(s, "breakdown") + 2.0), BREAKDOWN, "the breakdown is a breakdown");
    assert_eq!(m.section_at(st(s, "finale") - 2.0), BUILD, "the build before the finale is a build");
    // Why: the grand finale is the scene's biggest moment: the louder last
    // drop, not the first one.
    assert!(
        (m.finale_start - st(s, "finale")).abs() < 0.012 && (m.finale_end - st(s, "outro")).abs() < beat,
        "finale = the last, loudest drop ({:.2}-{:.2} s)",
        m.finale_start,
        m.finale_end
    );
    // Why: after the finale the scene winds down (cooldown); before it, it
    // must not.
    assert!(
        m.in_cooldown(st(s, "outro") + 2.0) && !m.in_cooldown(st(s, "finale") + 2.0) && !m.in_cooldown(st(s, "drop1") + 2.0),
        "cooldown only after the finale"
    );
    assert_eq!(m.section_at(st(s, "outro") + 2.0), CALM, "the outro is calm");
    // Why: phrase changes (a new plant species, a comet) must line up with
    // the music's 8-bar groups and restart at each section.
    assert!(m.phrases.iter().all(|&p| nearest(&s.downbeats, p as f64).abs() < 0.012), "every phrase starts on a downbeat");
    for name in ["intro", "drop1", "breakdown", "finale", "outro"] {
        assert!(nearest(&m.phrases, st(s, name)).abs() < 0.012, "a phrase starts with the {name}");
    }
    assert!(nearest(&m.phrases, st(s, "finale") + 8.0 * 4.0 * beat).abs() < 0.012, "phrases are 8 bars (finale + 8 bars)");
    assert_eq!(m.phrase_bar(st(s, "finale") + 3.5 * 4.0 * beat), 3, "bar in phrase counts from the phrase start");
    let pp = m.phrase_phase(st(s, "finale") + 4.0 * 4.0 * beat);
    assert!((pp - 0.5).abs() < 0.02, "phrase phase 0.5 half way ({pp:.2})");
}

/// A pre-drop chorus: the bass is in from the verse, the chorus is as loud
/// as the drop, then a 1 bar bass gap and the drop (LATE_FORM, the form of
/// Alan Walker "Darkside", whose drop Shawn hears at 59.0 s, not at the
/// chorus at 36.4 s). SongSections sees one loud block.
#[test]
fn late_drop() {
    let (s, m) = form(synth::LATE_FORM);
    let bar = 4.0 * 60.0 / synth::BPM;
    // Why: the drop is where the bass comes back after the gap. The scenes'
    // biggest moment (the forest bloom, t_to_drop) must land there, on the
    // entry downbeat, not 16 bars early at the chorus.
    assert!(
        m.drops.len() == 1 && (m.drops[0] as f64 - st(&s, "drop")).abs() < 0.012,
        "the drop is the bass entry after the gap ({:?}, entry {:.2} s)",
        m.drops,
        st(&s, "drop")
    );
    // Why: the loud chorus before it leads into the drop: it is its build
    // (tension, then release), and the change is a section boundary.
    assert!(
        m.section_at(st(&s, "chorus") + 2.0) == BUILD
            && m.section_at(st(&s, "drop") + 2.0) == DROP
            && nearest(&m.phrases, st(&s, "drop")).abs() < 0.012,
        "the chorus is the build, the drop starts a section ({:?})",
        m.sections
    );
    // Why: a split must not leave a sliver: a section under 4 bars is a
    // flicker on the timeline and in the scenes.
    let short = (0..m.sections.len().saturating_sub(2))
        .step_by(2)
        .any(|k| (m.sections[k + 2] - m.sections[k]) as f64 <= 4.0 * bar - 0.05);
    assert!(!short, "no section shorter than 4 bars ({:?})", m.sections);
    // Why: a 1 bar break inside a real drop (entered by a bass step) is a
    // fill, not a new drop: that drop must stay one section with one drop.
    let (f, mf) = form(synth::FILL_FORM);
    assert!(
        mf.drops.len() == 1 && (mf.drops[0] as f64 - st(&f, "drop1")).abs() < 0.012 && mf.section_at(st(&f, "drop") + 2.0) == DROP,
        "a break inside a real drop does not split it ({:?})",
        mf.sections
    );
}

/// A verse with the drums and the bass back but no lead (REPEAT_FORM:
/// chorus, breakdown, groove, then the chorus again as "drop"). Flume "Never
/// Be Like You": the verse at 80 s is as loud as the chorus, so SongSections
/// starts a DROP there; Shawn hears the drop at the chorus.
#[test]
fn repeat_drop_and_strength() {
    let (s, m) = form(synth::REPEAT_FORM);
    // Why: drums returning in a verse are not a drop. The scenes' big moment
    // (t_to_drop, the bloom) must wait for the chorus that repeats the first
    // drop, on its downbeat; a drop in the verse is "totally wrong" (Shawn).
    assert!(
        m.drops.len() == 2
            && (m.drops[0] as f64 - st(&s, "chorus")).abs() < 0.012
            && (m.drops[1] as f64 - st(&s, "drop")).abs() < 0.012,
        "drops at the chorus and its repeat, not at the verse ({:?})",
        m.drops
    );
    // Why: the verse sits between two drops: it is a dip, not the drop and
    // not a build (nothing rises in it).
    assert!(
        m.section_at(st(&s, "groove") + 2.0) == BREAKDOWN && m.section_at(st(&s, "drop") + 2.0) == DROP,
        "the verse is a breakdown, the repeat a drop ({:?})",
        m.sections
    );
    // Why: a later drop that already starts like the first drop must stay
    // where it is (the default form's finale repeats drop 1 plus a lead).
    let (ds, dm) = default();
    assert!(dm.drops.len() == 2 && (dm.drops[1] as f64 - st(ds, "finale")).abs() < 0.012, "a drop that starts like the first one stays ({:?})", dm.drops);
    // Each drop's strength 0..1 against the song's other drops.
    assert_eq!(m.drop_strength.len(), m.drops.len(), "one strength per drop ({:?})", m.drop_strength);
    // Why: Shawn hears Flume's chorus after the verse as a drop "but not a
    // big one": the verse already had the drums and the bass, so the entry
    // adds little. The scenes scale the drop moment by this, so the chorus
    // out of a bassless intro must be the bigger one, and the biggest drop of
    // a song is 1.
    assert!(m.drop_strength[0] > 0.999 && m.drop_strength[1] < 0.5, "the drop out of the intro is the biggest ({:?})", m.drop_strength);
    assert!(
        on_the_drop_eq(m.drop_strength_at(st(&s, "drop") + 0.5), m.drop_strength[1] as f64)
            && on_the_drop_eq(m.drop_strength_at(st(&s, "chorus") - 1.0), 1.0),
        "drop_strength_at gives the nearest drop's strength"
    );
    // Why: the default form's finale comes out of a snare-roll build with
    // the bass out and is 3 dB louder: it is the song's biggest moment.
    assert!(dm.drop_strength.len() == 2 && dm.drop_strength[1] > 0.999 && dm.drop_strength[0] > 0.0, "the finale out of a build is the biggest drop ({:?})", dm.drop_strength);
    // Why: the strengths travel with the cached map (scenes read the cache).
    let back = on_the_drop::format::map_from_json(&on_the_drop::format::map_to_json(&m, None)).unwrap().0;
    assert_eq!(back.drop_strength, m.drop_strength, "drop strengths survive the cache");
}

fn on_the_drop_eq(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-5
}

/// The whole-song drop pass (DROPS_FORM): the chorus, a dip with the bass, a
/// loud verse (drums and bass back, no lead: SongSections starts a DROP
/// there), a breakdown, a verse with the bass, then the chorus again.
#[test]
fn drop_pass() {
    let (s, m) = form(synth::DROPS_FORM);
    // Why: the loud verse repeats no drop and adds no bass entry: it is a
    // fake drop (Flume's verse at 80 s: "totally wrong", Shawn). The scenes'
    // big moment must fire only at the two choruses, each on its downbeat.
    assert!(
        m.drops.len() == 2
            && (m.drops[0] as f64 - st(&s, "chorus")).abs() < 0.012
            && (m.drops[1] as f64 - st(&s, "drop")).abs() < 0.012,
        "drops at the two choruses, not at the loud verse ({:?})",
        m.drops
    );
    // Why: between two drops the fake one is a dip, never a drop or a build
    // (nothing on the timeline or in the scenes may count down to it).
    assert_eq!(m.section_at(st(&s, "groove") + 2.0), BREAKDOWN, "the loud verse is a breakdown ({:?})", m.sections);
    assert_eq!(m.drop_strength.len(), 2, "one strength per drop ({:?})", m.drop_strength);
    // Why: the chorus out of the bassless intro is the big drop; the repeat
    // after a verse that already has the bass adds little (Flume 104 s, "a
    // drop, but not a big one"). The final-drop prior is weak: it must not
    // lift the small last drop over the big first one.
    assert!(m.drop_strength[0] > 0.999 && m.drop_strength[1] < 0.5, "the bigger drop ranks first ({:?})", m.drop_strength);
    // Why: a real drop is still a drop: scenes scale its moment by the
    // strength, so it never reaches 0 (the floor).
    assert!(m.drop_strength[1] as f64 >= map::STRENGTH_FLOOR - 1e-6, "a small real drop keeps the floor ({:?})", m.drop_strength);
}

/// An EDM build as loud as its drop (LOUD_BUILD_FORM): 16 bars of kick,
/// hats, a loud pad, a rising snare roll and a riser over a filtered bass,
/// then the drop with the full bass. On the 402 SSME tracks the total level
/// at the expert drops rises a median 0.1 dB, the low band 5.9 dB.
#[test]
fn loud_build() {
    let (s, m) = form(synth::LOUD_BUILD_FORM);
    // Why: the drop is where the bass comes in, on that downbeat. Joining the
    // loud build and the drop into one loud block fired the drop half way
    // into the build (SSME: at 164 missed drops we were already in a Peak
    // that began a median 26 s early).
    assert!(m.drops.len() == 1 && (m.drops[0] as f64 - st(&s, "bigdrop")).abs() < 0.012, "loud build: one drop, on the bass entry ({:?})", m.drops);
    assert!(nearest(&m.downbeats, m.drops[0] as f64).abs() < 1e-4, "loud build: the drop is one of the map's downbeats");
    // Why: the scenes charge up through the build (t_to_drop, build
    // progress): the loud part before the entry must be the BUILD.
    assert!(
        m.section_at(st(&s, "bigdrop") - 4.0) == BUILD && m.section_at(st(&s, "bigdrop") + 4.0) == DROP,
        "loud build: build, then drop ({:?})",
        m.sections
    );
    // Why: when the beat itself lands out of a bassless intro, that is the
    // drop, even when more bass comes later (Daft Punk "One More Time" at
    // 31.6 s, "Around the World" at 342 s). Calling that part a build would
    // fire the drop 16 bars late.
    let (e, me) = form(synth::ENTRY_FORM);
    assert!(me.drops.len() == 1 && (me.drops[0] as f64 - st(&e, "thin")).abs() < 0.012, "the beat landing is the drop ({:?})", me.drops);
}

/// The timeline vocabulary follows the track's energy character (dance /
/// song, like rekordbox's mood), never a genre; the scenes' ids stay.
#[test]
fn character() {
    let (_, m) = default();
    // Why: a four-on-the-floor EDM form (steady kick, bass drops in) is what
    // the DJ words were made for: the timeline must keep them, and name the
    // calm ends Intro and Outro.
    assert_eq!(m.character, DANCE, "four-on-the-floor EDM form is dance (cover {:.2}, step {:.1} dB)", m.kick_cover, m.drop_step);
    assert_eq!(m.section_words(&[]).join(","), "Intro,Drop,Breakdown,Build,Drop,Outro");
    // Why: half-time EDM (kick on 1, snare on 3: dubstep, trap, future bass;
    // Flume, Kygo) has the same build and drop as house. The old
    // four-on-the-floor test called 79 % of the 402 SSME EDM tracks "song",
    // so their timelines never said Build or Drop.
    let (_, h) = half_time();
    assert!(h.character == DANCE && h.section_words(&[]).iter().any(|w| w == "Drop"), "half-time EDM is dance ({:?})", h.section_words(&[]));
    // Why: a band playing the same energy form is not a dance track: it
    // drifts in tempo (a programmed track does not). Its timeline must only
    // say what the energy does: never "Drop" or "Build" (an EDM word on a
    // band song is false), even with a steady four-on-the-floor kick.
    let (rock, r) = drifting();
    assert_eq!(r.character, SONG, "a drifting band is song (fixed grid {})", r.fixed_tempo);
    let sw = r.section_words(&[]);
    assert!(
        !sw.iter().any(|w| ["Drop", "Build", "Breakdown"].contains(&w.as_str()))
            && sw.iter().any(|w| w == "Peak")
            && sw[0] == "Intro"
            && sw[sw.len() - 1] == "Outro",
        "song words, no Drop / Build: {sw:?}"
    );
    // Why: only the display changes. The scenes still read the same ids and
    // drop events, so the louder chorus is still a DROP section to them.
    assert!(r.section_at(st(rock, "finale") + 2.0) == DROP && !r.drops.is_empty(), "the song's peak is still a DROP id ({:?})", r.sections);
    // Why: every song word must be true of the energy curve. A BUILD
    // section that does not rise must not read "Rising": flat and as loud as
    // the peak -> Peak; flat and quiet -> Low.
    let mut f = SongMap::new();
    f.character = SONG;
    f.duration = 30.0;
    f.energy = vec![0; 300 * 4];
    let fill = |f: &mut SongMap, r: std::ops::Range<usize>, v: &dyn Fn(usize) -> u8| {
        for i in r {
            for c in 0..4 {
                f.energy[i * 4 + c] = v(i);
            }
        }
    };
    fill(&mut f, 0..300, &|i| if i < 100 { 120 } else if i < 200 { 240 } else { 230 });
    let secs = [0.0, CALM as f32, 10.0, BUILD as f32, 20.0, DROP as f32];
    assert_eq!(f.section_words(&secs)[1], "Peak", "a flat loud build reads Peak");
    fill(&mut f, 100..200, &|_| 120);
    assert_eq!(f.section_words(&secs)[1], "Low", "a flat quiet build reads Low");
    fill(&mut f, 100..200, &|i| (120 + (i - 100)) as u8);
    assert_eq!(f.section_words(&secs)[1], "Rising", "a rising build reads Rising");
    f.character = DANCE;
    assert_eq!(f.section_words(&secs)[1], "Build", "dance: the same section is a Build");
}

/// Band bins (SongMap.analyse input) of a form with sustained band levels:
/// parts (bars, low, mid, high, air dB, kick), 120 BPM, 0.5 s lead. A kick
/// (a decaying low and high pulse) on every beat of a part with kick, the
/// low band side-chained to it; 5 % jitter (Godot's RNG, seed 3).
fn bins(parts: &[(i64, f64, f64, f64, f64, bool)]) -> (Vec<f32>, Vec<f32>, usize, Vec<f64>) {
    let bin = 512.0 / 44100.0;
    let beat = 0.5;
    let bars: i64 = parts.iter().map(|p| p.0).sum();
    let nb = ((1.5 + (bars * 4) as f64 * beat) / bin) as usize;
    let mut raw = vec![0.0f32; nb * 3];
    let mut air = vec![0.0f32; nb];
    let mut starts = Vec::new();
    let mut t0 = 0.5;
    let mut rng = synth::Pcg::new(3);
    let db = |x: f64| (x * 0.11512925464970228420089957273422).exp();
    for p in parts {
        starts.push(t0);
        let t1 = t0 + (p.0 * 4) as f64 * beat;
        for i in (t0 / bin) as usize..((t1 / bin) as usize).min(nb) {
            let ph = (i as f64 * bin - t0) % beat / beat;
            let k = if p.5 { 0.5 * (-ph * 8.0).exp() } else { 0.0 };
            let j = 1.0 + 0.05 * rng.range(-1.0, 1.0);
            raw[i * 3] = ((db(p.1) * (ph * 4.0).min(1.0) + k) * j) as f32;
            raw[i * 3 + 1] = (db(p.2) * j) as f32;
            raw[i * 3 + 2] = ((db(p.3) + 0.06 * k) * j) as f32;
            air[i] = (db(p.4) * if ph > 0.5 && ph < 0.6 { 1.0 } else { 0.3 } * j) as f32;
        }
        t0 = t1;
    }
    (raw, air, nb, starts)
}

const BIN: f64 = 512.0 / 44100.0;
const PARTS: &[(i64, f64, f64, f64, f64, bool)] =
    &[(16, -12.0, -6.0, -20.0, -20.0, true), (16, -12.0, -20.0, -6.0, -6.0, true), (16, -12.0, -6.0, -20.0, -20.0, true)];

fn alternating() -> Vec<(i64, f64, f64, f64, f64, bool)> {
    (0..12)
        .map(|q| {
            let e = q % 2 == 0;
            (2, -12.0, if e { -6.0 } else { -20.0 }, if e { -20.0 } else { -6.0 }, if e { -20.0 } else { -6.0 }, true)
        })
        .collect()
}

/// SongNovelty: part boundaries from the beat-synchronous self-similarity
/// of the band balance, also where the energy does not change.
#[test]
fn part_bounds() {
    let (raw, air, nb, starts) = bins(PARTS);
    let m = SongMap::analyse(&raw, nb, BIN, &air);
    // Why: a new part is where the sound changes, not only where it gets
    // louder. SongSections only sees energy steps (>= 3 dB), so it finds no
    // boundary here; SongNovelty must mark both changes, on their downbeats
    // (the section snap reads them).
    let secs = on_the_drop::sections::analyse(&raw, nb, BIN, &m.beats);
    assert_eq!(secs.len(), 2, "the energy segmenter sees one part ({secs:?})");
    assert!(
        m.part_bounds.len() == 2
            && (0..2).all(|k| (m.part_bounds[k] as f64 - starts[k + 1]).abs() < 0.03
                && nearest(&m.downbeats, m.part_bounds[k] as f64).abs() < 1e-4),
        "timbre change at equal energy: a part boundary on each change downbeat ({:?}, true {:?})",
        m.part_bounds,
        &starts[1..]
    );
    // Why: a steady loop has no parts. The peak threshold is relative to the
    // song's own novelty, so without the absolute floor (and the spread
    // floor of the z-score) jitter alone would cut a loop every few bars.
    let (raw, air, nb, _) = bins(&[(48, -6.0, -6.0, -8.0, -8.0, true)]);
    let ms = SongMap::analyse(&raw, nb, BIN, &air);
    assert!(ms.part_bounds.is_empty(), "a steady loop has no part boundary ({:?})", ms.part_bounds);
    // Why: a part shorter than 4 bars is a flicker on the timeline; two
    // boundaries are always at least SongNovelty.MIN_BARS bars apart, also
    // when the sound changes every 2 bars.
    let (raw, air, nb, _) = bins(&alternating());
    let mc = SongMap::analyse(&raw, nb, BIN, &air);
    let gap = mc.part_bounds.windows(2).map(|w| (w[1] - w[0]) as f64).fold(f64::INFINITY, f64::min);
    assert!(gap >= on_the_drop::novelty::MIN_BARS as f64 * 2.0 - 0.05, "part boundaries at least 4 bars apart ({:?})", mc.part_bounds);
}

/// Shawn (2026-10-22): split sections at the part boundaries, also when both
/// sides get the same label (SongMap._split_parts). Not ported: the live
/// SectionTracker part (frame-visualizer's real-time code).
#[test]
fn part_split() {
    let (raw, air, nb, _) = bins(PARTS);
    let m = SongMap::analyse(&raw, nb, BIN, &air);
    // Why: the label editor can fix a wrong label but cannot add a missing
    // boundary. Equal-energy parts (the pad swaps for a lead and hats) must
    // be their own sections, each with the label of the section it came
    // from, so Shawn can relabel each one on the headset.
    let n = m.sections.len() / 2;
    let ok = n == 3
        && (0..3).all(|k| {
            let want = if k == 0 { 0.0 } else { m.part_bounds[k - 1] as f64 };
            (m.sections[k * 2] as f64 - want).abs() < 1e-4 && m.sections[k * 2 + 1] == m.sections[1]
        });
    assert!(ok, "equal-energy parts are separate sections with the same id ({:?}, parts {:?})", m.sections, m.part_bounds);
    // Why: a piece inherits the section's word too: one Intro split in two
    // reads Intro, Intro (not Intro, then Breakdown or Low).
    let w = m.section_words(&[]);
    assert!(w.len() == n && w.iter().all(|x| *x == w[0]), "every piece shows the section's word ({w:?})");
    // Why: a section shorter than 4 bars is a flicker on the timeline and
    // too short to aim at; the split keeps PART_MIN_BARS bars per piece.
    let (raw, air, nb, _) = bins(&alternating());
    let mc = SongMap::analyse(&raw, nb, BIN, &air);
    let shortest = (0..mc.sections.len() / 2)
        .map(|k| {
            let b = if k * 2 + 2 < mc.sections.len() { mc.sections[k * 2 + 2] as f64 } else { mc.duration };
            b - mc.sections[k * 2] as f64
        })
        .fold(f64::INFINITY, f64::min);
    assert!(mc.sections.len() >= 4 && shortest >= (map::PART_MIN_BARS as f64 - 0.25) * 2.0, "every piece keeps 4 bars ({:?})", mc.sections);
    // A split song map: Intro 0, Build 16 | 24, Drop 32 | 48, Breakdown 64, Drop 80 | 96 .. 112 s.
    let (c, b, d, k) = (CALM as f32, BUILD as f32, DROP as f32, BREAKDOWN as f32);
    let mut f = SongMap::new();
    f.duration = 112.0;
    f.character = DANCE;
    f.sections = vec![0.0, c, 16.0, b, 24.0, b, 32.0, d, 48.0, d, 64.0, k, 80.0, d, 96.0, d];
    f.set_section_ids(&[CALM, BUILD, BUILD, DROP, DROP, BREAKDOWN, DROP, DROP], &[(32.0, 0.5), (80.0, 1.0)]);
    // Why: the split is a display / structure split; a DROP -> DROP part
    // boundary is no second drop (no second flower burst one phrase in).
    assert!(f.drops == [32.0, 80.0] && f.drop_strength == [0.5, 1.0], "a DROP -> DROP split is one drop ({:?}, {:?})", f.drops, f.drop_strength);
    let g = on_the_drop::format::map_from_json(&on_the_drop::format::map_to_json(&f, None)).unwrap().0;
    assert_eq!(g.drops, [32.0, 80.0], "... also after the cache");
    // Why: the finale is the whole last drop, not its last piece: in_finale
    // starts on the drop, where the scenes stage the big moment.
    assert!(f.finale_start == 80.0 && f.finale_end == 112.0, "the finale is the whole split drop ({}-{})", f.finale_start, f.finale_end);
}

#[test]
fn energy() {
    let (s, m) = default();
    let em = |a: f64, b: f64| m.energy_mean(a, b, -1);
    let intro = em(st(s, "intro") + 1.0, st(s, "drop1") - 1.0);
    let fin = em(st(s, "finale") + 1.0, st(s, "outro") - 1.0);
    let drop = em(st(s, "drop1") + 1.0, st(s, "breakdown") - 1.0);
    let outro = em(st(s, "outro") + 1.0, st(s, "outro") + 10.0);
    // Why: whole-song normalisation keeps the shape: a quiet intro stays
    // quiet (a running average would lift it to "normal") and the climax
    // gets the top of the range.
    assert!(fin > 0.85 && fin > drop && drop > intro + 0.15 && outro < intro, "energy: intro {intro:.2} < drop {drop:.2} < finale {fin:.2}, outro {outro:.2}");
    assert!(m.energy_at(st(s, "finale") + 4.0, 0) > m.energy_at(st(s, "breakdown") + 4.0, 0) + 0.3, "low-band energy: bass in the finale, none in the breakdown");
}

#[test]
fn queries() {
    let (s, m) = default();
    let t = st(s, "drop1") + 0.1;
    let bar = 4.0 * 60.0 / synth::BPM;
    let nd = m.next_events(DOWNBEAT, t, 2);
    // Why: scheduling asks "the next N downbeats"; they must be strictly
    // after t and in order.
    assert!(
        nd.len() == 2 && nd[0] as f64 > t && (nd[0] as f64 - (st(s, "drop1") + bar)).abs() < 0.012 && ((nd[1] - nd[0]) as f64 - bar).abs() < 0.012,
        "next 2 downbeats after t ({nd:?})"
    );
    let ev = m.events_between(KICK, st(s, "drop1") - 0.03, st(s, "drop1") + bar - 0.03);
    assert_eq!(ev.len(), 4, "events in (t0, t1]: one bar of four kicks");
    assert!(
        (m.next_time(DROP_EV, 5.0) - st(s, "drop1")).abs() < 0.012 && (m.next_time(DROP_EV, st(s, "drop1") + 1.0) - st(s, "finale")).abs() < 0.012,
        "time to the next drop from the intro and from the first drop"
    );
    assert!(m.next_time(DROP_EV, st(s, "finale") + 1.0).is_infinite(), "no drop after the last");
    assert_eq!(m.next_section(st(s, "breakdown") + 1.0), BUILD, "next section after the breakdown: the build");
    assert!(
        (m.energy_at(st(s, "drop1") - 1.0 + 3.0, -1) - m.energy_at(st(s, "drop1") + 2.0, -1)).abs() < 1e-9,
        "lookahead energy = the energy there"
    );
    let bp = m.beat_phase(s.beats[10] as f64 + 0.25 * 60.0 / synth::BPM);
    assert!(bp > 0.2 && bp < 0.3, "beat phase a quarter into a beat ({bp})");
    assert!(m.next_events(KICK, 1e6, 3).is_empty() && m.next_events(map::BEAT, -1.0, 1)[0] == m.beats[0], "queries past the ends");
}

#[test]
fn cache() {
    let (_, m) = default();
    let js = on_the_drop::format::map_to_json(m, None);
    let (m2, _) = on_the_drop::format::map_from_json(&js).expect("map loads");
    // Why: the map is built once per track; a cached song must give the
    // scenes exactly what a fresh analysis gives.
    assert!(
        m2.beats == m.beats
            && m2.downbeats == m.downbeats
            && m2.kicks == m.kicks
            && m2.snares == m.snares
            && m2.hats == m.hats
            && m2.phrases == m.phrases
            && m2.drops == m.drops
            && m2.energy == m.energy
            && m2.finale_start == m.finale_start
            && m2.hat_amp == m.hat_amp,
        "the loaded map equals the built one"
    );
    // Why: the timeline words come from the cached character; a cached
    // dance track must not come back as a song (or the other way round).
    assert!(
        m2.character == m.character
            && m2.four_floor == m.four_floor
            && m2.kick_cover == m.kick_cover
            && m2.drop_step == m.drop_step
            && m2.section_words(&[]) == m.section_words(&[]),
        "the loaded map keeps the track character"
    );
    // Why: a map of an older analysis must be rebuilt, not loaded (it would
    // show the old sections and drops after an analysis change).
    let old = js.replace(
        &format!("\"analysis\":{}", on_the_drop::ANALYSIS_VERSION),
        &format!("\"analysis\":{}", on_the_drop::ANALYSIS_VERSION - 1),
    );
    assert!(on_the_drop::format::map_from_json(&old).is_err(), "a map of an older analysis is not used");
    // Why: a map without the track character (a cache from before it existed)
    // would show every song with the default words: it must not load.
    let v: serde_json::Value = serde_json::from_str(&js).unwrap();
    let mut v2 = v.clone();
    v2["map"].as_object_mut().unwrap().remove("character");
    assert!(on_the_drop::format::map_from_json(&v2.to_string()).is_err(), "a map without the character is not used");
}

/// The Rust synth is the GDScript synth: same 16-bit samples (Godot's PCG32
/// and the same f64 / f32 steps). Needs a dump from parity/fv_dump.gd
/// (synth:<form> writes <dir>/synth_<form>.s16): OTD_PARITY_DIR=<dir>.
#[test]
#[ignore]
fn synth_matches_gdscript() {
    let dir = std::env::var("OTD_PARITY_DIR").expect("OTD_PARITY_DIR");
    let cases: Vec<(&str, Synth)> = vec![
        ("default", synth::make(&[0, 1, 2, 3], Opt::default())),
        ("half", synth::make(&[0, 2], Opt { bpm: 117.3, snare: Some(vec![2]), hats8: true, ..Default::default() })),
        ("drift", synth::make(&[0, 1, 2, 3], Opt { drift: 0.03, ..Default::default() })),
        ("late", synth::make(&[0, 1, 2, 3], Opt { form: synth::LATE_FORM, ..Default::default() })),
        ("loudbuild", synth::make(&[0, 1, 2, 3], Opt { form: synth::LOUD_BUILD_FORM, ..Default::default() })),
    ];
    for (name, s) in cases {
        let b = std::fs::read(format!("{dir}/synth_{name}.s16")).expect("dump");
        let g: Vec<i16> = b.chunks_exact(2).map(|c| i16::from_le_bytes([c[0], c[1]])).collect();
        let diff = g.iter().zip(&s.s16).filter(|(a, b)| a != b).count();
        assert!(g.len() == s.s16.len() && diff == 0, "{name}: {} vs {} samples, {diff} differ", g.len(), s.s16.len());
    }
}
