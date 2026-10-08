//! Precondition windows across schema-v2 recipes. Within one recipe, windows are disjoint
//! ([`RecipeV2::validate`]); across recipes for the same release, any two windows must be
//! identical or disjoint. Otherwise windows shifted from recipe to recipe would share all but a
//! few bytes, and each hash would reveal the difference. Identical windows are allowed: their
//! hashes are equal, so a second one reveals nothing new.

use crate::v2::RecipeV2;
use std::ops::Range;
use thiserror::Error;

/// Two precondition windows of recipes for the same release that overlap without being identical.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{first} and {second}: precondition windows {first_window:#x?} and {second_window:#x?} \
     overlap; windows of recipes for the same release must be identical or disjoint"
)]
pub struct WindowOverlap {
    /// `recipe_id replacements[index]` of the window that starts first.
    pub first: String,
    pub first_window: Range<u64>,
    pub second: String,
    pub second_window: Range<u64>,
}

/// Checks that, per release, any two precondition windows of `recipes` are identical or
/// disjoint. Each recipe should already be valid; a window that does not fit the 64-bit range is
/// skipped (validation refuses it).
///
/// # Errors
///
/// The first [`WindowOverlap`] in window order.
pub fn check_windows_across<'a>(
    recipes: impl IntoIterator<Item = &'a RecipeV2>,
) -> Result<(), WindowOverlap> {
    let mut windows: Vec<(&str, Range<u64>, String)> = Vec::new();
    for recipe in recipes {
        for (index, replacement) in recipe.replacements.iter().enumerate() {
            if let Some(window) = replacement.precondition_window() {
                let name = format!("{} replacements[{index}]", recipe.recipe_id);
                windows.push((&recipe.target.release, window, name));
            }
        }
    }
    windows.sort_by(|a, b| (a.0, a.1.start, a.1.end).cmp(&(b.0, b.1.start, b.1.end)));
    // Identical windows of one release reveal nothing new; keep one of each.
    windows.dedup_by(|later, earlier| later.0 == earlier.0 && later.1 == earlier.1);
    // Sorted by start, the remaining windows are pairwise disjoint if neighbours are.
    for pair in windows.windows(2) {
        let ((release, first, first_name), (next_release, next, next_name)) = (&pair[0], &pair[1]);
        if release == next_release && next.start < first.end {
            return Err(WindowOverlap {
                first: first_name.clone(),
                first_window: first.clone(),
                second: next_name.clone(),
                second_window: next.clone(),
            });
        }
    }
    Ok(())
}
