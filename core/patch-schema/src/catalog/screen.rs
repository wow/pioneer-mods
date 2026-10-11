//! A screen of one player and its slots: named places a skin can fill or a feature can use
//! (`catalog/screens/<player>/<screen>.json`).

use super::acceptance::{Address, Region};
use super::entry::{
    CatalogEntry, EntryError, check_id, check_name, check_player_id, check_schema_version,
    check_text, unique_keys,
};
use super::player::PixelFormat;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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

/// A slot's images: every element drawn from one image per state, all of one size, at a
/// decoded-application offset. A skin edits these images; nothing else of the screen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotImages {
    pub width: u32,
    pub height: u32,
    /// The pixel format: `rgb565` (16-bit little-endian), the only one so far.
    pub pixels: PixelFormat,
    /// The states each element has an image for (`normal`, `pressed`, …).
    pub states: Vec<String>,
    /// One per element, in the slot's order.
    pub elements: Vec<SlotElement>,
    /// How the boxes and images were established, and when the images are read.
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotElement {
    /// Where the element is drawn on the screen.
    #[serde(rename = "box")]
    pub area: Region,
    /// Its image for each state, by decoded-application offset.
    #[serde(deserialize_with = "unique_keys")]
    pub images: BTreeMap<String, Address>,
}

impl SlotImages {
    /// The length of one image, in bytes.
    pub fn image_len(&self) -> u64 {
        u64::from(self.width) * u64::from(self.height) * 2
    }

    /// Every image's decoded-application offset, element by element, state by state.
    pub fn offsets(&self) -> impl Iterator<Item = u32> + '_ {
        self.elements
            .iter()
            .flat_map(|element| element.images.values().map(|offset| offset.0))
    }

    fn validate(&self, field: &str, count: u32) -> Result<(), EntryError> {
        let rule = |at: &str, problem: &str| EntryError::Rule {
            field: format!("{field}.{at}"),
            problem: problem.to_owned(),
        };
        if self.width == 0 || self.height == 0 || self.image_len() > 1 << 20 {
            return Err(rule("width", "an image is 1 MiB at most, and not empty"));
        }
        let states: BTreeSet<&str> = self.states.iter().map(String::as_str).collect();
        if states.is_empty() || states.len() != self.states.len() {
            return Err(rule("states", "at least one state, none twice"));
        }
        for state in &self.states {
            check_name(&format!("{field}.states"), state)?;
        }
        if self.elements.len() != count as usize {
            return Err(rule("elements", "one element per slot element (its count)"));
        }
        let mut images = Vec::new();
        for (index, element) in self.elements.iter().enumerate() {
            let at = format!("elements[{index}]");
            if element.area.w != self.width || element.area.h != self.height {
                return Err(rule(&format!("{at}.box"), "the box is the images' size"));
            }
            if !element
                .images
                .keys()
                .map(String::as_str)
                .eq(states.iter().copied())
            {
                return Err(rule(
                    &format!("{at}.images"),
                    "one image per state, no other",
                ));
            }
            for offset in element.images.values() {
                if offset.0 % 2 != 0 {
                    return Err(rule(&format!("{at}.images"), "an image is 2-byte aligned"));
                }
                images.push(u64::from(offset.0));
            }
        }
        images.sort_unstable();
        if images
            .windows(2)
            .any(|pair| pair[1] - pair[0] < self.image_len())
        {
            return Err(rule("elements", "no two images overlap"));
        }
        check_text(&format!("{field}.evidence"), &self.evidence)
    }
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
        check_text("maintainer", &self.maintainer)
    }
}
