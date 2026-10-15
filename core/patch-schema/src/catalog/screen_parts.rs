//! A screen's parts (`parts.<name>`): the buttons, labels and displays it shows, where each is
//! touched and drawn, what touching it does, and the images it draws. The map skins, layouts and
//! features are planned from: a part's images are art a skin may repaint, its touch zone is a
//! layout record, and what it does is code.

use super::acceptance::{Address, Region};
use super::entry::{EntryError, check_name, check_text};
use super::screen_images::Position;
use crate::image::{MAX_IMAGE_BYTES, image_bytes};
use serde::{Deserialize, Serialize};

/// The application's run-time base: a touch handler's address is at least this.
pub const RUN_TIME_BASE: u32 = 0x0800_0000;
/// The size of one touch layout record ({x, y, width, height, handler}, five 32-bit words).
pub const TOUCH_RECORD_LEN: u32 = 20;

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

/// A touch zone: a record of the screen's touch layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchZone {
    /// The record's decoded-application offset ({x, y, width, height, handler}, 20 bytes).
    pub record: Address,
    /// The area the record holds.
    pub area: Region,
    /// The handler the record names, a run-time address.
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

/// One image a part draws (RGB565, 16-bit little-endian).
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
    /// The image's length in bytes (`u64::MAX` if it overflows, which validation refuses).
    pub fn len(&self) -> u64 {
        image_bytes(self.width, self.height, 2).unwrap_or(u64::MAX)
    }

    /// Whether the image has no pixel (validation refuses it).
    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0
    }

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
        for (index, zone) in self.touch.iter().enumerate() {
            let field = format!("{path}.touch[{index}]");
            if zone.record.0 % 4 != 0 {
                return Err(rule(
                    format!("{field}.record"),
                    "a layout record is 4-byte aligned",
                ));
            }
            if zone.handler.0 < RUN_TIME_BASE || zone.handler.0 % 2 != 0 {
                return Err(rule(
                    format!("{field}.handler"),
                    "a handler is an even run-time address in the application (0x08000000 on)",
                ));
            }
            if zone.area.w == 0 || zone.area.h == 0 {
                return Err(rule(format!("{field}.area"), "a touch area is not empty"));
            }
            if let Some(when) = &zone.when {
                check_text(&format!("{field}.when"), when)?;
            }
        }
        for (place, drawn) in self.drawn.iter().enumerate() {
            if drawn.images.is_empty() {
                return Err(rule(
                    format!("{path}.drawn[{place}].images"),
                    "a placement names an image",
                ));
            }
            for (index, image) in drawn.images.iter().enumerate() {
                let field = format!("{path}.drawn[{place}].images[{index}]");
                if image.offset.0 % 2 != 0 {
                    return Err(rule(
                        format!("{field}.offset"),
                        "an image is 2-byte aligned",
                    ));
                }
                if image.is_empty() || image.len() > MAX_IMAGE_BYTES {
                    return Err(rule(field, "an image has 1 byte to 1 MiB of pixels"));
                }
                check_text(&format!("{field}.seen"), &image.seen)?;
            }
        }
        if let Some(slot) = &self.slot {
            check_name(&format!("{path}.slot"), slot)?;
        }
        Ok(())
    }
}
