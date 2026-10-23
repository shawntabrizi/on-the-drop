//! User section labels (port of frame-visualizer `lib/section_overrides.gd`,
//! `SectionOverrides`): the user corrects a section's label; the labels are a
//! layer applied after the analysis, through one hook
//! ([`SongMap::set_section_ids`]), so drops, drop strengths and the finale
//! follow the user's label. A drop the user makes has strength 1.0; one the
//! user removes loses its strength.
//!
//! Each label is anchored to its section's start and end time, not its
//! index: after a new analysis it goes to the section whose span overlaps it
//! best by more than [`MATCH`] (overlap over the longer span) and takes that
//! section's times. A label that matches no section but covers more than
//! MATCH of each of several sections (a section now split at its part
//! boundaries) goes to each of those pieces. A label that matches nothing is
//! dropped (logged).
//!
//! File IO stays with the caller: [`Labels`] is the file content (JSON, see
//! [`crate::format`]); a method that changes it returns `true` and the
//! caller saves.

use crate::gd::{self, maxf, minf};
use crate::map::SongMap;
use crate::sections::{BREAKDOWN, BUILD, CALM, DROP};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const MATCH: f64 = 0.5;

/// Section id of each word (both vocabularies, [`SongMap::vocabulary`]).
pub fn word_id(word: &str) -> Option<i32> {
    Some(match word {
        "Intro" | "Outro" | "Low" => CALM,
        "Build" | "Rising" => BUILD,
        "Drop" | "Peak" => DROP,
        "Breakdown" | "Dip" => BREAKDOWN,
        _ => return None,
    })
}

/// One stored label: the section's span when it was set, the analysis' word
/// then, and the user's word.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Label {
    pub start: f64,
    pub end: f64,
    pub auto: String,
    pub label: String,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct TrackLabels {
    /// The track's file name (for the ground truth only).
    #[serde(default)]
    pub file: String,
    pub labels: Vec<Label>,
    /// The track's identity (fingerprint), so another key of the same
    /// recording finds these labels ([`crate::track_id::same_track`]).
    /// Absent in frame-visualizer's old file-hash entries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track: Option<crate::track_id::TrackRecord>,
}

/// All stored labels: track key -> labels.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Labels {
    pub tracks: BTreeMap<String, TrackLabels>,
}

impl Labels {
    /// Every label as ground truth (TSV: track key, file, start, end, auto
    /// word, user word), with a header line.
    pub fn truth_tsv(&self) -> String {
        let mut s = String::from("track_key\tfile\tstart\tend\tauto\tuser\n");
        for (k, t) in &self.tracks {
            for e in &t.labels {
                s += &format!("{}\t{}\t{:.3}\t{:.3}\t{}\t{}\n", k, t.file, e.start, e.end, e.auto, e.label);
            }
        }
        s
    }
}

/// Overlap of two spans over the longer one (0..1); spans are f32 pairs as
/// the GDScript Vector2.
pub fn overlap(a: (f32, f32), b: (f32, f32)) -> f64 {
    let (ax, ay, bx, by) = (a.0 as f64, a.1 as f64, b.0 as f64, b.1 as f64);
    let o = minf(ay, by) - maxf(ax, bx);
    maxf(o, 0.0) / maxf(maxf(ay - ax, by - bx), 0.001)
}

/// The labels of one track applied to its map (the GDScript object's state
/// for the current overview). Make one per analysed map with [`Self::new`];
/// it records the analysis' own ids, words and drop strengths.
#[derive(Clone, Debug, Default)]
pub struct LabelSession {
    pub key: String,
    pub file: String,
    /// The analysis' section ids, words and drop time -> strength.
    pub auto_ids: Vec<i32>,
    pub auto_words: Vec<String>,
    pub auto_strength: Vec<(f32, f32)>,
    /// Per section: the user's word, "" = auto.
    pub user_words: Vec<String>,
    /// Per section: the word shown (user's, else auto).
    pub words: Vec<String>,
    /// What the last calls did (re-anchored, dropped, split), for a log.
    pub log: Vec<String>,
}

impl LabelSession {
    pub fn new(map: &SongMap) -> Self {
        let auto_ids = map.sections.iter().skip(1).step_by(2).map(|&x| x as i32).collect();
        let auto_words = if map.sections.is_empty() { Vec::new() } else { map.section_words(&[]) };
        let n = map.drops.len().min(map.drop_strength.len());
        let auto_strength = (0..n).map(|i| (map.drops[i], map.drop_strength[i])).collect();
        LabelSession { auto_ids, auto_words, auto_strength, ..Default::default() }
    }

    pub fn count(&self) -> usize {
        self.auto_ids.len()
    }

    /// Start and end (s) of section i of the map (f32, as a GDScript Vector2).
    pub fn span(map: &SongMap, i: usize) -> (f32, f32) {
        let s = &map.sections;
        let end = if i * 2 + 2 < s.len() { s[i * 2 + 2] } else { map.duration as f32 };
        (s[i * 2], end)
    }

    pub fn is_user(&self, i: usize) -> bool {
        i < self.user_words.len() && !self.user_words[i].is_empty()
    }

    /// Applies the stored labels of track `key` to `map`. Returns true when
    /// `labels` changed (re-anchored or dropped labels): save it.
    pub fn apply(&mut self, map: &mut SongMap, labels: &mut Labels, key: &str, file: &str) -> bool {
        self.key = key.to_string();
        self.file = file.to_string();
        let changed = self.match_labels(map, labels);
        if changed {
            self.store(map, labels);
        }
        self.push(map);
        changed
    }

    /// The user's word for section i ("" or the auto word: back to auto).
    /// Returns true when `labels` changed (always for a valid i and a known
    /// word; an unknown word changes nothing): save it.
    pub fn set_label(&mut self, map: &mut SongMap, labels: &mut Labels, i: usize, word: &str) -> bool {
        if i >= self.count() || (!word.is_empty() && word != self.auto_words[i] && word_id(word).is_none()) {
            return false;
        }
        self.user_words[i] = if word == self.auto_words[i] { String::new() } else { word.to_string() };
        self.store(map, labels);
        self.push(map);
        true
    }

    fn match_labels(&mut self, map: &SongMap, labels: &Labels) -> bool {
        self.user_words = vec![String::new(); self.count()];
        let Some(t) = labels.tracks.get(&self.key) else {
            return false;
        };
        let mut by_section: BTreeMap<usize, String> = BTreeMap::new();
        let mut changed = false;
        let key = self.key.clone();
        for e in &t.labels {
            let es = (e.start as f32, e.end as f32);
            let mut best: i64 = -1;
            let mut best_r = MATCH;
            for i in 0..self.count() {
                let r = overlap(es, Self::span(map, i));
                if r > best_r {
                    best_r = r;
                    best = i as i64;
                }
            }
            let mut pieces: Vec<usize> = Vec::new();
            if best < 0 {
                for i in 0..self.count() {
                    let sp = Self::span(map, i);
                    let len = sp.1 as f64 - sp.0 as f64;
                    if overlap(es, sp) * maxf(e.end - e.start, len) > MATCH * len {
                        pieces.push(i);
                    }
                }
            }
            if pieces.len() >= 2 && word_id(&e.label).is_some() {
                self.log.push(format!(
                    "{} {:.1}-{:.1} s now covers {} sections; each keeps the label",
                    key, e.start, e.end, pieces.len()
                ));
                for i in pieces {
                    by_section.entry(i).or_insert_with(|| e.label.clone());
                }
                changed = true;
                continue;
            }
            if best < 0 || word_id(&e.label).is_none() {
                self.log.push(format!(
                    "dropped {} {:.1}-{:.1} s ({} -> {}): it matches no section now",
                    key, e.start, e.end, e.auto, e.label
                ));
                changed = true;
                continue;
            }
            let best = best as usize;
            if by_section.contains_key(&best) {
                self.log.push(format!("{key} section {best} has two labels; the newer one stays"));
                changed = true;
            }
            by_section.insert(best, e.label.clone());
            let sp = Self::span(map, best);
            if !gd::is_equal_approx(e.start, sp.0 as f64) || !gd::is_equal_approx(e.end, sp.1 as f64) {
                self.log.push(format!(
                    "{} {:.1}-{:.1} s re-anchored to {:.1}-{:.1} s",
                    key, e.start, e.end, sp.0, sp.1
                ));
                changed = true;
            }
        }
        for (i, w) in by_section {
            self.user_words[i] = w;
        }
        changed
    }

    /// user_words -> `labels` (anchored to the current spans).
    fn store(&self, map: &SongMap, labels: &mut Labels) {
        let mut out = Vec::new();
        for i in 0..self.count() {
            if !self.user_words[i].is_empty() {
                let sp = Self::span(map, i);
                out.push(Label {
                    start: sp.0 as f64,
                    end: sp.1 as f64,
                    auto: self.auto_words[i].clone(),
                    label: self.user_words[i].clone(),
                });
            }
        }
        if out.is_empty() {
            labels.tracks.remove(&self.key);
        } else {
            let track = labels.tracks.get(&self.key).and_then(|t| t.track.clone());
            labels.tracks.insert(self.key.clone(), TrackLabels { file: self.file.clone(), labels: out, track });
        }
    }

    /// user_words -> the map's ids (set_section_ids) and `words`.
    fn push(&mut self, map: &mut SongMap) {
        let mut ids = self.auto_ids.clone();
        self.words = self.auto_words.clone();
        for i in 0..self.count() {
            if !self.user_words[i].is_empty() {
                ids[i] = word_id(&self.user_words[i]).expect("stored words are known");
                self.words[i] = self.user_words[i].clone();
            }
        }
        map.set_section_ids(&ids, &self.auto_strength);
    }
}
