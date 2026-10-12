//! A screen slot's images (`slots.<name>.images`): where each element is drawn, and from which
//! image per state. A skin edits these images and nothing else of the screen.

use super::acceptance::{Address, Region};
use super::entry::{EntryError, check_name, check_text, unique_keys};
use super::player::PixelFormat;
use crate::image::{MAX_IMAGE_BYTES, image_bytes};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Every element of a slot drawn from one image per state, all of one size and format, each at a
/// decoded-application offset.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotImages {
    pub width: u32,
    pub height: u32,
    /// The pixel format: `rgb565` (16-bit little-endian), the only one so far.
    pub pixels: PixelFormat,
    /// The states each element has an image for (`normal`, `pressed`, …), in this order.
    pub states: Vec<String>,
    /// One per element, in the slot's order.
    pub elements: Vec<SlotElement>,
    /// How the positions and images were established, and when the images are read.
    pub evidence: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SlotElement {
    /// Where the element's top-left pixel is drawn on the screen; it covers the images' size.
    pub at: Position,
    /// Its image for each state, by decoded-application offset.
    #[serde(deserialize_with = "unique_keys")]
    pub images: BTreeMap<String, Address>,
}

/// A pixel of the screen, from the top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Position {
    pub x: u32,
    pub y: u32,
}

/// One image of a slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotImage<'a> {
    /// The element's index in the slot.
    pub element: usize,
    pub state: &'a str,
    /// Its decoded-application offset.
    pub offset: u32,
}

impl SlotImages {
    /// The length of one image, in bytes (`u64::MAX` if it overflows, which validation refuses).
    pub fn image_len(&self) -> u64 {
        image_bytes(self.width, self.height, self.pixels.bytes_per_pixel()).unwrap_or(u64::MAX)
    }

    /// The screen area element `index` covers.
    pub fn area(&self, element: &SlotElement) -> Region {
        Region {
            x: element.at.x,
            y: element.at.y,
            w: self.width,
            h: self.height,
        }
    }

    /// Every image, element by element and, within one, in the order of `states`.
    pub fn images(&self) -> impl Iterator<Item = SlotImage<'_>> + '_ {
        self.elements
            .iter()
            .enumerate()
            .flat_map(move |(index, element)| {
                self.states.iter().filter_map(move |state| {
                    let offset = element.images.get(state)?.0;
                    Some(SlotImage {
                        element: index,
                        state,
                        offset,
                    })
                })
            })
    }

    pub(super) fn validate(&self, field: &str, count: u32) -> Result<(), EntryError> {
        let rule = |at: &str, problem: String| EntryError::Rule {
            field: format!("{field}.{at}"),
            problem,
        };
        if self.width == 0 {
            return Err(rule("width", "an image is not empty".to_owned()));
        }
        if self.height == 0 {
            return Err(rule("height", "an image is not empty".to_owned()));
        }
        if self.image_len() > MAX_IMAGE_BYTES {
            return Err(rule(
                "width",
                format!("width x height is {MAX_IMAGE_BYTES} bytes at most"),
            ));
        }
        let states: BTreeSet<&str> = self.states.iter().map(String::as_str).collect();
        if states.is_empty() || states.len() != self.states.len() {
            return Err(rule("states", "at least one state, none twice".to_owned()));
        }
        for state in &self.states {
            check_name(&format!("{field}.states"), state)?;
        }
        if self.elements.len() != count as usize {
            let problem = "one element per slot element (its count)".to_owned();
            return Err(rule("elements", problem));
        }
        for (index, element) in self.elements.iter().enumerate() {
            if !element
                .images
                .keys()
                .map(String::as_str)
                .eq(states.iter().copied())
            {
                let problem = "one image per state, no other".to_owned();
                return Err(rule(&format!("elements[{index}].images"), problem));
            }
            if element.images.values().any(|offset| offset.0 % 2 != 0) {
                let problem = "an image is 2-byte aligned".to_owned();
                return Err(rule(&format!("elements[{index}].images"), problem));
            }
            let area = self.area(element);
            if let Some(other) = self.elements[..index]
                .iter()
                .position(|other| self.area(other).overlaps(&area))
            {
                let problem = format!("elements[{other}] and elements[{index}] overlap on screen");
                return Err(rule("elements", problem));
            }
        }
        let mut offsets: Vec<u64> = self.images().map(|image| u64::from(image.offset)).collect();
        offsets.sort_unstable();
        if offsets
            .windows(2)
            .any(|pair| pair[1] - pair[0] < self.image_len())
        {
            return Err(rule("elements", "no two images overlap".to_owned()));
        }
        check_text(&format!("{field}.evidence"), &self.evidence)
    }
}
