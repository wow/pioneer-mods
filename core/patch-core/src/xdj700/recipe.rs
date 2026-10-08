//! Applies a schema-v2 recipe ([`RecipeV2`]): changes to the decoded application of a pinned
//! release, rebuilt into a complete update through [`rebuild_with_edited_stock_application`].
//!
//! Before the input is read, the recipe must pass its static checks, name a known release whose
//! pins it repeats exactly, declare a label higher and a reported version lower than the release's
//! own version, and keep every replacement out of the release's protected ranges. On the stock
//! application, the precondition window around each replacement (defined relative to its
//! offset, so a wrong offset moves the window) must match its declared SHA-256. The output
//! application may differ from stock only in the declared replacements and the version string,
//! the rebuild then runs its own verification (including the version rule), and any declared
//! output identities must match.

use super::is_label_higher;
use super::rebuild::{RebuiltUpdate, rebuild_with_edited_stock_application};
use super::release::{OFFICIAL_V115, StockRelease};
use crate::engine::verify_bounded_diff;
use crate::error::{PatchEngineError, RebuildError};
use crate::identity::sha256_hex;
use patch_schema::{RecipeV2, RecipeV2Error, Replacement};
use std::ops::Range;
use thiserror::Error;

/// A release recipes may target: its identifier, its pins, and the decoded-application ranges no
/// replacement may touch.
///
/// The fields are public so that tests can use synthetic targets. Like [`StockRelease`], the
/// safety guarantees hold only for the pinned constants in [`RECIPE_TARGETS`]; production code
/// must never construct a target from user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeTarget<'a> {
    /// The identifier recipes name in `target.release`.
    pub id: &'a str,
    pub release: StockRelease<'a>,
    /// Ranges of the decoded application that replacements may not overlap.
    pub protected: &'a [Range<usize>],
}

/// The releases recipes may target.
///
/// v1.15 protects `[0, 0x800)`: the application header and its version block (the version changes
/// only through `reported_version`); code starts at `0x800`. The code that runs early during
/// start-up is **not yet mapped**, so keeping recipes out of it is still a review and
/// hardware-staging rule (`docs/xdj700-flashing.md`, section 5), not something this list enforces.
pub const RECIPE_TARGETS: &[RecipeTarget<'static>] = &[RecipeTarget {
    id: "xdj700-v1.15",
    release: OFFICIAL_V115,
    protected: &[Range {
        start: 0,
        end: 0x800,
    }],
}];

// Every target pins a version block, and one of its protected ranges covers the version string
// and its NUL, so no replacement can change it.
const _: () = {
    let mut index = 0;
    while index < RECIPE_TARGETS.len() {
        let target = &RECIPE_TARGETS[index];
        let Some(block) = target.release.version_block else {
            panic!("every recipe target pins a version block");
        };
        let mut covered = false;
        let mut range = 0;
        while range < target.protected.len() {
            let protected = &target.protected[range];
            covered |= protected.start <= block.offset
                && block.offset + VERSION_FIELD_TEXT_LEN < protected.end;
            range += 1;
        }
        assert!(covered, "a protected range must cover the version string");
        index += 1;
    }
};

/// Why a recipe was not applied.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum RecipeError {
    #[error("recipe is invalid: {0}")]
    Invalid(#[from] RecipeV2Error),

    #[error("unknown release {release:?}; known: {known}")]
    UnknownRelease { release: String, known: String },

    #[error("{field} is {recipe}, but release {release} pins {pinned}")]
    TargetMismatch {
        field: &'static str,
        release: String,
        recipe: String,
        pinned: String,
    },

    #[error(
        "label {label} is not higher than the release's own version {stock}; the updater would \
         skip it"
    )]
    LabelNotHigher { label: String, stock: String },

    #[error(
        "replacements[{index}] at {start:#x}..{end:#x} overlaps protected range \
         {protected_start:#x}..{protected_end:#x}"
    )]
    Protected {
        index: usize,
        start: u64,
        end: u64,
        protected_start: usize,
        protected_end: usize,
    },

    #[error(
        "replacements[{index}].precondition ends at {end:#x}, past the application's end at \
         {len:#x}"
    )]
    OutOfBounds { index: usize, end: u64, len: usize },

    #[error(
        "replacements[{index}]: the precondition window has SHA-256 {actual}, not {expected}; \
         the recipe does not match this application"
    )]
    Precondition {
        index: usize,
        expected: String,
        actual: String,
    },

    #[error("the application changed outside the declared spans: {0}")]
    UndeclaredChange(PatchEngineError),

    #[error("{field} is {actual}, but the recipe expects {expected}")]
    UnexpectedOutput {
        field: &'static str,
        expected: String,
        actual: String,
    },

    #[error(transparent)]
    Rebuild(#[from] RebuildError),
}

/// Applies `recipe` to `input`, the official update of the release the recipe names.
///
/// # Errors
///
/// [`RecipeError::UnknownRelease`], or any [`apply_recipe_v2_to`] error.
pub fn apply_recipe_v2(recipe: &RecipeV2, input: &[u8]) -> Result<RebuiltUpdate, RecipeError> {
    recipe.validate()?;
    let target = recipe_target(&recipe.target.release)
        .ok_or_else(|| unknown_release(&recipe.target.release))?;
    apply_recipe_v2_to(recipe, target, input)
}

/// [`RecipeError::UnknownRelease`] for `release`, listing the known targets.
pub fn unknown_release(release: &str) -> RecipeError {
    let known: Vec<&str> = RECIPE_TARGETS.iter().map(|target| target.id).collect();
    RecipeError::UnknownRelease {
        release: release.to_owned(),
        known: known.join(", "),
    }
}

/// The known target with identifier `id`.
pub fn recipe_target(id: &str) -> Option<&'static RecipeTarget<'static>> {
    RECIPE_TARGETS.iter().find(|target| target.id == id)
}

/// Every check that needs no firmware: the static recipe checks, the release pins, the label and
/// reported-version order, and the protected ranges. [`apply_recipe_v2_to`] runs it first; front
/// ends can run it before reading the input.
///
/// # Errors
///
/// [`RecipeError::Invalid`], [`RecipeError::TargetMismatch`], [`RecipeError::LabelNotHigher`],
/// [`RecipeError::Protected`], or a [`RecipeError::Rebuild`] for a release without a version
/// block or a reported version that is not lower.
pub fn check_recipe_v2(recipe: &RecipeV2, target: &RecipeTarget<'_>) -> Result<(), RecipeError> {
    recipe.validate()?;
    let release = &target.release;
    let block = release.version_block.ok_or(RebuildError::NoVersionBlock)?;
    let pin = |field, recipe: &str, pinned: &str| {
        let same = if field == "target.release" {
            recipe == pinned
        } else {
            recipe.eq_ignore_ascii_case(pinned)
        };
        if same {
            Ok(())
        } else {
            Err(RecipeError::TargetMismatch {
                field,
                release: target.id.to_owned(),
                recipe: recipe.to_owned(),
                pinned: pinned.to_owned(),
            })
        }
    };
    pin("target.release", &recipe.target.release, target.id)?;
    pin(
        "target.upd_sha256",
        &recipe.target.upd_sha256,
        release.upd_sha256,
    )?;
    pin(
        "target.application_sha256",
        &recipe.target.application_sha256,
        block.stock_application_sha256,
    )?;
    let stock_label = format!("Ver{}", block.stock_version);
    if !is_label_higher(&recipe.label, &stock_label)? {
        return Err(RecipeError::LabelNotHigher {
            label: recipe.label.clone(),
            stock: block.stock_version.to_owned(),
        });
    }
    block.validate_reported_version(&recipe.reported_version)?;
    for (index, replacement) in recipe.replacements.iter().enumerate() {
        let start = replacement.offset;
        // `validate` has checked that this cannot overflow.
        let end = start.saturating_add(replacement.len());
        let overlap = |protected: &&Range<usize>| {
            start < protected.end as u64 && (protected.start as u64) < end
        };
        if let Some(protected) = target.protected.iter().find(overlap) {
            return Err(RecipeError::Protected {
                index,
                start,
                end,
                protected_start: protected.start,
                protected_end: protected.end,
            });
        }
    }

    Ok(())
}

/// Applies `recipe` to `input` for an explicit `target` (tests pass synthetic targets; production
/// code uses [`apply_recipe_v2`]).
///
/// # Errors
///
/// Every refusal listed on [`RecipeError`], checked in the order the module docs describe.
pub fn apply_recipe_v2_to(
    recipe: &RecipeV2,
    target: &RecipeTarget<'_>,
    input: &[u8],
) -> Result<RebuiltUpdate, RecipeError> {
    check_recipe_v2(recipe, target)?;
    let release = &target.release;
    let block = release.version_block.ok_or(RebuildError::NoVersionBlock)?;
    let rebuilt =
        rebuild_with_edited_stock_application(input, release, &recipe.label, |decoded| {
            let stock = decoded.to_vec();
            // Every precondition is checked on the stock application before anything changes,
            // since windows may overlap other replacements.
            for (index, replacement) in recipe.replacements.iter().enumerate() {
                let precondition = &replacement.precondition;
                let window = window(index, replacement, stock.len())?;
                let actual = sha256_hex(&stock[window]);
                if !actual.eq_ignore_ascii_case(&precondition.sha256) {
                    return Err(RecipeError::Precondition {
                        index,
                        expected: precondition.sha256.clone(),
                        actual,
                    });
                }
            }
            // The version string, then every replaced span.
            let mut declared = Vec::with_capacity(recipe.replacements.len() + 1);
            declared.push(block.offset..block.offset + VERSION_FIELD_TEXT_LEN);
            for replacement in &recipe.replacements {
                // Inside its precondition window, which is inside the application.
                let start = usize::try_from(replacement.offset).expect("inside the window");
                let bytes = replacement.bytes().expect("validated hex");
                let span = start..start + bytes.len();
                decoded[span.clone()].copy_from_slice(&bytes);
                declared.push(span);
            }
            block.set_reported_version(decoded, &recipe.reported_version)?;
            verify_bounded_diff(&stock, decoded, &declared).map_err(RecipeError::UndeclaredChange)
        })?;

    if let Some(expected) = &recipe.expected {
        let pairs = [
            (
                "application_sha256",
                &expected.application_sha256,
                rebuilt.application_sha256(),
            ),
            ("upd_sha256", &expected.upd_sha256, rebuilt.sha256()),
        ];
        for (field, expected, actual) in pairs {
            if let Some(expected) = expected
                && !expected.eq_ignore_ascii_case(actual)
            {
                return Err(RecipeError::UnexpectedOutput {
                    field,
                    expected: expected.clone(),
                    actual: actual.to_owned(),
                });
            }
        }
    }
    Ok(rebuilt)
}

/// The four characters of `X.YY`; the NUL after them never changes.
const VERSION_FIELD_TEXT_LEN: usize = 4;

/// The replacement's precondition window, if it lies inside the application.
fn window(
    index: usize,
    replacement: &Replacement,
    application_len: usize,
) -> Result<Range<usize>, RecipeError> {
    // `validate` has checked that the window neither starts before 0 nor overflows.
    let Range { start: offset, end } = replacement
        .precondition_window()
        .unwrap_or(u64::MAX..u64::MAX);
    let out_of_bounds = || RecipeError::OutOfBounds {
        index,
        end,
        len: application_len,
    };
    let start = usize::try_from(offset).map_err(|_| out_of_bounds())?;
    let end_usize = usize::try_from(end).map_err(|_| out_of_bounds())?;
    if end_usize > application_len {
        return Err(out_of_bounds());
    }
    Ok(start..end_usize)
}
