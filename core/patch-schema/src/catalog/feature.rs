//! A feature: a behaviour change declared against capability and slot names, never against a
//! player, with one implementation (a recipe) per player (`catalog/features/<id>.json`).

use super::entry::{
    CatalogEntry, EntryError, Maturity, check_id, check_implementation, check_labels, check_name,
    check_player_id, check_schema_version, check_slot_ref, check_text, check_unique,
    split_slot_ref, unique_keys,
};
use super::skin::STOCK_SKIN;
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
    #[serde(default, deserialize_with = "unique_keys")]
    pub labels: BTreeMap<String, Vec<String>>,
    /// The implementations for each player id: one per way of drawing the feature's labels
    /// ([`FeatureImplementation::draws_labels`]).
    #[serde(deserialize_with = "unique_keys")]
    pub implementations: BTreeMap<String, Vec<FeatureImplementation>>,
    pub maintainer: String,
}

/// What a player must have for the feature.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Requires {
    /// Capabilities that must be available, each holding the listed values (none: only
    /// available).
    #[serde(default, deserialize_with = "unique_keys")]
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
    /// The screens whose labels the recipe draws itself, each in the style of a skin
    /// (`{"perform": "stock"}`): those screens must then keep that skin. A labelled screen not
    /// listed is left to the chosen skin (a skin implementation drawing that label set). Only
    /// `stock` is accepted for now: a skin cannot yet leave slots for a feature to draw, so a
    /// feature drawing in another skin's style would edit the same images as the skin.
    #[serde(default, deserialize_with = "unique_keys")]
    pub draws_labels: BTreeMap<String, String>,
}

impl CatalogEntry for Feature {
    const KIND: &'static str = "feature";

    fn id(&self) -> &str {
        &self.id
    }

    fn validate(&self) -> Result<(), EntryError> {
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
        for (player, implementations) in &self.implementations {
            check_player_id("implementations", player)?;
            if implementations.is_empty() {
                return Err(EntryError::Empty {
                    field: format!("implementations.{player}"),
                });
            }
            for (index, implementation) in implementations.iter().enumerate() {
                let field = format!("implementations.{player}[{index}]");
                self.check_implementation(&field, implementation)?;
                let draws = &implementation.draws_labels;
                if implementations[..index]
                    .iter()
                    .any(|earlier| &earlier.draws_labels == draws)
                {
                    return Err(EntryError::Rule {
                        field,
                        problem: "another implementation draws the labels the same way".to_owned(),
                    });
                }
            }
        }
        check_text("maintainer", &self.maintainer)
    }
}

impl Feature {
    fn check_implementation(
        &self,
        field: &str,
        implementation: &FeatureImplementation,
    ) -> Result<(), EntryError> {
        check_implementation(
            field,
            &implementation.recipe,
            &implementation.evidence,
            &implementation.limits,
        )?;
        for (screen, skin) in &implementation.draws_labels {
            check_id(&format!("{field}.draws_labels"), screen)?;
            if skin != STOCK_SKIN {
                return Err(EntryError::Rule {
                    field: format!("{field}.draws_labels.{screen}"),
                    problem: "only the stock style for now: a skin cannot yet leave slots for a \
                              feature to draw"
                        .to_owned(),
                });
            }
            let labelled = self
                .labels
                .keys()
                .filter_map(|slot| split_slot_ref(slot))
                .any(|(labelled, _)| labelled == screen);
            if !labelled {
                return Err(EntryError::Rule {
                    field: format!("{field}.draws_labels.{screen}"),
                    problem: "the feature gives no labels on this screen".to_owned(),
                });
            }
        }
        Ok(())
    }
}
