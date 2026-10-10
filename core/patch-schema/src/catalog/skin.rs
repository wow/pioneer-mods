//! A skin: a look for one screen, drawn with the project's or the community's own art, or with
//! transforms of the player's own images on the owner's computer (`catalog/skins/<id>.json`).
//! The skin `stock` (the player's own look) is built in and has no file.

use super::entry::{
    EntryError, Maturity, check_id, check_implementation, check_labels, check_name,
    check_player_id, check_schema_version, check_text, check_unique,
};
use super::player::ScreenClass;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// The id of the built-in skin: the player's own look, with no changes of its own.
pub const STOCK_SKIN: &str = "stock";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Skin {
    pub schema_version: u32,
    /// Named for what it looks like, never after another product.
    pub id: String,
    pub title: String,
    /// The screen it is for (`perform`).
    pub screen: String,
    pub requires: SkinRequires,
    pub art: Art,
    /// The implementations for each player id: one per label set the skin draws, since each
    /// (skin, label set) pair has its own output pin.
    pub implementations: BTreeMap<String, Vec<SkinImplementation>>,
    pub maintainer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkinRequires {
    pub screen_class: ScreenClass,
    /// Slot names on its screen that must exist (`beat_loop.pad`).
    #[serde(default)]
    pub slots: Vec<String>,
}

/// Where the skin's pixels come from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Art {
    /// Drawn by the skin's authors, who declare it their own.
    Original,
    /// Computed from the player's own images on the owner's computer; no pixel is published.
    Transform,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkinImplementation {
    /// The recipe for this player and label set, relative to the repository root. It must pin
    /// its output.
    pub recipe: String,
    pub maturity: Maturity,
    pub evidence: String,
    #[serde(default)]
    pub limits: Vec<String>,
    /// The labels it draws, by slot name on its screen; none for the stock labels.
    #[serde(default)]
    pub labels: BTreeMap<String, Vec<String>>,
}

impl Skin {
    /// Checks the file on its own (the catalog checks its references).
    ///
    /// # Errors
    ///
    /// The first [`EntryError`] found.
    pub fn validate(&self) -> Result<(), EntryError> {
        check_schema_version(self.schema_version)?;
        check_id("id", &self.id)?;
        if self.id == STOCK_SKIN {
            return Err(EntryError::Rule {
                field: "id".to_owned(),
                problem: "\"stock\" is the built-in skin".to_owned(),
            });
        }
        check_text("title", &self.title)?;
        check_id("screen", &self.screen)?;
        for slot in &self.requires.slots {
            check_name("requires.slots", slot)?;
        }
        check_unique("requires.slots", &self.requires.slots)?;
        if self.implementations.is_empty() {
            return Err(EntryError::Empty {
                field: "implementations".to_owned(),
            });
        }
        for (player, implementations) in &self.implementations {
            check_player_id("implementations", player)?;
            if implementations.is_empty() {
                return Err(EntryError::Empty {
                    field: format!("implementations.{player}"),
                });
            }
            for (index, implementation) in implementations.iter().enumerate() {
                self.check_implementation(
                    &format!("implementations.{player}[{index}]"),
                    implementation,
                )?;
            }
            for (index, implementation) in implementations.iter().enumerate() {
                if implementations[..index]
                    .iter()
                    .any(|earlier| earlier.labels == implementation.labels)
                {
                    return Err(EntryError::Rule {
                        field: format!("implementations.{player}[{index}]"),
                        problem: "another implementation draws the same label set".to_owned(),
                    });
                }
            }
        }
        check_text("maintainer", &self.maintainer)
    }

    fn check_implementation(
        &self,
        field: &str,
        implementation: &SkinImplementation,
    ) -> Result<(), EntryError> {
        check_implementation(
            field,
            &implementation.recipe,
            &implementation.evidence,
            &implementation.limits,
        )?;
        for (slot, labels) in &implementation.labels {
            check_name(&format!("{field}.labels"), slot)?;
            check_labels(&format!("{field}.labels.{slot}"), labels)?;
            if !self.requires.slots.contains(slot) {
                return Err(EntryError::Rule {
                    field: format!("{field}.labels.{slot}"),
                    problem: "a slot the skin draws labels in must be in requires.slots".to_owned(),
                });
            }
        }
        Ok(())
    }
}
