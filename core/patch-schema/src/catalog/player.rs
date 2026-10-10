//! A player: one model and firmware release, its pins, screen class, budgets, capabilities and
//! screens (`catalog/players/<id>.json`).

use super::entry::{
    EntryError, check_id, check_name, check_player_id, check_schema_version, check_sha256,
    check_text, check_unique, unique_keys,
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
    pub budgets: Budgets,
    /// Named facts about the stock firmware ([`Capability`]).
    #[serde(default, deserialize_with = "unique_keys")]
    pub capabilities: BTreeMap<String, Capability>,
    /// The screens catalogued for this player, each in `catalog/screens/<player>/<screen>.json`.
    pub screens: Vec<String>,
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

impl Player {
    /// Checks the file on its own (the catalog checks its references).
    ///
    /// # Errors
    ///
    /// The first [`EntryError`] found.
    pub fn validate(&self) -> Result<(), EntryError> {
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
