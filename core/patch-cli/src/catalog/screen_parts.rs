//! The engine's checks of screen parts against their release:
//!
//! - every touch record, image and table inside the decoded application, and every handler inside
//!   the running one (a bounds check only: the owner test `official_screen_parts` holds each
//!   record to its handler);
//! - part images held like slot images (out of the protected ranges, edited only whole);
//! - the stock values a part publishes (its touch records and tables) kept out of the committed
//!   recipes' precondition windows, except in the ranges review accepted
//!   ([`PUBLISHED_IN_WINDOWS`]).

use super::slots::check_image;
use anyhow::{Result, bail};
use patch_core::xdj700::RecipeTarget;
use patch_schema::RecipeV2;
use patch_schema::catalog::{Player, Screen};
use std::collections::BTreeMap;
use std::ops::Range;

/// Decoded-application ranges inside committed precondition windows whose stock values the
/// catalogue may publish, as reviewed (docs/recipes.md, "Known exceptions"): the deck screens'
/// touch records, and the BEAT LOOP and SLIP LOOP tables' index bytes. Widening this is a change
/// review sees.
const PUBLISHED_IN_WINDOWS: &[(&str, Range<u64>)] = &[
    ("xdj700-v1.15", 0xd_5d38..0xd_5f68),
    ("xdj700-v1.15", 0xd_6220..0xd_6250),
];

/// Checks `screen`'s parts against `player`, `target` and the catalog's `recipes`.
pub(super) fn check_screen_parts(
    screen: &Screen,
    player: &Player,
    target: &RecipeTarget<'_>,
    recipes: &BTreeMap<String, RecipeV2>,
) -> Result<()> {
    let app_len = target.application_len as u64;
    let pixel = u64::from(player.screen_class.pixels.bytes_per_pixel());
    // The catalog check has given every player whose screens have touch zones a touch layout.
    let record_bytes = player
        .touch_layout
        .as_ref()
        .map_or(0, |layout| u64::from(layout.record_bytes));
    let windows = windows(recipes, &screen.player);
    let running = target.load_address..target.load_address + app_len;
    for (name, part) in &screen.parts {
        let owner = format!("screen {}/{}: part {name}", screen.player, screen.id);
        for (index, zone) in part.touch.iter().enumerate() {
            let at = format!("{owner}: touch[{index}]");
            let record = u64::from(zone.record.0);
            let range = record..record + record_bytes;
            if range.end > app_len {
                bail!("{at}.record {record:#x} is not in the {app_len}-byte application");
            }
            published(&at, &range, &screen.player, &windows)?;
            if !running.contains(&u64::from(zone.handler.0)) {
                bail!(
                    "{at}.handler {:#010x} is not in the application",
                    zone.handler.0
                );
            }
        }
        for (place, drawn) in part.drawn.iter().enumerate() {
            for (index, image) in drawn.images.iter().enumerate() {
                let start = u64::from(image.offset.0);
                let len = u64::from(image.width) * u64::from(image.height) * pixel;
                let at = format!("{owner}: drawn[{place}].images[{index}] at {start:#x}");
                let size = (image.width, image.height);
                check_image(
                    &at,
                    (start, start + len),
                    size,
                    target,
                    recipes,
                    &screen.player,
                )?;
            }
        }
        for (index, table) in part.tables.iter().enumerate() {
            let at = format!("{owner}: tables[{index}]");
            for entry in table.entries() {
                if entry.end > app_len {
                    bail!("{at} ends past the {app_len}-byte application");
                }
                published(&at, &entry, &screen.player, &windows)?;
            }
        }
    }
    Ok(())
}

/// Every precondition window of the catalog's recipes for `release`, with its recipe's path.
fn windows<'a>(
    recipes: &'a BTreeMap<String, RecipeV2>,
    release: &str,
) -> Vec<(&'a str, Range<u64>)> {
    recipes
        .iter()
        .filter(|(_, recipe)| recipe.target.release == release)
        .flat_map(|(path, recipe)| {
            recipe.replacements.iter().map(move |replacement| {
                let start = replacement
                    .offset
                    .saturating_sub(replacement.precondition.before);
                let end = replacement.offset + replacement.len() + replacement.precondition.after;
                (path.as_str(), start..end)
            })
        })
        .collect()
}

/// Refuses publishing the stock bytes of `range` (`at` names them) inside a committed window,
/// unless review accepted that range for `release`.
fn published(
    at: &str,
    range: &Range<u64>,
    release: &str,
    windows: &[(&str, Range<u64>)],
) -> Result<()> {
    let accepted = PUBLISHED_IN_WINDOWS.iter().any(|(id, allowed)| {
        *id == release && allowed.start <= range.start && range.end <= allowed.end
    });
    if accepted {
        return Ok(());
    }
    if let Some((path, window)) = windows
        .iter()
        .find(|(_, window)| range.start < window.end && window.start < range.end)
    {
        bail!(
            "{at} ({:#x}..{:#x}) lies in recipe {path}'s precondition window ({:#x}..{:#x}): \
             publishing its stock value would weaken that window's hash",
            range.start,
            range.end,
            window.start,
            window.end
        );
    }
    Ok(())
}
