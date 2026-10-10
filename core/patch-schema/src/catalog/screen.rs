//! A screen of one player and its slots: named places a skin can fill or a feature can use
//! (`catalog/screens/<player>/<screen>.json`).

use super::entry::{
    EntryError, check_id, check_name, check_player_id, check_schema_version, check_text,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Screen {
    pub schema_version: u32,
    /// The screen's id, the same for every player that has it (`main`, `perform`).
    pub id: String,
    pub player: String,
    pub title: String,
    #[serde(default)]
    pub slots: BTreeMap<String, Slot>,
    pub maintainer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    /// How many elements the slot has (six BEAT LOOP buttons): a feature gives one label each.
    pub count: u32,
    pub description: String,
}

impl Screen {
    /// Checks the file on its own (the catalog checks its references).
    ///
    /// # Errors
    ///
    /// The first [`EntryError`] found.
    pub fn validate(&self) -> Result<(), EntryError> {
        check_schema_version(self.schema_version)?;
        check_id("id", &self.id)?;
        check_player_id("player", &self.player)?;
        check_text("title", &self.title)?;
        for (name, slot) in &self.slots {
            check_name("slots", name)?;
            if slot.count == 0 {
                return Err(EntryError::Rule {
                    field: format!("slots.{name}.count"),
                    problem: "a slot has at least one element".to_owned(),
                });
            }
            check_text(&format!("slots.{name}.description"), &slot.description)?;
        }
        check_text("maintainer", &self.maintainer)
    }
}
