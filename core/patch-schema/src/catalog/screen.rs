//! A screen of one player, its slots (named places a skin can fill or a feature can use) and its
//! parts (everything it shows, mapped: [`Part`]) (`catalog/screens/<player>/<screen>.json`).

use super::entry::{
    CatalogEntry, EntryError, check_id, check_name, check_player_id, check_schema_version,
    check_text, unique_keys,
};
use super::screen_images::SlotImages;
use super::screen_parts::{Part, TOUCH_RECORD_LEN};
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
    #[serde(default, deserialize_with = "unique_keys")]
    pub slots: BTreeMap<String, Slot>,
    /// Its parts, by name, when mapped.
    #[serde(
        default,
        deserialize_with = "unique_keys",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub parts: BTreeMap<String, Part>,
    pub maintainer: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Slot {
    /// How many elements the slot has (six BEAT LOOP buttons): a feature gives one label each.
    pub count: u32,
    pub description: String,
    /// Where the slot's elements are drawn, and from which images, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub images: Option<SlotImages>,
}

impl CatalogEntry for Screen {
    const KIND: &'static str = "screen";

    fn id(&self) -> &str {
        &self.id
    }

    fn name(&self) -> String {
        format!("screen {}/{}", self.player, self.id)
    }

    fn validate(&self) -> Result<(), EntryError> {
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
            if let Some(images) = &slot.images {
                images.validate(&format!("slots.{name}.images"), slot.count)?;
            }
        }
        check_records_apart(&self.parts)?;
        for (name, part) in &self.parts {
            part.validate(name)?;
            if let Some(slot) = &part.slot
                && !self.slots.contains_key(slot)
            {
                return Err(EntryError::Rule {
                    field: format!("parts.{name}.slot"),
                    problem: format!("the screen has no slot {slot}"),
                });
            }
        }
        check_text("maintainer", &self.maintainer)
    }
}

/// No two touch zones of a screen name one layout record, or records that overlap.
fn check_records_apart(parts: &BTreeMap<String, Part>) -> Result<(), EntryError> {
    let mut records: Vec<(u32, &str)> = parts
        .iter()
        .flat_map(|(name, part)| {
            part.touch
                .iter()
                .map(move |zone| (zone.record.0, name.as_str()))
        })
        .collect();
    records.sort_unstable();
    if let Some(pair) = records
        .windows(2)
        .find(|pair| u64::from(pair[1].0) < u64::from(pair[0].0) + u64::from(TOUCH_RECORD_LEN))
    {
        return Err(EntryError::Rule {
            field: format!("parts.{}.touch", pair[1].1),
            problem: format!(
                "its record {:#x} overlaps part {}'s record {:#x} (a record is {TOUCH_RECORD_LEN} \
                 bytes)",
                pair[1].0, pair[0].1, pair[0].0
            ),
        });
    }
    Ok(())
}
