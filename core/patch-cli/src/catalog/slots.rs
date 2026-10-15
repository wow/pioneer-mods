//! The engine's checks of screen slot images (each lies in its release's decoded application, out
//! of the release's protected ranges, and a recipe that edits it edits exactly that image) and of
//! screen parts (their layout records and images lie in the decoded application, their handlers
//! in the running one).

use anyhow::{Result, bail};
use patch_core::xdj700::RecipeTarget;
use patch_schema::RecipeV2;
use patch_schema::catalog::{RUN_TIME_BASE, Screen, SlotImages, TOUCH_RECORD_LEN};
use std::collections::BTreeMap;

/// Checks `screen`'s slot images against `target` and the catalog's `recipes` for its player.
pub(super) fn check_slot_images(
    screen: &Screen,
    target: &RecipeTarget<'_>,
    recipes: &BTreeMap<String, RecipeV2>,
) -> Result<()> {
    let app_len = target.application_len as u64;
    for (name, slot) in &screen.slots {
        let Some(images) = &slot.images else { continue };
        let owner = format!("screen {}/{}: slot {name}", screen.player, screen.id);
        for image in images.images() {
            let (start, end) = span(images, image.offset);
            let at = format!("{owner}'s image at {start:#x}");
            if end > app_len {
                bail!("{at} ends past the {app_len}-byte application");
            }
            if let Some(range) = target
                .protected
                .iter()
                .find(|range| start < range.end as u64 && (range.start as u64) < end)
            {
                bail!(
                    "{at} overlaps the release's protected range {:#x}..{:#x}",
                    range.start,
                    range.end
                );
            }
            for (path, recipe) in recipes {
                if recipe.target.release != screen.player {
                    continue;
                }
                for (index, edit) in recipe.image_edits.iter().enumerate() {
                    let edit_end = edit.offset.saturating_add(edit.len().unwrap_or(u64::MAX));
                    let overlaps = edit.offset < end && start < edit_end;
                    let same = edit.offset == start
                        && (edit.width, edit.height) == (images.width, images.height);
                    if overlaps && !same {
                        bail!(
                            "recipe {path}: image_edits[{index}] ({}x{} at {:#x}) overlaps {at} \
                             ({}x{}) but is not that image",
                            edit.width,
                            edit.height,
                            edit.offset,
                            images.width,
                            images.height
                        );
                    }
                }
            }
        }
    }
    Ok(())
}

/// Checks `screen`'s parts against `target`: every layout record and image inside the decoded
/// application, every handler inside the application as it runs (loaded at 0x08000000).
pub(super) fn check_parts(screen: &Screen, target: &RecipeTarget<'_>) -> Result<()> {
    let app_len = target.application_len as u64;
    for (name, part) in &screen.parts {
        let at = format!("screen {}/{}: part {name}", screen.player, screen.id);
        for (index, zone) in part.touch.iter().enumerate() {
            let record = u64::from(zone.record.0);
            if record + u64::from(TOUCH_RECORD_LEN) > app_len {
                bail!(
                    "{at}: touch[{index}].record {record:#x} is not in the {app_len}-byte \
                     application"
                );
            }
            let handler = u64::from(zone.handler.0) - u64::from(RUN_TIME_BASE);
            if handler >= app_len {
                bail!(
                    "{at}: touch[{index}].handler {:#010x} is not in the application",
                    zone.handler.0
                );
            }
        }
        for (place, drawn) in part.drawn.iter().enumerate() {
            for (index, image) in drawn.images.iter().enumerate() {
                if u64::from(image.offset.0).saturating_add(image.len()) > app_len {
                    bail!(
                        "{at}: drawn[{place}].images[{index}] ends past the {app_len}-byte \
                         application"
                    );
                }
            }
        }
    }
    Ok(())
}

fn span(images: &SlotImages, offset: u32) -> (u64, u64) {
    let start = u64::from(offset);
    (start, start.saturating_add(images.image_len()))
}
