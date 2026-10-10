//! Windows across schema-v2 recipes: the replacements' precondition windows and the edited images
//! ([`RecipeV2::windows`]). Within one recipe, windows are disjoint ([`RecipeV2::validate`]);
//! across recipes for the same release, precondition windows must be disjoint too. Windows shifted
//! from recipe to recipe would share all but a few bytes, and each hash would reveal the
//! difference. Identical windows are refused too, unless their replacements are identical: each
//! recipe's rules assume that the bytes around its span are published nowhere, and that its own
//! span alone decides which stock bytes `bytes_hex` writes out, so another recipe's span inside
//! the same window breaks both. A replacement repeated exactly (offset, bytes, window and hash;
//! the purpose may differ) publishes nothing the first did not, so a recipe may build on another's
//! change. An edited image must also be clear of other recipes' precondition windows, whose hash
//! would cover its pixels. Images of different recipes may overlap: no hash of an image is
//! published, and each recipe is applied on its own to the official file, so two edits of one
//! image never meet.

use crate::v2::{RecipeV2, Replacement};
use crate::windows::WindowOwner;
use std::ops::Range;
use thiserror::Error;

/// Two overlapping windows of recipes for the same release.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error(
    "{first} and {second}: windows {first_window:#x?} and {second_window:#x?} overlap; windows of \
     recipes for the same release must be disjoint, unless one recipe repeats another's \
     replacement exactly (offset, bytes_hex, before, after and hash)"
)]
pub struct WindowOverlap {
    /// `recipe_id replacements[index]` or `recipe_id image_edits[index]` of the window that starts
    /// first.
    pub first: String,
    pub first_window: Range<u64>,
    /// The position of the recipe holding the first window, in the order the recipes were given.
    pub first_recipe: usize,
    pub second: String,
    pub second_window: Range<u64>,
    /// The position of the recipe holding the second window.
    pub second_recipe: usize,
}

/// Checks that, per release, the precondition windows of `recipes` are pairwise disjoint (or belong
/// to identical replacements) and that no edited image overlaps one. Each recipe should already be
/// valid; a window that does not fit the 64-bit range is skipped (validation refuses it).
///
/// # Errors
///
/// The first [`WindowOverlap`] of two precondition windows in window order, or else the first of
/// an image (in image order) and a precondition window.
pub fn check_windows_across<'a>(
    recipes: impl IntoIterator<Item = &'a RecipeV2>,
) -> Result<(), WindowOverlap> {
    let (mut windows, mut images): (Vec<Named<'a>>, Vec<Named<'a>>) = (Vec::new(), Vec::new());
    for (position, recipe) in recipes.into_iter().enumerate() {
        for labelled in recipe.windows() {
            let replacement = match labelled.owner {
                WindowOwner::Replacement(index) => recipe.replacements.get(index),
                WindowOwner::ImageEdit(_) => None,
            };
            let named = Named {
                release: &recipe.target.release,
                range: labelled.window,
                name: format!("{} {}", recipe.recipe_id, labelled.owner),
                position,
                replacement,
            };
            match labelled.owner {
                WindowOwner::Replacement(_) => windows.push(named),
                WindowOwner::ImageEdit(_) => images.push(named),
            }
        }
    }
    windows.sort_by(|a, b| a.key().cmp(&b.key()));
    images.sort_by(|a, b| a.key().cmp(&b.key()));
    // Sorted by start, the windows are pairwise disjoint if neighbours are; identical
    // replacements share one window, so a window overlapping one of them meets the other too.
    for pair in windows.windows(2) {
        let (first, next) = (&pair[0], &pair[1]);
        if first.release == next.release
            && next.range.start < first.range.end
            && !Named::same_replacement(first, next)
        {
            return Err(Named::overlap(first, next));
        }
    }
    // Disjoint (or identical) and sorted, a release's windows also end in order. The first of them
    // that ends after an image starts is the only one to check: if it starts at or after the
    // image's end, every later window does too.
    for image in &images {
        let next = windows.partition_point(|window| {
            (window.release, window.range.end) <= (image.release, image.range.start)
        });
        if let Some(window) = windows.get(next)
            && window.release == image.release
            && window.range.start < image.range.end
        {
            return Err(Named::overlap(window, image));
        }
    }
    Ok(())
}

/// A window of one recipe, named for errors.
struct Named<'a> {
    release: &'a str,
    range: Range<u64>,
    /// `recipe_id replacements[index]` or `recipe_id image_edits[index]`.
    name: String,
    /// The position of the recipe in the order given.
    position: usize,
    /// The replacement the window belongs to; `None` for an image.
    replacement: Option<&'a Replacement>,
}

impl<'a> Named<'a> {
    fn key(&self) -> (&'a str, u64, u64) {
        (self.release, self.range.start, self.range.end)
    }

    /// Whether `a` and `b` are the same replacement: offset, bytes, window and hash (compared
    /// without regard to case), whatever their purposes say.
    fn same_replacement(a: &Named<'_>, b: &Named<'_>) -> bool {
        let (Some(a), Some(b)) = (a.replacement, b.replacement) else {
            return false;
        };
        a.repeats(b)
    }

    /// The overlap of `a` and `b`, the one that starts first first.
    fn overlap(a: &Named<'_>, b: &Named<'_>) -> WindowOverlap {
        let (first, second) = if (b.range.start, b.range.end) < (a.range.start, a.range.end) {
            (b, a)
        } else {
            (a, b)
        };
        WindowOverlap {
            first: first.name.clone(),
            first_window: first.range.clone(),
            first_recipe: first.position,
            second: second.name.clone(),
            second_window: second.range.clone(),
            second_recipe: second.position,
        }
    }
}

impl Replacement {
    /// Whether `other` repeats this replacement exactly: the same offset, `bytes_hex`, window and
    /// hash (hex compared without regard to case); only the purpose may differ. A repeat publishes
    /// nothing new, so recipes may share it, and a composed build applies it once.
    pub fn repeats(&self, other: &Replacement) -> bool {
        self.offset == other.offset
            && self.bytes_hex.eq_ignore_ascii_case(&other.bytes_hex)
            && self.precondition.before == other.precondition.before
            && self.precondition.after == other.precondition.after
            && self
                .precondition
                .sha256
                .eq_ignore_ascii_case(&other.precondition.sha256)
    }
}
