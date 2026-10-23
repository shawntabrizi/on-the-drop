//! WebAssembly binding of on-the-drop for the web app (`web/`). Thin
//! wrapper: all logic lives in the `on-the-drop` crate. Decoding stays in the
//! browser (Web Audio).
//!
//! Every function takes and returns JSON strings, so the web worker and the
//! page can pass the results around and store them as they are. The plain
//! Rust functions ([`api`]) run natively too: the parity test compares their
//! output on native and on wasm32.
//!
//! The analysis JSON ([`api::analyze`]) has the fields of the Godot binding's
//! dictionary (frame-visualizer's `SongMap.to_dict` plus `levels`, `bins`,
//! `bin_s`, `analysis`, `part_bounds`, `fixed_tempo`, `grid_drift`,
//! `bin_raw`, `bin_air`; `finale` is `[start, end]`), and the derived
//! `drops`, `words` (the section words) and `vocabulary` for the editor.

use wasm_bindgen::prelude::*;

pub mod api {
    use on_the_drop::Analysis;
    use on_the_drop::SongMap;
    use on_the_drop::format::{self, MapData};
    use on_the_drop::overrides::{LabelSession, Labels};
    use on_the_drop::track_id::{self, TrackRecord};
    use serde::Serialize;

    /// The map as the app reads it: the map file fields plus the derived
    /// drops and the section words.
    #[derive(Serialize)]
    struct MapView<'a> {
        #[serde(flatten)]
        data: MapData,
        drops: &'a [f32],
        words: Vec<String>,
        vocabulary: Vec<&'static str>,
    }

    #[derive(Serialize)]
    struct AnalysisView<'a> {
        #[serde(flatten)]
        map: MapView<'a>,
        levels: &'a [u8],
        bins: usize,
        bin_s: f64,
        analysis: u32,
        part_bounds: &'a [f32],
        fixed_tempo: bool,
        grid_drift: f64,
        bin_raw: &'a [f32],
        bin_air: &'a [f32],
    }

    fn map_view<'a>(m: &'a SongMap, words: Vec<String>) -> MapView<'a> {
        MapView { data: MapData::from(m), drops: &m.drops, words, vocabulary: m.vocabulary() }
    }

    fn auto_words(m: &SongMap) -> Vec<String> {
        if m.sections.is_empty() { Vec::new() } else { m.section_words(&[]) }
    }

    fn json<T: Serialize>(v: &T) -> String {
        serde_json::to_string(v).expect("serialises")
    }

    fn analysis_json(a: &Analysis) -> String {
        json(&AnalysisView {
            map: map_view(&a.map, auto_words(&a.map)),
            levels: &a.levels,
            bins: a.bins.nb,
            bin_s: a.bins.bin_s,
            analysis: on_the_drop::ANALYSIS_VERSION,
            part_bounds: &a.map.part_bounds,
            fixed_tempo: a.map.fixed_tempo,
            grid_drift: a.map.grid_drift,
            bin_raw: &a.bins.raw,
            bin_air: &a.bins.air,
        })
    }

    /// Analyses `channels` interleaved channels (1 or 2) at `rate` Hz.
    /// `{}` when there is not one full bin, or for a bad rate or channel count.
    pub fn analyze(pcm: &[f32], channels: u32, rate: f64) -> String {
        if !(rate > 0.0 && rate <= 1_000_000.0) {
            return "{}".into();
        }
        let a = match channels {
            1 => on_the_drop::analyze_mono(pcm, rate),
            2 => on_the_drop::analyze_stereo(pcm, rate),
            _ => None,
        };
        a.map_or_else(|| "{}".into(), |a| analysis_json(&a))
    }

    /// The track record (fingerprint) of `channels` interleaved channels at
    /// `rate` Hz. `{}` for a bad rate or channel count.
    pub fn fingerprint(pcm: &[f32], channels: u32, rate: u32) -> String {
        if !(1..=1_000_000).contains(&rate) || channels == 0 {
            return "{}".into();
        }
        json(&TrackRecord::from_pcm(pcm, rate, channels))
    }

    #[derive(Serialize)]
    struct Match {
        matched: bool,
        cover: f64,
        score: f64,
        offset: f64,
    }

    /// Compares two fingerprints (base64) with their durations (s).
    pub fn matches(a: &str, duration_a: f64, b: &str, duration_b: f64) -> String {
        let fa = track_id::decode(a).unwrap_or_default();
        let fb = track_id::decode(b).unwrap_or_default();
        let r = track_id::matches(&fa, duration_a, &fb, duration_b);
        json(&Match { matched: r.matched, cover: r.cover, score: r.score, offset: r.offset })
    }

    fn record(s: &str) -> Result<TrackRecord, String> {
        serde_json::from_str(s).map_err(|e| format!("track record: {e}"))
    }

    fn labels(s: &str) -> Result<Labels, String> {
        if s.trim().is_empty() {
            return Ok(Labels::default());
        }
        format::labels_from_json(s).map_err(|e| e.to_string())
    }

    /// Same recording? (`track_id::same_track`: the fingerprints, else the tags.)
    pub fn same_track(a: &str, b: &str) -> Result<bool, String> {
        Ok(track_id::same_track(&record(a)?, &record(b)?))
    }

    /// The map from an analysis or map JSON (its map fields; others ignored).
    fn read_map(s: &str) -> Result<SongMap, String> {
        let d: MapData = serde_json::from_str(s).map_err(|e| format!("map: {e}"))?;
        d.to_map().ok_or_else(|| "map: arrays do not fit".to_string())
    }

    #[derive(Serialize)]
    struct LabelResult<'a> {
        /// The map with the user's labels applied (drops, strengths, finale).
        map: MapView<'a>,
        /// The labels file content (on-the-drop/labels) after the call.
        labels: String,
        /// The analysis' word of each section.
        auto_words: &'a [String],
        /// Per section: does the user's word override the analysis'?
        user: Vec<bool>,
        /// Did the call change the labels (save them)?
        changed: bool,
        log: &'a [String],
    }

    /// Keeps the track record next to the track's labels.
    fn keep_record(l: &mut Labels, key: &str, track: &str) -> Result<bool, String> {
        if track.trim().is_empty() {
            return Ok(false);
        }
        match l.tracks.get_mut(key) {
            Some(t) if t.track.is_none() => {
                t.track = Some(record(track)?);
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    /// The labels of track `key` applied to the analysis map `auto_map` (the
    /// map as analysed, without labels): `{map, labels, auto_words, user,
    /// changed, log}`. With `section` >= 0, section `section` first gets the
    /// user's `word` ("" or the analysis' word: back to auto). `track` (a
    /// track record JSON, or "") is stored with the labels.
    pub fn relabel(
        auto_map: &str,
        labels_json: &str,
        key: &str,
        file: &str,
        track: &str,
        section: i32,
        word: &str,
    ) -> Result<String, String> {
        let mut m = read_map(auto_map)?;
        let mut l = labels(labels_json)?;
        let mut s = LabelSession::new(&m);
        let mut changed = s.apply(&mut m, &mut l, key, file);
        if section >= 0 {
            changed |= s.set_label(&mut m, &mut l, section as usize, word);
        }
        changed |= keep_record(&mut l, key, track)?;
        let user = (0..s.count()).map(|i| s.is_user(i)).collect();
        Ok(json(&LabelResult {
            map: map_view(&m, s.words.clone()),
            labels: format::labels_to_json(&l),
            auto_words: &s.auto_words,
            user,
            changed,
            log: &s.log,
        }))
    }

    /// [`relabel`] without a change: the stored labels applied.
    pub fn apply_labels(
        auto_map: &str,
        labels_json: &str,
        key: &str,
        file: &str,
        track: &str,
    ) -> Result<String, String> {
        relabel(auto_map, labels_json, key, file, track, -1, "")
    }

    /// The key under which `labels` holds the labels of the recording
    /// `track`: its own key, else the key of an entry whose track record is
    /// the same recording ([`track_id::same_track`]). "" when none.
    pub fn find_label_key(labels_json: &str, track: &str) -> Result<String, String> {
        let l = labels(labels_json)?;
        let r = record(track)?;
        if l.tracks.contains_key(&r.key) {
            return Ok(r.key);
        }
        for (k, t) in &l.tracks {
            if t.track.as_ref().is_some_and(|x| track_id::same_track(x, &r)) {
                return Ok(k.clone());
            }
        }
        Ok(String::new())
    }

    #[derive(Serialize)]
    struct Merge {
        labels: String,
        /// Imported entries that replaced the labels of a known recording.
        merged: usize,
        /// Imported entries of new recordings.
        added: usize,
    }

    /// Merges an imported labels file into `labels_json` by recording: an
    /// imported entry goes to the entry with the same key or the same
    /// recording (its labels replace the old ones; a missing track record
    /// is filled in), else it is added under its own key.
    pub fn merge_labels(labels_json: &str, imported: &str) -> Result<String, String> {
        let mut l = labels(labels_json)?;
        let from = labels(imported)?;
        let (mut merged, mut added) = (0, 0);
        for (k, t) in from.tracks {
            let target = if l.tracks.contains_key(&k) {
                Some(k.clone())
            } else {
                t.track.as_ref().and_then(|r| {
                    l.tracks
                        .iter()
                        .find(|(_, x)| x.track.as_ref().is_some_and(|y| track_id::same_track(y, r)))
                        .map(|(k, _)| k.clone())
                })
            };
            match target {
                Some(key) => {
                    let e = l.tracks.get_mut(&key).expect("key exists");
                    e.labels = t.labels;
                    if e.track.is_none() {
                        e.track = t.track;
                    }
                    if e.file.is_empty() {
                        e.file = t.file;
                    }
                    merged += 1;
                }
                None => {
                    l.tracks.insert(k, t);
                    added += 1;
                }
            }
        }
        Ok(json(&Merge { labels: format::labels_to_json(&l), merged, added }))
    }

    /// A map file (on-the-drop/map) of a map JSON (analysis or labelled map)
    /// with its track record (`track`: a record JSON, or "").
    pub fn map_file(map_json: &str, track: &str) -> Result<String, String> {
        let m = read_map(map_json)?;
        let r = if track.trim().is_empty() { None } else { Some(record(track)?) };
        Ok(format::map_to_json(&m, r.as_ref()))
    }

    #[derive(Serialize)]
    struct MapFileView<'a> {
        map: MapView<'a>,
        track: Option<TrackRecord>,
    }

    /// Reads a map file: `{map, track}`. An error for another format or
    /// analysis version.
    pub fn read_map_file(s: &str) -> Result<String, String> {
        let (m, track) = format::map_from_json(s).map_err(|e| e.to_string())?;
        Ok(json(&MapFileView { map: map_view(&m, auto_words(&m)), track }))
    }

    /// A labels file (on-the-drop/labels, also frame-visualizer's old file)
    /// read and written back in the current form.
    pub fn normalize_labels(s: &str) -> Result<String, String> {
        Ok(format::labels_to_json(&labels(s)?))
    }
}

fn js<T>(r: Result<T, String>) -> Result<T, JsError> {
    r.map_err(|e| JsError::new(&e))
}

/// Version of the Rust core library.
#[wasm_bindgen]
pub fn version() -> String {
    on_the_drop::version().into()
}

/// The analysis version (map files of another version are refused).
#[wasm_bindgen(js_name = analysisVersion)]
pub fn analysis_version() -> u32 {
    on_the_drop::ANALYSIS_VERSION
}

#[wasm_bindgen(js_name = fingerprintVersion)]
pub fn fingerprint_version() -> String {
    on_the_drop::track_id::FINGERPRINT_VERSION.into()
}

/// See [`api::analyze`].
#[wasm_bindgen]
pub fn analyze(pcm: &[f32], channels: u32, rate: f64) -> String {
    api::analyze(pcm, channels, rate)
}

/// See [`api::fingerprint`].
#[wasm_bindgen]
pub fn fingerprint(pcm: &[f32], channels: u32, rate: u32) -> String {
    api::fingerprint(pcm, channels, rate)
}

/// See [`api::matches`].
#[wasm_bindgen]
pub fn matches(a: &str, duration_a: f64, b: &str, duration_b: f64) -> String {
    api::matches(a, duration_a, b, duration_b)
}

#[wasm_bindgen(js_name = sameTrack)]
pub fn same_track(a: &str, b: &str) -> Result<bool, JsError> {
    js(api::same_track(a, b))
}

/// See [`api::apply_labels`].
#[wasm_bindgen(js_name = applyLabels)]
pub fn apply_labels(auto_map: &str, labels: &str, key: &str, file: &str, track: &str) -> Result<String, JsError> {
    js(api::apply_labels(auto_map, labels, key, file, track))
}

/// See [`api::relabel`].
#[wasm_bindgen]
pub fn relabel(
    auto_map: &str,
    labels: &str,
    key: &str,
    file: &str,
    track: &str,
    section: i32,
    word: &str,
) -> Result<String, JsError> {
    js(api::relabel(auto_map, labels, key, file, track, section, word))
}

#[wasm_bindgen(js_name = findLabelKey)]
pub fn find_label_key(labels: &str, track: &str) -> Result<String, JsError> {
    js(api::find_label_key(labels, track))
}

#[wasm_bindgen(js_name = mergeLabels)]
pub fn merge_labels(labels: &str, imported: &str) -> Result<String, JsError> {
    js(api::merge_labels(labels, imported))
}

#[wasm_bindgen(js_name = mapFile)]
pub fn map_file(map: &str, track: &str) -> Result<String, JsError> {
    js(api::map_file(map, track))
}

#[wasm_bindgen(js_name = readMapFile)]
pub fn read_map_file(s: &str) -> Result<String, JsError> {
    js(api::read_map_file(s))
}

#[wasm_bindgen(js_name = normalizeLabels)]
pub fn normalize_labels(s: &str) -> Result<String, JsError> {
    js(api::normalize_labels(s))
}
