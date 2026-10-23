//! Native vs wasm32 parity: the binding's JSON for the synthetic songs
//! (on-the-drop/tests/common/synth.rs) must be byte for byte the same.
//!
//! Run the native half first: it writes the JSON to target/wasm-parity/;
//! the wasm half (wasm-bindgen-test in Node) computes it again and compares.
//!
//! ```sh
//! cargo test -p on-the-drop-wasm --test parity
//! CARGO_TARGET_WASM32_UNKNOWN_UNKNOWN_RUNNER=wasm-bindgen-test-runner \
//!   cargo test -p on-the-drop-wasm --test parity --target wasm32-unknown-unknown
//! ```
//! (`scripts/wasm_test.sh` runs both.)

#[path = "../../on-the-drop/tests/common/synth.rs"]
mod synth;

use on_the_drop_wasm::api;
use synth::Opt;

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../target/wasm-parity");

fn songs() -> Vec<(&'static str, synth::Synth)> {
    let four = [0, 1, 2, 3];
    vec![
        ("default", synth::make(&four, Opt::default())),
        ("half", synth::make(&[0, 2], Opt { bpm: 117.3, snare: Some(vec![2]), hats8: true, ..Default::default() })),
        ("drift", synth::make(&four, Opt { drift: 0.03, ..Default::default() })),
        ("late", synth::make(&four, Opt { form: synth::LATE_FORM, ..Default::default() })),
        ("fill", synth::make(&four, Opt { form: synth::FILL_FORM, ..Default::default() })),
        ("repeat", synth::make(&four, Opt { form: synth::REPEAT_FORM, ..Default::default() })),
        ("drops", synth::make(&four, Opt { form: synth::DROPS_FORM, ..Default::default() })),
        ("loudbuild", synth::make(&four, Opt { form: synth::LOUD_BUILD_FORM, ..Default::default() })),
        ("entry", synth::make(&four, Opt { form: synth::ENTRY_FORM, ..Default::default() })),
    ]
}

/// Every output the app uses, for one song: (name, JSON).
fn outputs(name: &str, s: &synth::Synth) -> Vec<(String, String)> {
    let mono = s.pcm_44k();
    let mut out = Vec::new();
    let analysis = api::analyze(&mono, 1, 44100.0);
    assert!(analysis.len() > 2, "{name}: the synthetic song analyses");
    let track = api::fingerprint(&mono, 1, 44100);
    if name == "default" {
        // The stereo path (the browser decodes stereo) and a 48 kHz rate.
        let stereo: Vec<f32> = mono.iter().flat_map(|&x| [x, x * 0.5]).collect();
        out.push(("stereo".into(), api::analyze(&stereo, 2, 44100.0)));
        out.push(("stereo_fp".into(), api::fingerprint(&stereo, 2, 44100)));
        out.push(("rate48".into(), api::analyze(&mono, 1, 48000.0)));
    }
    // Labels: change the second section, then the last one back to auto.
    let first = api::apply_labels(&analysis, "", "k", "song.wav", &track).unwrap();
    let words: serde_json::Value = serde_json::from_str(&first).unwrap();
    let n = words["map"]["words"].as_array().unwrap().len() as i32;
    let vocab = words["map"]["vocabulary"].as_array().unwrap();
    let word = vocab[2].as_str().unwrap(); // Drop / Rising
    let r1 = api::relabel(&analysis, "", "k", "song.wav", &track, 1.min(n - 1), word).unwrap();
    let l1: serde_json::Value = serde_json::from_str(&r1).unwrap();
    let r2 =
        api::relabel(&analysis, l1["labels"].as_str().unwrap(), "k", "song.wav", &track, n - 1, "Breakdown").unwrap();
    let l2: serde_json::Value = serde_json::from_str(&r2).unwrap();
    let mapf = api::map_file(&serde_json::to_string(&l2["map"]).unwrap(), &track).unwrap();
    let fp: serde_json::Value = serde_json::from_str(&track).unwrap();
    let fps = fp["fingerprint"].as_str().unwrap();
    let dur = fp["duration"].as_f64().unwrap();
    out.push(("analysis".into(), analysis));
    out.push(("track".into(), track.clone()));
    out.push(("relabel1".into(), r1));
    out.push(("relabel2".into(), r2));
    out.push(("map_file".into(), mapf.clone()));
    out.push(("read_map_file".into(), api::read_map_file(&mapf).unwrap()));
    out.push(("matches_self".into(), api::matches(fps, dur, fps, dur)));
    out.push(("find_key".into(), api::find_label_key(l2["labels"].as_str().unwrap(), &track).unwrap()));
    out
}

fn file(song: &str, what: &str) -> String {
    format!("{DIR}/{song}.{what}.json")
}

#[cfg(not(target_arch = "wasm32"))]
#[test]
fn native_writes_reference() {
    std::fs::create_dir_all(DIR).unwrap();
    for (name, s) in songs() {
        for (what, json) in outputs(name, &s) {
            std::fs::write(file(name, &what), json).unwrap();
        }
    }
}

#[cfg(target_arch = "wasm32")]
mod wasm {
    use super::*;
    use wasm_bindgen::prelude::*;
    use wasm_bindgen_test::wasm_bindgen_test;

    #[wasm_bindgen(module = "fs")]
    extern "C" {
        #[wasm_bindgen(js_name = readFileSync, catch)]
        fn read_file_sync(path: &str, encoding: &str) -> Result<String, JsValue>;
    }

    #[wasm_bindgen_test]
    fn wasm_matches_native() {
        let mut compared = 0;
        for (name, s) in songs() {
            for (what, json) in outputs(name, &s) {
                let path = file(name, &what);
                let native = read_file_sync(&path, "utf8").unwrap_or_else(|_| {
                    panic!(
                        "{path} is missing: run the native half first (cargo test -p on-the-drop-wasm --test parity)"
                    )
                });
                if native != json {
                    let at = native
                        .bytes()
                        .zip(json.bytes())
                        .position(|(a, b)| a != b)
                        .unwrap_or(native.len().min(json.len()));
                    let lo = at.saturating_sub(80);
                    panic!(
                        "{name}.{what}: wasm differs from native at byte {at}\nnative: {}\nwasm:   {}",
                        &native[lo..(at + 80).min(native.len())],
                        &json[lo..(at + 80).min(json.len())]
                    );
                }
                compared += 1;
            }
        }
        assert!(compared >= 9 * 8, "compared {compared} outputs");
    }
}
