//! Checks the recipe entry points add to the built-in ones, and the overlap walk the protected
//! ranges and the protected set share.

use super::protected_set::{ProtectedSet, check_recipe_against_protected_set};
use super::recipe::{RecipeError, RecipeTarget};
use patch_schema::{RecipeV2, WindowOwner};
use std::ops::Range;

/// Checks a caller adds to the built-in ones, which every entry point runs again rather than trust
/// its caller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RecipeChecks<'a> {
    /// The protected set measured in emulation ([`check_recipe_against_protected_set`]). `None`
    /// skips that check, which is the caller's decision to make and to report.
    pub protected_set: Option<&'a ProtectedSet>,
}

impl RecipeChecks<'static> {
    /// No added checks.
    pub const NONE: Self = Self {
        protected_set: None,
    };
}

impl RecipeChecks<'_> {
    /// Runs the added checks on a recipe that passed [`check_recipe_v2`](super::check_recipe_v2).
    pub(super) fn run(
        &self,
        recipe: &RecipeV2,
        target: &RecipeTarget<'_>,
    ) -> Result<(), RecipeError> {
        match self.protected_set {
            Some(set) => check_recipe_against_protected_set(recipe, target, set),
            None => Ok(()),
        }
    }
}

/// A window of `owner` (a replacement's span or precondition window, or an image) that overlaps a
/// protected range, in half-open decoded offsets.
pub(super) struct Overlap<'r> {
    pub owner: WindowOwner,
    pub what: &'static str,
    pub start: u64,
    pub end: u64,
    pub protected: &'r Range<usize>,
}

/// `window` as a range of an application of `application_len` bytes, or `out_of_bounds(end)` if it
/// ends past the application. The one bounds check of every window kind.
pub(super) fn bounded(
    window: Option<Range<u64>>,
    application_len: usize,
    out_of_bounds: impl Fn(u64) -> RecipeError,
) -> Result<Range<usize>, RecipeError> {
    // `validate` has checked that the window fits the 64-bit range.
    let Range { start, end } = window.unwrap_or(u64::MAX..u64::MAX);
    let start = usize::try_from(start).map_err(|_| out_of_bounds(end))?;
    let end_usize = usize::try_from(end).map_err(|_| out_of_bounds(end))?;
    if end_usize > application_len {
        return Err(out_of_bounds(end));
    }
    Ok(start..end_usize)
}

/// The bounds error of `owner`'s window.
pub(super) fn out_of_bounds(
    owner: WindowOwner,
    application_len: usize,
) -> impl Fn(u64) -> RecipeError {
    move |end| match owner {
        WindowOwner::Replacement(index) => RecipeError::OutOfBounds {
            index,
            end,
            len: application_len,
        },
        WindowOwner::ImageEdit(index) => RecipeError::ImageOutOfBounds {
            index,
            end,
            len: application_len,
        },
    }
}

/// Checks every window of `recipe` ([`RecipeV2::windows`], in recipe order) against `protected`:
/// a replacement's span, then its precondition window; an image edit's image. Each window is
/// bounds-checked first. Returns `refuse` of the first overlap.
pub(super) fn check_ranges(
    recipe: &RecipeV2,
    target: &RecipeTarget<'_>,
    protected: &[Range<usize>],
    refuse: impl Fn(Overlap<'_>) -> RecipeError,
) -> Result<(), RecipeError> {
    for labelled in recipe.windows() {
        let owner = labelled.owner;
        let window = bounded(
            Some(labelled.window.clone()),
            target.application_len,
            out_of_bounds(owner, target.application_len),
        )?;
        let window = window.start as u64..window.end as u64;
        let ranges = match &labelled.span {
            Some(span) => vec![("span", span.clone()), ("precondition window", window)],
            None => vec![("image", window)],
        };
        for (what, range) in ranges {
            let overlap = |protected: &&Range<usize>| {
                range.start < protected.end as u64 && (protected.start as u64) < range.end
            };
            if let Some(protected) = protected.iter().find(overlap) {
                return Err(refuse(Overlap {
                    owner,
                    what,
                    start: range.start,
                    end: range.end,
                    protected,
                }));
            }
        }
    }
    Ok(())
}
