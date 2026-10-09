//! Windows across schema-v2 recipes: the replacements' precondition windows and the edited images
//! ([`RecipeV2::windows`]). Within one recipe, windows are disjoint ([`RecipeV2::validate`]);
//! across recipes for the same release they must be disjoint too. Windows shifted from recipe to
//! recipe would share all but a few bytes, and each hash would reveal the difference. Even
//! identical windows are refused: each recipe's rules assume that the bytes around its span are
//! published nowhere, and that its own span alone decides which stock bytes `bytes_hex` keeps, so
//! another recipe's span inside the same window breaks both. An edited image must also be clear of
//! other recipes' windows: a window's hash over it would cover its pixels, and two edits of the
//! same image would conflict.

use crate::v2::RecipeV2;
use std::ops::Range;
use thiserror::Error;

/// Two overlapping windows of recipes for the same release.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{first} and {second}: windows {first_window:#x?} and {second_window:#x?} overlap; windows of \
     recipes for the same release must be disjoint"
)]
pub struct WindowOverlap {
    /// `recipe_id replacements[index]` or `recipe_id image_edits[index]` of the window that starts
    /// first.
    pub first: String,
    pub first_window: Range<u64>,
    pub second: String,
    pub second_window: Range<u64>,
}

/// Checks that, per release, the windows of `recipes` are pairwise disjoint. Each
/// recipe should already be valid; a window that does not fit the 64-bit range is skipped
/// (validation refuses it).
///
/// # Errors
///
/// The first [`WindowOverlap`] in window order.
pub fn check_windows_across<'a>(
    recipes: impl IntoIterator<Item = &'a RecipeV2>,
) -> Result<(), WindowOverlap> {
    let mut windows: Vec<(&str, Range<u64>, String)> = Vec::new();
    for recipe in recipes {
        for labelled in recipe.windows() {
            let name = format!("{} {}", recipe.recipe_id, labelled.owner);
            windows.push((&recipe.target.release, labelled.window, name));
        }
    }
    windows.sort_by(|a, b| (a.0, a.1.start, a.1.end).cmp(&(b.0, b.1.start, b.1.end)));
    // Sorted by start, the windows are pairwise disjoint if neighbours are.
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
