//! A feature: a behaviour change declared against capability and slot names, never against a
//! player, with one implementation (a recipe) per player (`catalog/features/<id>.json`).

use super::entry::{
    EntryError, Maturity, check_id, check_implementation, check_labels, check_name,
    check_player_id, check_schema_version, check_slot_ref, check_text, check_unique,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub schema_version: u32,
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub requires: Requires,
    /// Features that cannot be chosen with this one, because they mean something else for the
    /// same controls (listed on both sides).
    #[serde(default)]
    pub conflicts: Vec<String>,
    /// The text this feature gives each slot it relabels (`perform.beat_loop.pad`), one label per
    /// element. A skin draws them; see [`FeatureImplementation::draws_labels`].
    #[serde(default)]
    pub labels: BTreeMap<String, Vec<String>>,
    /// One implementation per player id.
    pub implementations: BTreeMap<String, FeatureImplementation>,
    pub maintainer: String,
}

/// What a player must have for the feature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Capabilities that must be available, each holding the listed values (none: only
    /// available).
    #[serde(default)]
    pub capabilities: BTreeMap<String, Vec<String>>,
    /// Slots that must exist, as `<screen>.<slot name>`.
    #[serde(default)]
    pub slots: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FeatureImplementation {
    /// The recipe for this player, relative to the repository root. It must pin its output.
    pub recipe: String,
    pub maturity: Maturity,
    /// Where the maturity is established (a stage of the flashing guide, an emulator rehearsal).
    pub evidence: String,
    /// Known limits the builder shows with the feature.
    #[serde(default)]
    pub limits: Vec<String>,
    /// Set when the recipe draws the feature's labels itself, in the style of this skin
    /// (`stock`): the labelled screens must then use that skin.
    #[serde(default)]
    pub draws_labels: Option<String>,
}

impl Feature {
    /// Checks the file on its own (the catalog checks its references).
    ///
    /// # Errors
    ///
    /// The first [`EntryError`] found.
    pub fn validate(&self) -> Result<(), EntryError> {
        check_schema_version(self.schema_version)?;
        check_id("id", &self.id)?;
        check_text("title", &self.title)?;
        check_text("description", &self.description)?;
        for name in self.requires.capabilities.keys() {
            check_name("requires.capabilities", name)?;
        }
        for (name, values) in &self.requires.capabilities {
            let field = format!("requires.capabilities.{name}");
            for value in values {
                check_text(&field, value)?;
            }
            check_unique(&field, values)?;
        }
        for slot in &self.requires.slots {
            check_slot_ref("requires.slots", slot)?;
        }
        check_unique("requires.slots", &self.requires.slots)?;
        for conflict in &self.conflicts {
            check_id("conflicts", conflict)?;
            if conflict == &self.id {
                return Err(EntryError::Rule {
                    field: "conflicts".to_owned(),
                    problem: "a feature cannot conflict with itself".to_owned(),
                });
            }
        }
        check_unique("conflicts", &self.conflicts)?;
        for (slot, labels) in &self.labels {
            check_slot_ref("labels", slot)?;
            check_labels(&format!("labels.{slot}"), labels)?;
            if !self.requires.slots.contains(slot) {
                return Err(EntryError::Rule {
                    field: format!("labels.{slot}"),
                    problem: "a relabelled slot must be in requires.slots".to_owned(),
                });
            }
        }
        if self.implementations.is_empty() {
            return Err(EntryError::Empty {
                field: "implementations".to_owned(),
            });
        }
        for (player, implementation) in &self.implementations {
            let field = format!("implementations.{player}");
            check_player_id("implementations", player)?;
            check_implementation(
                &field,
                &implementation.recipe,
                &implementation.evidence,
                &implementation.limits,
            )?;
            if let Some(skin) = &implementation.draws_labels {
                check_id(&format!("{field}.draws_labels"), skin)?;
                if self.labels.is_empty() {
                    return Err(EntryError::Rule {
                        field: format!("{field}.draws_labels"),
                        problem: "the feature gives no labels to draw".to_owned(),
                    });
                }
            }
        }
        check_text("maintainer", &self.maintainer)
    }
}
