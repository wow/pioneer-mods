//! A screen's parts (`parts.<name>`): the buttons, labels and displays it shows, where each is
//! touched and drawn, what touching it does, the images it draws and the tables it reads. The
//! map skins, layouts and features are planned from: a part's images are art a recipe may
//! repaint (a skin, through a slot that names them), its touch zone is a layout record, and what
//! it does is code or a table.
//!
//! These are the player-agnostic rules; the catalog holds a part to its player (screen, pixel
//! format, touch layout shape) and the loader to its release.

use super::acceptance::{Address, Region};
use super::entry::{EntryError, check_name, check_text};
use super::screen_images::Position;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// One part of a screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    /// What the screen shows on it (`REV`, `HOT CUE A`), or what it is (`phrase bars`).
    pub title: String,
    pub kind: PartKind,
    /// The zones whose touch acts on it, one per layout record.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub touch: Vec<TouchZone>,
    /// Where it is drawn and from which images, when measured: one placement per position.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub drawn: Vec<Drawn>,
    /// The slot that names its images, if one does (`beat_loop.pad`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slot: Option<String>,
    /// Tables of the application it reads (the BEAT LOOP lengths), with their stock values.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tables: Vec<PartTable>,
    /// What touching it does, or what it shows.
    pub does: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PartKind {
    Button,
    /// A row of buttons that one zone covers (the BEAT LOOP pads).
    Buttons,
    Label,
    Display,
    Frame,
}

/// A touch zone: a record of the application's touch layout (its shape: the player's
/// `touch_layout`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchZone {
    /// The record's decoded-application offset.
    pub record: Address,
    /// The area the record holds.
    pub area: Region,
    /// The handler the record names, a run-time address (from the release's load address).
    pub handler: Address,
    /// When the zone acts, if not whenever the screen is shown (`while a loop plays`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub when: Option<String>,
}

/// One place a part is drawn and the images it was seen drawing there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Drawn {
    /// Where its images' top-left pixel is drawn.
    pub at: Position,
    pub images: Vec<PartImage>,
}

/// One image a part draws, in the player's pixel format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartImage {
    /// Its decoded-application offset.
    pub offset: Address,
    pub width: u32,
    pub height: u32,
    /// When it was first seen drawn (`after touching QUANTIZE`).
    pub seen: String,
}

impl PartImage {
    /// The screen area it covers when drawn at `at`.
    pub fn area(&self, at: Position) -> Region {
        Region {
            x: at.x,
            y: at.y,
            w: self.width,
            h: self.height,
        }
    }
}

/// A table of the application a part reads: one entry per value, `entry_bytes` each
/// (little-endian), `stride` bytes apart, from `offset`. Its values are the stock ones.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PartTable {
    /// The first entry's decoded-application offset.
    pub offset: Address,
    pub entry_bytes: u32,
    pub stride: u32,
    pub values: Vec<u32>,
    /// What the entries are (`loop length indices, pads 1 to 6`).
    pub what: String,
}

impl PartTable {
    /// The decoded-application range of each entry, whose stock value the table publishes.
    pub fn entries(&self) -> impl Iterator<Item = std::ops::Range<u64>> + '_ {
        (0..self.values.len() as u64).map(|index| {
            let start = u64::from(self.offset.0) + index * u64::from(self.stride);
            start..start + u64::from(self.entry_bytes)
        })
    }
}

impl Part {
    /// Static checks of part `name` (its field path is `parts.<name>`).
    pub(super) fn validate(&self, name: &str) -> Result<(), EntryError> {
        let path = format!("parts.{name}");
        let rule = |field: String, problem: &str| EntryError::Rule {
            field,
            problem: problem.to_owned(),
        };
        check_name("parts", name)?;
        check_text(&format!("{path}.title"), &self.title)?;
        check_text(&format!("{path}.does"), &self.does)?;
        check_text(&format!("{path}.evidence"), &self.evidence)?;
        if self.touch.is_empty() && self.drawn.is_empty() && self.slot.is_none() {
            return Err(rule(path, "a part is touched, drawn or names a slot"));
        }
        let mut zones = BTreeSet::new();
        for (index, zone) in self.touch.iter().enumerate() {
            let field = format!("{path}.touch[{index}]");
            if zone.area.w == 0 || zone.area.h == 0 {
                return Err(rule(format!("{field}.area"), "a touch area is not empty"));
            }
            if let Some(when) = &zone.when {
                check_text(&format!("{field}.when"), when)?;
            }
            let key = (
                zone.area.x,
                zone.area.y,
                zone.area.w,
                zone.area.h,
                &zone.when,
            );
            if !zones.insert(key) {
                return Err(rule(
                    field,
                    "another zone of the part has this area and `when`",
                ));
            }
        }
        let mut places = BTreeSet::new();
        for (place, drawn) in self.drawn.iter().enumerate() {
            let field = format!("{path}.drawn[{place}]");
            if !places.insert((drawn.at.x, drawn.at.y)) {
                return Err(rule(
                    field,
                    "another placement of the part is at this position",
                ));
            }
            if drawn.images.is_empty() {
                return Err(rule(
                    format!("{field}.images"),
                    "a placement names an image",
                ));
            }
            let mut offsets = BTreeSet::new();
            for (index, image) in drawn.images.iter().enumerate() {
                let field = format!("{field}.images[{index}]");
                if image.width == 0 || image.height == 0 {
                    return Err(rule(field, "an image has pixels"));
                }
                if !offsets.insert(image.offset) {
                    return Err(rule(field, "the placement names this image already"));
                }
                check_text(&format!("{field}.seen"), &image.seen)?;
            }
        }
        if let Some(slot) = &self.slot {
            check_name(&format!("{path}.slot"), slot)?;
        }
        for (index, table) in self.tables.iter().enumerate() {
            let field = format!("{path}.tables[{index}]");
            let bytes = table.entry_bytes;
            if !matches!(bytes, 1 | 2 | 4) || table.stride < bytes {
                return Err(rule(
                    field,
                    "an entry is 1, 2 or 4 bytes, and `stride` is at least that",
                ));
            }
            if table.values.is_empty() {
                return Err(rule(format!("{field}.values"), "a table has an entry"));
            }
            if table
                .values
                .iter()
                .any(|&v| bytes < 4 && v >> (8 * bytes) != 0)
            {
                return Err(rule(
                    format!("{field}.values"),
                    "a value fits in `entry_bytes`",
                ));
            }
            check_text(&format!("{field}.what"), &table.what)?;
        }
        Ok(())
    }
}
