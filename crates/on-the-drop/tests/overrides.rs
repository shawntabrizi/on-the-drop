//! User section labels (src/overrides.rs): ports of the logic tests of
//! frame-visualizer `test/test_section_labels.gd` (override_timeline_and_drops,
//! version_bump, reanchor, split_pieces, reset_to_auto). The UI parts (the
//! timeline, the menu, the gesture) and the scenes' section tracker stay in
//! the app; here the map's drops stand for what the tracker fires.

use on_the_drop::format::{labels_from_json, labels_to_json, map_from_json, map_to_json};
use on_the_drop::map::{self, DROP_EV, SongMap};
use on_the_drop::overrides::{LabelSession, Labels};
use on_the_drop::sections::{BREAKDOWN as K, BUILD as B, CALM as C, DROP as D};

/// A dance track's map: Intro 0, Build 16, Drop 32, Breakdown 64, Drop 80,
/// Outro 112..128 s (or other starts / ids).
fn map_with(starts: &[f32], ids: &[i32]) -> SongMap {
    let mut m = SongMap::new();
    m.duration = 128.0;
    m.character = map::DANCE;
    for (s, id) in starts.iter().zip(ids) {
        m.sections.push(*s);
        m.sections.push(*id as f32);
    }
    m.set_section_ids(ids, &[]); // fills drops / section starts like the analysis
    m.drop_strength = (0..m.drops.len()).map(|i| 0.4 + 0.2 * i as f32).collect(); // 0.4, 0.6
    m
}

fn dance() -> SongMap {
    map_with(&[0.0, 16.0, 32.0, 64.0, 80.0, 112.0], &[C, B, D, K, D, C])
}

/// A new session on `m` with the labels of `key` (as at app start).
fn session(m: &mut SongMap, labels: &mut Labels, key: &str) -> LabelSession {
    let mut s = LabelSession::new(m);
    s.apply(m, labels, key, "");
    s
}

fn approx(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-5
}

#[test]
fn override_timeline_and_drops() {
    // Why: Shawn's ask: the user's label is the truth everywhere. If a Drop
    // becomes a Breakdown, no drop fires there (no flower burst on a quiet
    // part); if a section becomes a Drop, a drop fires at its start, with
    // full strength (the user says it is a drop).
    let mut m = dance();
    let mut labels = Labels::default();
    let mut s = session(&mut m, &mut labels, "trackA");
    assert_eq!(s.words, ["Intro", "Build", "Drop", "Breakdown", "Drop", "Outro"], "no labels: the analysis' words");
    assert_eq!(m.drops, [32.0, 80.0], "no labels: drops at 32 and 80 s");
    assert!(s.set_label(&mut m, &mut labels, 4, "Breakdown")); // the drop at 80 s is not one
    assert!(m.drops == [32.0] && m.drop_strength.len() == 1 && approx(m.drop_strength[0] as f64, 0.4), "Drop -> Breakdown: the drop and its strength go ({:?}, {:?})", m.drops, m.drop_strength);
    assert_eq!(m.section_at(90.0), K, "the scenes read a breakdown at 90 s");
    assert_eq!(m.next_time(DROP_EV, 40.0), f64::INFINITY, "t_to_drop: no drop is coming after 40 s");
    s.set_label(&mut m, &mut labels, 5, "Drop"); // the outro is the real last drop
    assert_eq!(m.drops, [32.0, 112.0], "Outro -> Drop: a drop at its start");
    assert!(approx(m.drop_strength_at(112.0), 1.0) && approx(m.drop_strength_at(32.0), 0.4), "a user-made drop has strength 1.0, the analysis' drop keeps its own ({:?})", m.drop_strength);
    assert!(approx(m.next_time(DROP_EV, 100.0) - 100.0, 12.0), "t_to_drop counts down to the user's drop");
    assert!(s.words[4] == "Breakdown" && s.words[5] == "Drop" && s.words[2] == "Drop", "the shown words follow ({:?})", s.words);
    // Why: an unknown word (e.g. from a newer app) must not corrupt the map.
    assert!(!s.set_label(&mut m, &mut labels, 1, "Chorus"), "an unknown word changes nothing");
    assert_eq!(labels.tracks["trackA"].labels.len(), 2, "both labels are stored");
    assert!(labels.truth_tsv().contains("trackA\t\t80.000\t112.000\tDrop\tBreakdown"), "the ground truth has key, start, end, auto and user word");
}

#[test]
fn version_bump() {
    // Why: a new analysis (ANALYSIS_VERSION bump) makes the saved map
    // unusable and analyses the song again; the user's labels live in their
    // own file, keyed by the track (not the analysis), and must survive that.
    let mut m = dance();
    let mut labels = Labels::default();
    let mut s = session(&mut m, &mut labels, "track-key");
    s.set_label(&mut m, &mut labels, 2, "Breakdown");
    let file = labels_to_json(&labels);
    let mut old_map: serde_json::Value = serde_json::from_str(&map_to_json(&m, None)).unwrap();
    old_map["analysis"] = serde_json::json!(on_the_drop::ANALYSIS_VERSION - 1);
    assert!(map_from_json(&old_map.to_string()).is_err(), "a map of another analysis version is not used");
    // A restart and a new analysis: a new file reader, a new map.
    let mut again = labels_from_json(&file).expect("labels load");
    let mut m2 = dance();
    let s2 = session(&mut m2, &mut again, "track-key");
    assert!(s2.words[2] == "Breakdown" && s2.is_user(2) && !m2.drops.contains(&32.0), "after a version bump the label still applies ({:?}, drops {:?})", s2.words, m2.drops);
}

#[test]
fn reanchor() {
    // Why: re-analysis moves boundaries by a bar or so; the label is for that
    // part of the song, not for "section 4", so it follows the part. A label
    // whose part no longer exists must not land on a wrong section.
    let mut labels = Labels::default();
    let mut m = dance();
    let mut s = session(&mut m, &mut labels, "trackR");
    s.set_label(&mut m, &mut labels, 4, "Breakdown"); // 80-112 s
    let mut moved = map_with(&[0.0, 16.0, 30.0, 62.0, 82.0, 110.0], &[C, B, D, K, D, C]);
    let l2 = session(&mut moved, &mut labels, "trackR");
    assert!(l2.words[4] == "Breakdown" && l2.is_user(4) && !l2.is_user(3), "a 2 s boundary shift: the label stays on its section ({:?})", l2.words);
    assert_eq!(moved.drops, [30.0], "... and the drops follow it");
    let e = &labels.tracks["trackR"].labels[0];
    assert!(approx(e.start, 82.0) && approx(e.end, 110.0), "the label is re-anchored to the new times ({e:?})");
    // An extra index before it: the index changes, the time does not.
    let mut split = map_with(&[0.0, 8.0, 16.0, 32.0, 64.0, 82.0, 110.0], &[C, C, B, D, K, D, C]);
    let l3 = session(&mut split, &mut labels, "trackR");
    assert!(l3.is_user(5) && l3.words[5] == "Breakdown" && !l3.is_user(4), "a new section before it: anchored by time, not index ({:?})", l3.words);
    // No section overlaps it by more than half: dropped.
    let mut other = map_with(&[0.0, 60.0], &[C, D]);
    let l4 = session(&mut other, &mut labels, "trackR");
    assert!(!l4.is_user(0) && !l4.is_user(1) && other.drops == [60.0], "a label that matches no section is not applied ({:?})", l4.words);
    assert!(!labels.tracks.contains_key("trackR"), "... and it is dropped from the file");
}

#[test]
fn split_pieces() {
    // A dance map split at part boundaries (SongMap split_parts): Intro
    // 0 | 8, Build 16, Drop 32 | 48, Breakdown 64, Drop 80, Outro 112.
    let mut labels = Labels::default();
    let mut m = map_with(&[0.0, 8.0, 16.0, 32.0, 48.0, 64.0, 80.0, 112.0], &[C, C, B, D, D, K, D, C]);
    let mut s = session(&mut m, &mut labels, "trackS");
    // Why: each piece of a split section keeps the section's word; the
    // second Intro piece is not a "Breakdown" (a calm part before a drop).
    assert!(s.words[0] == "Intro" && s.words[1] == "Intro" && s.words[3] == "Drop" && s.words[4] == "Drop", "split pieces show their section's word ({:?})", s.words);
    assert_eq!(m.drops, [32.0, 80.0], "a Drop | Drop split is one drop");
    // Why: Shawn's reason for the split: he can relabel one real part. The
    // label is for that piece only; the other piece and the drop stay.
    s.set_label(&mut m, &mut labels, 4, "Breakdown"); // 48-64 s
    assert!(s.is_user(4) && !s.is_user(3) && s.words[3] == "Drop" && s.words[4] == "Breakdown", "a label applies to one piece only ({:?})", s.words);
    assert!(m.drops == [32.0, 80.0] && m.section_at(40.0) == D && m.section_at(50.0) == K, "... and the scenes follow it ({:?})", m.drops);
    let e = &labels.tracks["trackS"].labels[0];
    assert!(approx(e.start, 48.0) && approx(e.end, 64.0), "the label is stored for the piece ({e:?})");
    // Why: a label made before the split covers a section that is now two
    // pieces, and matches neither by more than half. It is Shawn's ground
    // truth: it must stay on both pieces, not be dropped.
    let mut labels = Labels::default();
    let mut m = dance();
    let mut old = session(&mut m, &mut labels, "trackS");
    old.set_label(&mut m, &mut labels, 2, "Breakdown"); // 32-64 s, unsplit
    let mut split = map_with(&[0.0, 16.0, 32.0, 48.0, 64.0, 80.0, 112.0], &[C, B, D, D, K, D, C]);
    let l2 = session(&mut split, &mut labels, "trackS");
    assert!(l2.is_user(2) && l2.is_user(3) && l2.words[2] == "Breakdown" && l2.words[3] == "Breakdown" && !l2.is_user(1) && !l2.is_user(4), "an old label on a now split section goes to each piece ({:?})", l2.words);
    assert_eq!(split.drops, [80.0], "... and its drop is gone for the scenes");
}

#[test]
fn reset_to_auto() {
    // Why: the user must be able to undo a label: Reset gives back the
    // analysis exactly (drops, strengths, words) and forgets the label.
    let mut labels = Labels::default();
    let mut m = dance();
    let mut s = session(&mut m, &mut labels, "trackZ");
    let (drops0, st0) = (m.drops.clone(), m.drop_strength.clone());
    s.set_label(&mut m, &mut labels, 2, "Build");
    s.set_label(&mut m, &mut labels, 1, "Intro");
    assert!(!m.drops.contains(&32.0), "Drop -> Build: no drop at 32 s");
    s.set_label(&mut m, &mut labels, 2, "");
    s.set_label(&mut m, &mut labels, 1, "Build"); // picking the auto word is a reset too
    assert!(!s.is_user(1) && !s.is_user(2) && s.words[2] == "Drop", "reset: back to auto ({:?})", s.words);
    assert!(m.drops == drops0 && m.drop_strength == st0, "reset: the analysis' drops and strengths come back ({:?} {:?})", m.drops, m.drop_strength);
    assert!(!labels.tracks.contains_key("trackZ"), "reset: nothing is stored for the track");
}

#[test]
fn labels_file_round_trip_and_fv_file() {
    let mut labels = Labels::default();
    let mut m = dance();
    let mut s = session(&mut m, &mut labels, "k1");
    s.set_label(&mut m, &mut labels, 4, "Breakdown");
    let js = labels_to_json(&labels);
    // Why: the file is the user's ground truth; it must read back unchanged,
    // and say what it is so a newer reader can migrate it.
    assert_eq!(labels_from_json(&js).unwrap(), labels);
    let v: serde_json::Value = serde_json::from_str(&js).unwrap();
    assert!(v["format"] == "on-the-drop/labels" && v["version"] == 1, "{v}");
    // Why: labels made in frame-visualizer (no "format" key, its own track
    // keys) must load, so a user's earlier work is not lost.
    let fv = r#"{"tracks": {"9f0c": {"file": "a.ogg", "labels": [{"start": 80.0, "end": 112.0, "auto": "Drop", "label": "Breakdown"}]}}}"#;
    let l = labels_from_json(fv).expect("fv file loads");
    assert_eq!(l.tracks["9f0c"].labels[0].label, "Breakdown");
    // Why: a frame-visualizer entry keyed by a track record carries that
    // record, so a re-encoded copy (another exact key) can find it by
    // fingerprint; a label change must keep the record, or the next
    // device's copy loses the labels.
    let rec = on_the_drop::track_id::TrackRecord::new(&[1, 2, 3], 128.0, 44100);
    let mut lt = Labels::default();
    lt.tracks.insert(rec.key.clone(), on_the_drop::overrides::TrackLabels { file: "a.ogg".into(), labels: vec![], track: Some(rec.clone()) });
    let mut m2 = dance();
    let mut s2 = session(&mut m2, &mut lt, &rec.key.clone());
    s2.set_label(&mut m2, &mut lt, 4, "Breakdown");
    let back = labels_from_json(&labels_to_json(&lt)).unwrap();
    assert_eq!(back.tracks[&rec.key].track.as_ref(), Some(&rec), "the record survives a label change and the file");
    assert_eq!(back.tracks[&rec.key].labels.len(), 1);
    // Why: a file from a newer format must not be read as if it were ours.
    let newer = js.replace("\"version\": 1", "\"version\": 2");
    assert!(labels_from_json(&newer).is_err(), "a newer labels version is refused");
}
