//! A player: one model and firmware release, its pins, screen class, budgets, capabilities and
//! screens (`catalog/players/<id>.json`).

use super::acceptance::Address;
use super::entry::{
    CatalogEntry, EntryError, check_doc_path, check_id, check_name, check_player_id,
    check_schema_version, check_sha256, check_text, check_unique, unique_keys,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Player {
    pub schema_version: u32,
    /// The release id recipes name in `target.release` (`xdj700-v1.15`).
    pub id: String,
    pub model: String,
    pub firmware: Firmware,
    pub screen_class: ScreenClass,
    /// The shape of the application's touch layout records, which screen parts name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub touch_layout: Option<TouchLayout>,
    pub budgets: Budgets,
    /// Named facts about the stock firmware ([`Capability`]).
    #[serde(default, deserialize_with = "unique_keys")]
    pub capabilities: BTreeMap<String, Capability>,
    /// The screens catalogued for this player, each in `catalog/screens/<player>/<screen>.json`.
    pub screens: Vec<String>,
    /// The panel controls acceptance tests may press, by name (`usb`, `rotary_push`).
    #[serde(default, deserialize_with = "unique_keys")]
    pub controls: BTreeMap<String, Control>,
    /// The guest physical address ranges acceptance tests may read: run-time state (variables
    /// the application sets while it runs), never the loaded application. None: no memory check
    /// for this player yet.
    #[serde(default)]
    pub state_memory: Vec<StateRange>,
    pub restore: Restore,
    pub maintainer: String,
}

/// The official update the owner supplies, by its pins (the same as its recipes' `target`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Firmware {
    /// The file name the update must have on the stick (`XDJ700.UPD`).
    pub file: String,
    pub upd_sha256: String,
    pub application_sha256: String,
}

/// A panel control: the code and value the application receives from the panel, as the emulator
/// delivers them (not checked against the hardware panel), each `0x` and two hexadecimal digits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Control {
    pub code: String,
    pub value: String,
    pub description: String,
}

impl Control {
    /// The code and value as bytes, if well formed.
    pub fn bytes(&self) -> Option<(u8, u8)> {
        Some((hex_byte(&self.code)?, hex_byte(&self.value)?))
    }
}

/// A range of run-time state an acceptance test may read, `start` to `end` exclusive, and the
/// `evidence` that it holds run-time state only.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateRange {
    pub start: Address,
    pub end: Address,
    pub evidence: String,
}

impl StateRange {
    /// Whether the `bytes` bytes at `address` lie within the range.
    pub fn holds(&self, address: u32, bytes: u8) -> bool {
        let last = u64::from(address) + u64::from(bytes);
        self.start.0 <= address && last <= u64::from(self.end.0)
    }
}

/// `0x` and two hexadecimal digits.
fn hex_byte(value: &str) -> Option<u8> {
    let digits = value.strip_prefix("0x")?;
    if digits.len() != 2 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u8::from_str_radix(digits, 16).ok()
}

/// How an owner brings back the stock application after flashing a build of this player; a
/// builder prints it with every build.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Restore {
    /// What the official update does over a build, and why.
    pub official: String,
    /// The backup when the official update is not enough.
    pub backup: String,
    /// The player's flashing guide, relative to the repository root (`docs/<file>.md`).
    pub guide: String,
}

/// The shape of a touch layout record ({x, y, width, height, handler} on the XDJ-700): its size,
/// its alignment in the application, and its handler's alignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TouchLayout {
    pub record_bytes: u32,
    pub record_alignment: u32,
    pub handler_alignment: u32,
    pub evidence: String,
}

/// The display a skin is drawn for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ScreenClass {
    pub width: u32,
    pub height: u32,
    pub pixels: PixelFormat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PixelFormat {
    Rgb565,
}

impl PixelFormat {
    /// The bytes one pixel takes.
    pub fn bytes_per_pixel(self) -> u32 {
        match self {
            PixelFormat::Rgb565 => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budgets {
    /// How many bytes the compressed MAIN image may grow by (the engine's bound for the release).
    pub compressed_main_growth_bytes: u64,
}

/// A named fact about the stock firmware: the values established for it (`values`), or why it is
/// unusable (`unavailable`, for example "absent on this model"), and where that is established.
/// `values` list what the evidence shows, not necessarily everything the firmware holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Capability {
    #[serde(default)]
    pub values: Vec<String>,
    #[serde(default)]
    pub unavailable: Option<String>,
    pub evidence: String,
}

impl Capability {
    /// Whether the capability is available and holds every one of `values`.
    pub fn includes(&self, values: &[String]) -> bool {
        self.unavailable.is_none() && values.iter().all(|value| self.values.contains(value))
    }
}

impl CatalogEntry for Player {
    const KIND: &'static str = "player";

    fn id(&self) -> &str {
        &self.id
    }

    fn validate(&self) -> Result<(), EntryError> {
        check_schema_version(self.schema_version)?;
        check_player_id("id", &self.id)?;
        check_text("model", &self.model)?;
        let file_ok = !self.firmware.file.is_empty()
            && self.firmware.file.len() <= 64
            && self
                .firmware
                .file
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '.' || c == '_' || c == '-');
        if !file_ok {
            return Err(EntryError::Rule {
                field: "firmware.file".to_owned(),
                problem: "a plain file name of letters, digits, '.', '_' and '-'".to_owned(),
            });
        }
        check_sha256("firmware.upd_sha256", &self.firmware.upd_sha256)?;
        check_sha256(
            "firmware.application_sha256",
            &self.firmware.application_sha256,
        )?;
        let mut events = BTreeMap::new();
        for (name, control) in &self.controls {
            check_name("controls", name)?;
            let Some(event) = control.bytes() else {
                return Err(EntryError::Rule {
                    field: format!("controls.{name}"),
                    problem: "code and value: 0x and two hexadecimal digits each".to_owned(),
                });
            };
            if let Some(other) = events.insert(event, name) {
                return Err(EntryError::Rule {
                    field: format!("controls.{name}"),
                    problem: format!(
                        "the same code and value as {other}: a test pressing one would press both"
                    ),
                });
            }
            check_text(
                &format!("controls.{name}.description"),
                &control.description,
            )?;
        }
        for (index, range) in self.state_memory.iter().enumerate() {
            let field = format!("state_memory[{index}]");
            let rule = |problem: &str| EntryError::Rule {
                field: field.clone(),
                problem: problem.to_owned(),
            };
            if range.start >= range.end {
                return Err(rule("start below end"));
            }
            // Ranges that overlap or touch are written as one, so a read is in one range or none.
            let earlier = &self.state_memory[..index];
            if earlier
                .iter()
                .any(|other| range.start <= other.end && other.start <= range.end)
            {
                return Err(rule("overlaps or touches another range; write them as one"));
            }
            check_text(&format!("{field}.evidence"), &range.evidence)?;
        }
        check_text("restore.official", &self.restore.official)?;
        check_text("restore.backup", &self.restore.backup)?;
        check_doc_path("restore.guide", &self.restore.guide)?;
        if let Some(layout) = &self.touch_layout {
            let aligned = |a: u32| a.is_power_of_two();
            if layout.record_bytes == 0
                || !aligned(layout.record_alignment)
                || !aligned(layout.handler_alignment)
            {
                return Err(EntryError::Rule {
                    field: "touch_layout".to_owned(),
                    problem: "a record has bytes, and alignments are powers of two".to_owned(),
                });
            }
            check_text("touch_layout.evidence", &layout.evidence)?;
        }
        if self.screen_class.width == 0 || self.screen_class.height == 0 {
            return Err(EntryError::Rule {
                field: "screen_class".to_owned(),
                problem: "width and height must be positive".to_owned(),
            });
        }
        for (name, capability) in &self.capabilities {
            let field = format!("capabilities.{name}");
            check_name("capabilities", name)?;
            check_text(&format!("{field}.evidence"), &capability.evidence)?;
            match (&capability.unavailable, capability.values.is_empty()) {
                (None, false) => {
                    for value in &capability.values {
                        check_text(&format!("{field}.values"), value)?;
                    }
                    check_unique(&format!("{field}.values"), &capability.values)?;
                }
                (Some(reason), true) => check_text(&format!("{field}.unavailable"), reason)?,
                _ => {
                    return Err(EntryError::Rule {
                        field,
                        problem: "give either values or the reason it is unavailable".to_owned(),
                    });
                }
            }
        }
        if self.screens.is_empty() {
            return Err(EntryError::Empty {
                field: "screens".to_owned(),
            });
        }
        for screen in &self.screens {
            check_id("screens", screen)?;
        }
        check_unique("screens", &self.screens)?;
        check_text("maintainer", &self.maintainer)
    }
}
