//! JSON file formats (serde), each with a format name and version.
//!
//! Map file (`on-the-drop/map`): the analysis of one track.
//! ```json
//! {"format": "on-the-drop/map", "version": 1, "analysis": 11,
//!  "track": {TrackRecord, optional},
//!  "map": {"duration", "bpm", "beats", "beat_bar", "beat_support", "phrases",
//!          "sections", "finale": [start, end], "energy", "kicks", "snares",
//!          "hats", "kick_amp", "snare_amp", "hat_amp", "character",
//!          "kick_cover", "four_floor", "drop_step", "drop_strength"}}
//! ```
//! The map fields are frame-visualizer's `SongMap.to_dict` (the cache), so
//! the scenes get the same map from a file as from a fresh analysis;
//! `analysis` is [`crate::ANALYSIS_VERSION`]: a reader rebuilds a map of an
//! older analysis. Times are f32 seconds (written in the shortest form that
//! reads back to the same f32).
//!
//! Labels file (`on-the-drop/labels`): the user's section labels.
//! ```json
//! {"format": "on-the-drop/labels", "version": 1,
//!  "tracks": {<track key>: {"file": name, "labels": [{"start", "end", "auto", "label"}]}}}
//! ```
//! The same content as frame-visualizer's `user://section_labels.json`
//! (which has no "format" key; it reads as version 1).

use crate::map::SongMap;
use crate::overrides::Labels;
use crate::track_id::TrackRecord;
use serde::{Deserialize, Serialize};

pub const MAP_FORMAT: &str = "on-the-drop/map";
pub const MAP_FORMAT_VERSION: u32 = 1;
pub const LABELS_FORMAT: &str = "on-the-drop/labels";
pub const LABELS_FORMAT_VERSION: u32 = 1;

/// The cached map fields (`SongMap.to_dict`).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapData {
    pub duration: f64,
    pub bpm: f64,
    pub beats: Vec<f32>,
    pub beat_bar: Vec<u8>,
    pub beat_support: Vec<u8>,
    pub phrases: Vec<f32>,
    pub sections: Vec<f32>,
    pub finale: [f32; 2],
    pub energy: Vec<u8>,
    pub kicks: Vec<f32>,
    pub snares: Vec<f32>,
    pub hats: Vec<f32>,
    pub kick_amp: Vec<u8>,
    pub snare_amp: Vec<u8>,
    pub hat_amp: Vec<u8>,
    pub character: i32,
    pub kick_cover: f64,
    pub four_floor: f64,
    pub drop_step: f64,
    pub drop_strength: Vec<f32>,
}

impl From<&SongMap> for MapData {
    fn from(m: &SongMap) -> Self {
        MapData {
            duration: m.duration,
            bpm: m.bpm,
            beats: m.beats.clone(),
            beat_bar: m.beat_bar.clone(),
            beat_support: m.beat_support.clone(),
            phrases: m.phrases.clone(),
            sections: m.sections.clone(),
            // the cache holds the finale as a Vector2 (f32)
            finale: [m.finale_start as f32, m.finale_end as f32],
            energy: m.energy.clone(),
            kicks: m.kicks.clone(),
            snares: m.snares.clone(),
            hats: m.hats.clone(),
            kick_amp: m.kick_amp.clone(),
            snare_amp: m.snare_amp.clone(),
            hat_amp: m.hat_amp.clone(),
            character: m.character,
            kick_cover: m.kick_cover,
            four_floor: m.four_floor,
            drop_step: m.drop_step,
            drop_strength: m.drop_strength.clone(),
        }
    }
}

impl MapData {
    /// The map back (`SongMap.from_dict`): downbeats, section starts and
    /// drops are rebuilt. None when the arrays do not fit together.
    pub fn to_map(&self) -> Option<SongMap> {
        if self.beat_bar.len() != self.beats.len() || self.sections.len() % 2 != 0 {
            return None;
        }
        let mut m = SongMap::new();
        m.duration = self.duration;
        m.bpm = self.bpm;
        m.beats = self.beats.clone();
        m.beat_bar = self.beat_bar.clone();
        m.beat_support = self.beat_support.clone();
        m.phrases = self.phrases.clone();
        m.sections = self.sections.clone();
        m.finale_start = self.finale[0] as f64;
        m.finale_end = self.finale[1] as f64;
        m.energy = self.energy.clone();
        m.kicks = self.kicks.clone();
        m.snares = self.snares.clone();
        m.hats = self.hats.clone();
        m.kick_amp = self.kick_amp.clone();
        m.snare_amp = self.snare_amp.clone();
        m.hat_amp = self.hat_amp.clone();
        m.character = self.character;
        m.kick_cover = self.kick_cover;
        m.four_floor = self.four_floor;
        m.drop_step = self.drop_step;
        m.drop_strength = self.drop_strength.clone();
        m.derive();
        Some(m)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct MapFile {
    pub format: String,
    pub version: u32,
    pub analysis: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<TrackRecord>,
    pub map: MapData,
}

#[derive(Debug)]
pub enum FormatError {
    Json(serde_json::Error),
    /// Another format, a newer format version or an older analysis.
    Version(String),
    Invalid,
}

impl std::fmt::Display for FormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FormatError::Json(e) => write!(f, "json: {e}"),
            FormatError::Version(s) => write!(f, "version: {s}"),
            FormatError::Invalid => write!(f, "invalid map"),
        }
    }
}

impl std::error::Error for FormatError {}

pub fn map_to_json(m: &SongMap, track: Option<&TrackRecord>) -> String {
    let f = MapFile {
        format: MAP_FORMAT.into(),
        version: MAP_FORMAT_VERSION,
        analysis: crate::ANALYSIS_VERSION,
        track: track.cloned(),
        map: m.into(),
    };
    serde_json::to_string(&f).expect("map serialises")
}

/// Reads a map file. An older analysis version is an error (re-analyse).
pub fn map_from_json(s: &str) -> Result<(SongMap, Option<TrackRecord>), FormatError> {
    let f: MapFile = serde_json::from_str(s).map_err(FormatError::Json)?;
    if f.format != MAP_FORMAT || f.version > MAP_FORMAT_VERSION {
        return Err(FormatError::Version(format!("{} v{}", f.format, f.version)));
    }
    if f.analysis != crate::ANALYSIS_VERSION {
        return Err(FormatError::Version(format!("analysis {} (want {})", f.analysis, crate::ANALYSIS_VERSION)));
    }
    let m = f.map.to_map().ok_or(FormatError::Invalid)?;
    Ok((m, f.track))
}

#[derive(Serialize, Deserialize)]
struct LabelsFile {
    #[serde(default)]
    format: Option<String>,
    #[serde(default = "one")]
    version: u32,
    tracks: std::collections::BTreeMap<String, crate::overrides::TrackLabels>,
}

fn one() -> u32 {
    1
}

pub fn labels_to_json(l: &Labels) -> String {
    let f = LabelsFile { format: Some(LABELS_FORMAT.into()), version: LABELS_FORMAT_VERSION, tracks: l.tracks.clone() };
    serde_json::to_string_pretty(&f).expect("labels serialise")
}

/// Reads a labels file (also frame-visualizer's, without "format").
pub fn labels_from_json(s: &str) -> Result<Labels, FormatError> {
    let f: LabelsFile = serde_json::from_str(s).map_err(FormatError::Json)?;
    if f.format.as_deref().is_some_and(|x| x != LABELS_FORMAT) || f.version > LABELS_FORMAT_VERSION {
        return Err(FormatError::Version(format!("{:?} v{}", f.format, f.version)));
    }
    Ok(Labels { tracks: f.tracks })
}
