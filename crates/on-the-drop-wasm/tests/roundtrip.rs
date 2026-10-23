//! The files the web app exports (through this binding) read back through
//! the core's readers (`on_the_drop::format`), and a re-import of the same
//! recording from other PCM finds its labels by fingerprint.

#[path = "../../on-the-drop/tests/common/synth.rs"]
mod synth;

use on_the_drop::format::{self, MapData};
use on_the_drop::track_id::TrackRecord;
use on_the_drop_wasm::api;
use serde_json::Value;

fn v(s: &str) -> Value {
    serde_json::from_str(s).unwrap()
}

#[test]
fn exports_read_back_and_reimport_finds_labels() {
    let s = synth::make(&[0, 1, 2, 3], synth::Opt::default());
    let mono = s.pcm_44k();
    let analysis = api::analyze(&mono, 1, 44100.0);
    let track = api::fingerprint(&mono, 1, 44100);
    let rec: TrackRecord = serde_json::from_str(&track).unwrap();

    // The user relabels section 1; the labels file keeps the track record.
    let a = v(&api::apply_labels(&analysis, "", &rec.key, "a.wav", &track).unwrap());
    let auto1 = a["auto_words"][1].as_str().unwrap();
    let word = a["map"]["vocabulary"]
        .as_array()
        .unwrap()
        .iter()
        .map(|w| w.as_str().unwrap())
        .find(|w| *w != auto1)
        .unwrap()
        .to_string();
    let r = v(&api::relabel(&analysis, "", &rec.key, "a.wav", &track, 1, &word).unwrap());
    assert_eq!(r["user"][1], true, "section 1 is the user's");
    assert_eq!(r["map"]["words"][1], word.as_str(), "the user's word shows");
    let labels_json = r["labels"].as_str().unwrap();
    let labels = format::labels_from_json(labels_json).expect("the exported labels read back");
    let t = &labels.tracks[&rec.key];
    assert_eq!(t.track.as_ref(), Some(&rec), "the fingerprint record is kept");
    assert_eq!(t.labels.len(), 1);
    assert_eq!(t.labels[0].label, word);
    assert_eq!(format::labels_to_json(&labels), labels_json, "labels round-trip unchanged");

    // The map bundle: the labelled map with its record.
    let map_json = serde_json::to_string(&r["map"]).unwrap();
    let file = api::map_file(&map_json, &track).unwrap();
    let (m, back) = format::map_from_json(&file).expect("the exported map reads back");
    assert_eq!(back.as_ref(), Some(&rec));
    let want: MapData = serde_json::from_str(&map_json).unwrap();
    assert_eq!(MapData::from(&m), want, "map round-trips unchanged");
    assert_eq!(format::map_to_json(&m, back.as_ref()), file);

    // Reset to auto: the word of the analysis removes the label.
    let auto = r["auto_words"][1].as_str().unwrap();
    let reset = v(&api::relabel(&analysis, labels_json, &rec.key, "a.wav", &track, 1, auto).unwrap());
    assert_eq!(reset["user"][1], false);
    assert!(format::labels_from_json(reset["labels"].as_str().unwrap()).unwrap().tracks.is_empty());

    // Another file of the same recording: other PCM (stereo, a little
    // padding), so another key; matches() finds the labels.
    let mut stereo: Vec<f32> = vec![0.0; 2 * 4410];
    stereo.extend(mono.iter().flat_map(|&x| [x * 0.9, x * 0.8]));
    let track2 = api::fingerprint(&stereo, 2, 44100);
    let rec2: TrackRecord = serde_json::from_str(&track2).unwrap();
    assert_eq!(api::find_label_key(labels_json, &track2).unwrap(), rec.key, "the re-import finds the labels");
    assert!(
        v(&api::matches(&rec.fingerprint, rec.duration, &rec2.fingerprint, rec2.duration))["matched"]
            .as_bool()
            .unwrap()
    );

    // Import merge: an exported file under another key merges into the
    // entry of the same recording.
    let moved = labels_json.replace(&rec.key, "elsewhere");
    let mut empty = labels.clone();
    empty.tracks.get_mut(&rec.key).unwrap().labels.clear();
    let m = v(&api::merge_labels(&format::labels_to_json(&empty), &moved).unwrap());
    assert_eq!((m["merged"].as_u64(), m["added"].as_u64()), (Some(1), Some(0)));
    let merged = format::labels_from_json(m["labels"].as_str().unwrap()).unwrap();
    assert_eq!(merged.tracks[&rec.key].labels, t.labels);
    // A different song is added, not merged.
    let other =
        synth::make(&[0, 2], synth::Opt { bpm: 117.3, snare: Some(vec![2]), hats8: true, ..Default::default() });
    let otrack = api::fingerprint(&other.pcm_44k(), 1, 44100);
    let orec: TrackRecord = serde_json::from_str(&otrack).unwrap();
    assert_eq!(api::find_label_key(labels_json, &otrack).unwrap(), "");
    let mut ol = labels.clone();
    let mut e = ol.tracks.remove(&rec.key).unwrap();
    e.track = Some(orec.clone());
    ol.tracks.insert(orec.key.clone(), e);
    let m2 = v(&api::merge_labels(labels_json, &format::labels_to_json(&ol)).unwrap());
    assert_eq!((m2["merged"].as_u64(), m2["added"].as_u64()), (Some(0), Some(1)));

    // Refusals: another analysis version, another format.
    assert!(api::read_map_file(&file.replace("\"analysis\":11", "\"analysis\":10")).is_err());
    assert!(api::normalize_labels(r#"{"format":"x","tracks":{}}"#).is_err());
    // frame-visualizer's old labels file (no format key) reads.
    assert!(api::normalize_labels(r#"{"tracks":{"h":{"file":"f","labels":[]}}}"#).is_ok());
}

#[test]
fn bad_input_is_empty() {
    assert_eq!(api::analyze(&[], 2, 44100.0), "{}");
    assert_eq!(api::analyze(&[0.0; 10], 3, 44100.0), "{}");
    assert_eq!(api::analyze(&[0.0; 10], 1, 0.0), "{}");
    assert_eq!(api::fingerprint(&[0.0; 10], 1, 0), "{}");
    assert!(!v(&api::matches("", 1.0, "", 1.0))["matched"].as_bool().unwrap());
    assert!(api::relabel("{}", "", "k", "", "", 0, "Drop").is_err());
}
