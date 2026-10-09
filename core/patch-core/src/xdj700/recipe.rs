//! Applies a schema-v2 recipe ([`RecipeV2`]): changes to the decoded application of a pinned
//! release, rebuilt into a complete update through [`rebuild_with_edited_stock_application`].
//!
//! Before the input is read, the recipe must pass its static checks, name a known release whose
//! pins it repeats exactly, declare a label higher and a reported version lower than the release's
//! own version, and keep every replacement, its precondition window and every edited image inside
//! the application and out of the release's protected ranges. On the stock application, before
//! anything changes, each precondition window (defined relative to its offset, so a wrong offset
//! moves the window) must match its declared SHA-256, and the recipe must not reveal the stock
//! bytes in it (the `precondition` module). An edited image has no hash of its own (the
//! `image_edit` module): a recipe with image edits must pin its output application instead. The
//! output application may differ from stock only in the declared replacements, the edited image
//! rows and the version string (checked by the rebuild entry point), the rebuild then runs its own
//! verification (including the version rule), and any declared output identities must match.

use super::app_version::VERSION_TEXT_LEN;
use super::image_edit::{apply_image_edit, image_range};
use super::is_label_higher;
use super::output::OutputIdentities;
use super::precondition::{DeclaredHash, checked_window};
use super::rebuild::{RebuiltUpdate, rebuild_with_edited_stock_application};
use super::recipe_checks::{RecipeChecks, check_ranges};
pub use super::recipe_error::RecipeError;
use super::release::{OFFICIAL_V115, StockRelease};
use crate::error::RebuildError;
use crate::identity::sha256_hex;
use patch_schema::{RecipeV2, WindowOwner};
use std::ops::Range;

/// A release recipes may target: its identifier, its pins, the length of its stock decoded
/// application, and the decoded-application ranges no replacement or precondition may touch.
///
/// The fields are public so that tests can use synthetic targets. Like [`StockRelease`], the
/// safety guarantees hold only for the pinned constants in [`RECIPE_TARGETS`]; production code
/// must never construct a target from user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecipeTarget<'a> {
    /// The identifier recipes name in `target.release`.
    pub id: &'a str,
    pub release: StockRelease<'a>,
    /// Length of the stock decoded application (fixed by its pinned SHA-256), so that every
    /// window is bounds-checked before the input is read.
    pub application_len: usize,
    /// The run-time address the decoded application is linked at: a decoded offset `o` runs at
    /// `load_address + o`. A protected set ([`ProtectedSet`](super::ProtectedSet)) uses run-time
    /// addresses.
    pub load_address: u64,
    /// Ranges of the decoded application that replacements, their precondition windows and edited
    /// images may not overlap (they hold known strings, which would make a window's hash
    /// invertible).
    pub protected: &'a [Range<usize>],
}

/// The releases recipes may target.
///
/// v1.15 protects `[0, 0x800)`: the application header and its version block (the version changes
/// only through `reported_version`). The entry point is at `0x800`, but code below it also runs at
/// start-up (the set measured in emulation starts at `0x600`), so this range must not be narrowed
/// to the version block. Keeping recipes out of the code that start-up and the update path run is
/// checked against a protected set kept outside the repository, which the entry points take in
/// [`RecipeChecks`] (see [`super::check_recipe_against_protected_set`]); keeping them out of the
/// data that code reads is not checked by the tool. The rules are in
/// `docs/xdj700-flashing.md`, section 5. The application is linked at `0x0800_0000` (its entry
/// point runs at `0x0800_0800`).
pub const RECIPE_TARGETS: &[RecipeTarget<'static>] = &[RecipeTarget {
    id: "xdj700-v1.15",
    release: OFFICIAL_V115,
    application_len: 18_601_864,
    load_address: 0x0800_0000,
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
            covered |=
                protected.start <= block.offset && block.offset + VERSION_TEXT_LEN < protected.end;
            range += 1;
        }
        assert!(covered, "a protected range must cover the version string");
        index += 1;
    }
};

/// Applies `recipe` to `input`, the official update of the release the recipe names.
///
/// # Errors
///
/// [`RecipeError::UnknownRelease`], or any [`apply_recipe_v2_to`] error.
pub fn apply_recipe_v2(
    recipe: &RecipeV2,
    input: &[u8],
    checks: RecipeChecks<'_>,
) -> Result<RebuiltUpdate, RecipeError> {
    let target = recipe_target(&recipe.target.release)
        .ok_or_else(|| unknown_release(&recipe.target.release))?;
    apply_recipe_v2_to(recipe, target, input, checks)
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

/// Every check that needs no firmware: the static recipe checks (image edits and their output pin
/// included), the release id and pins, the label and reported-version order, and the bounds and
/// protected ranges of every replacement, precondition window and edited image.
/// [`apply_recipe_v2_to`] runs it first; front ends can run it before reading the input.
///
/// # Errors
///
/// [`RecipeError::Invalid`] (also for a malformed image edit), [`RecipeError::TargetMismatch`],
/// [`RecipeError::LabelNotHigher`], [`RecipeError::OutOfBounds`],
/// [`RecipeError::ImageOutOfBounds`], [`RecipeError::Protected`], [`RecipeError::ImageProtected`],
/// or a [`RecipeError::Rebuild`] for a release without a version block or a reported version that
/// is not lower.
pub fn check_recipe_v2(recipe: &RecipeV2, target: &RecipeTarget<'_>) -> Result<(), RecipeError> {
    recipe.validate()?;
    let release = &target.release;
    let block = release.version_block.ok_or(RebuildError::NoVersionBlock)?;
    let mismatch = |field, recipe: &str, pinned: &str| RecipeError::TargetMismatch {
        field,
        release: target.id.to_owned(),
        recipe: recipe.to_owned(),
        pinned: pinned.to_owned(),
    };
    // An identifier, compared exactly.
    if recipe.target.release != target.id {
        return Err(mismatch(
            "target.release",
            &recipe.target.release,
            target.id,
        ));
    }
    // SHA-256 pins, compared without regard to case.
    let pins = [
        (
            "target.upd_sha256",
            &recipe.target.upd_sha256,
            release.upd_sha256,
        ),
        (
            "target.application_sha256",
            &recipe.target.application_sha256,
            block.stock_application_sha256,
        ),
    ];
    for (field, recipe_value, pinned) in pins {
        if !recipe_value.eq_ignore_ascii_case(pinned) {
            return Err(mismatch(field, recipe_value, pinned));
        }
    }
    let stock_label = format!("Ver{}", block.stock_version);
    if !is_label_higher(&recipe.label, &stock_label)? {
        return Err(RecipeError::LabelNotHigher {
            label: recipe.label.clone(),
            stock: block.stock_version.to_owned(),
        });
    }
    block.validate_reported_version(&recipe.reported_version)?;
    check_ranges(recipe, target, target.protected, |found| {
        match found.owner {
            WindowOwner::Replacement(index) => RecipeError::Protected {
                index,
                what: found.what,
                start: found.start,
                end: found.end,
                protected_start: found.protected.start,
                protected_end: found.protected.end,
            },
            WindowOwner::ImageEdit(index) => RecipeError::ImageProtected {
                index,
                start: found.start,
                end: found.end,
                protected_start: found.protected.start,
                protected_end: found.protected.end,
            },
        }
    })
}

/// Applies `recipe` to `input` for an explicit `target` (tests pass synthetic targets; production
/// code uses [`apply_recipe_v2`] or a target from [`recipe_target`]), after the built-in checks and
/// those in `checks`.
///
/// # Errors
///
/// Every refusal listed on [`RecipeError`], checked in the order the module docs describe.
pub fn apply_recipe_v2_to(
    recipe: &RecipeV2,
    target: &RecipeTarget<'_>,
    input: &[u8],
    checks: RecipeChecks<'_>,
) -> Result<RebuiltUpdate, RecipeError> {
    check_recipe_v2(recipe, target)?;
    checks.run(recipe, target)?;
    let (rebuilt, _) = rebuild_recipe(recipe, target, input, DeclaredHash::Compare)?;
    let output = OutputIdentities::of(&rebuilt);
    let pins = output.pins(recipe.expected.as_ref());
    if let Some(pin) = pins.iter().find(|pin| pin.differs()) {
        return Err(RecipeError::UnexpectedOutput {
            field: pin.field,
            expected: pin
                .declared
                .expect("a differing pin is declared")
                .to_owned(),
            actual: pin.actual.to_owned(),
        });
    }
    Ok(rebuilt)
}

/// Rebuilds `input` with the changes of `recipe`, which has passed [`check_recipe_v2`] and the
/// caller's checks, and returns the update with the SHA-256 of each replacement's window on the
/// stock application, in recipe order. `declared` says whether each window is compared with its
/// declared hash ([`DeclaredHash::Ignore`] for a draft, whose hashes and output identities
/// [`super::precondition_hashes`] computes). The declared output identities are not compared here.
pub(super) fn rebuild_recipe(
    recipe: &RecipeV2,
    target: &RecipeTarget<'_>,
    input: &[u8],
    declared: DeclaredHash,
) -> Result<(RebuiltUpdate, Vec<String>), RecipeError> {
    let release = &target.release;
    let block = release.version_block.ok_or(RebuildError::NoVersionBlock)?;
    let mut window_hashes = Vec::with_capacity(recipe.replacements.len());
    let rebuilt =
        rebuild_with_edited_stock_application(input, release, &recipe.label, |decoded| {
            // Every window is checked (and hashed) on the stock application before anything
            // changes.
            for (index, replacement) in recipe.replacements.iter().enumerate() {
                let window = checked_window(index, replacement, decoded, declared)?;
                window_hashes.push(sha256_hex(&decoded[window]));
            }
            let mut images = Vec::with_capacity(recipe.image_edits.len());
            for (index, edit) in recipe.image_edits.iter().enumerate() {
                images.push(image_range(index, edit, decoded.len())?);
            }
            // The version string, then every run of written bytes, then every edited image row.
            let mut changed = Vec::with_capacity(recipe.replacements.len() + 1);
            changed.push(block.text_range());
            for replacement in &recipe.replacements {
                // Inside its precondition window, which is inside the application. Only written
                // bytes are declared, so the bounded diff also checks that kept (`--`) bytes stay
                // stock.
                let start = usize::try_from(replacement.offset).expect("inside the window");
                let pattern = replacement.pattern().expect("validated pattern");
                let mut run: Option<usize> = None;
                for (at, byte) in (start..).zip(&pattern) {
                    match (byte, run) {
                        (Some(byte), _) => {
                            decoded[at] = *byte;
                            run.get_or_insert(at);
                        }
                        (None, Some(from)) => {
                            changed.push(from..at);
                            run = None;
                        }
                        (None, None) => {}
                    }
                }
                // The last byte is written (`validate`), so a run is open here.
                changed.push(run.expect("written last byte")..start + pattern.len());
            }
            // Images are clear of the replacements' windows and of each other (`validate`), so
            // each still holds its stock pixels here.
            for (edit, image) in recipe.image_edits.iter().zip(images) {
                changed.extend(apply_image_edit(edit, image, decoded));
            }
            block.set_reported_version(decoded, &recipe.reported_version)?;
            Ok::<_, RecipeError>(changed)
        })?;
    Ok((rebuilt, window_hashes))
}
