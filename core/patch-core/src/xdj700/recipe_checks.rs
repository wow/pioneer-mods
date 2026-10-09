//! Checks the recipe entry points add to the built-in ones, and the overlap walk the protected
//! ranges and the protected set share.

use super::precondition::window;
use super::protected_set::{ProtectedSet, check_recipe_against_protected_set};
use super::recipe::{RecipeError, RecipeTarget};
use patch_schema::RecipeV2;
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

/// A replacement's span or precondition window that overlaps a protected range (half-open
/// decoded offsets).
pub(super) struct Overlap<'r> {
    pub index: usize,
    pub what: &'static str,
    pub start: u64,
    pub end: u64,
    pub protected: &'r Range<usize>,
}

/// Checks every replacement's span and precondition window against `protected`, returning
/// `refuse` of the first overlap. Windows are bounds-checked first.
pub(super) fn check_ranges(
    recipe: &RecipeV2,
    target: &RecipeTarget<'_>,
    protected: &[Range<usize>],
    refuse: impl Fn(Overlap<'_>) -> RecipeError,
) -> Result<(), RecipeError> {
    for (index, replacement) in recipe.replacements.iter().enumerate() {
        let window = window(index, replacement, target.application_len)?;
        // `validate` has checked that this cannot overflow.
        let span = replacement.offset..replacement.offset.saturating_add(replacement.len());
        let window = window.start as u64..window.end as u64;
        for (what, range) in [("span", span), ("precondition window", window)] {
            let overlap = |protected: &&Range<usize>| {
                range.start < protected.end as u64 && (protected.start as u64) < range.end
            };
            if let Some(protected) = protected.iter().find(overlap) {
                return Err(refuse(Overlap {
                    index,
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
