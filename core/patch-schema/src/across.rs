//! Windows across schema-v2 recipes: the replacements' precondition windows and the edited images
//! ([`RecipeV2::windows`]). Within one recipe, windows are disjoint ([`RecipeV2::validate`]);
//! across recipes for the same release, precondition windows must be disjoint too. Windows shifted
//! from recipe to recipe would share all but a few bytes, and each hash would reveal the
//! difference. Even identical windows are refused: each recipe's rules assume that the bytes around
//! its span are published nowhere, and that its own span alone decides which stock bytes
//! `bytes_hex` keeps, so another recipe's span inside the same window breaks both. An edited image
//! must also be clear of other recipes' precondition windows, whose hash would cover its pixels.
//! Images of different recipes may overlap: no hash of an image is published, and each recipe is
//! applied on its own to the official file, so two edits of one image never meet.

use crate::v2::RecipeV2;
use crate::windows::WindowOwner;
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

/// Checks that, per release, the precondition windows of `recipes` are pairwise disjoint and that
/// no edited image overlaps one. Each recipe should already be valid; a window that does not fit
/// the 64-bit range is skipped (validation refuses it).
///
/// # Errors
///
/// The first [`WindowOverlap`] of two precondition windows in window order, or else the first of
/// an image (in image order) and a precondition window.
pub fn check_windows_across<'a>(
    recipes: impl IntoIterator<Item = &'a RecipeV2>,
) -> Result<(), WindowOverlap> {
    let (mut windows, mut images): (Vec<Named<'a>>, Vec<Named<'a>>) = (Vec::new(), Vec::new());
    for recipe in recipes {
        for labelled in recipe.windows() {
            let named = Named {
                release: &recipe.target.release,
                range: labelled.window,
                name: format!("{} {}", recipe.recipe_id, labelled.owner),
            };
            match labelled.owner {
                WindowOwner::Replacement(_) => windows.push(named),
                WindowOwner::ImageEdit(_) => images.push(named),
            }
        }
    }
    windows.sort_by(|a, b| a.key().cmp(&b.key()));
    images.sort_by(|a, b| a.key().cmp(&b.key()));
    // Sorted by start, the windows are pairwise disjoint if neighbours are.
    for pair in windows.windows(2) {
        if pair[0].release == pair[1].release && pair[1].range.start < pair[0].range.end {
            return Err(Named::overlap(&pair[0], &pair[1]));
        }
    }
    // Disjoint and sorted, a release's windows also end in order. The first of them that ends
    // after an image starts is the only one to check: if it starts at or after the image's end,
    // every later window does too.
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
}

impl<'a> Named<'a> {
    fn key(&self) -> (&'a str, u64, u64) {
        (self.release, self.range.start, self.range.end)
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
            second: second.name.clone(),
            second_window: second.range.clone(),
        }
    }
}
