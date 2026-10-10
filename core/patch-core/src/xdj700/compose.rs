//! Composes several schema-v2 recipes ("fragments") into one rebuild, with one label and one
//! reported version (`docs/modular-builds.md`, "Building a profile").
//!
//! Before the input is used ([`check_composition`], which needs no firmware), every fragment has
//! its own `recipe_id`, passes [`check_recipe_v2`] and the caller's checks and pins its output
//! application; the composition's label and reported version pass the same rules as a recipe's;
//! precondition windows are disjoint across fragments except for exact repeats
//! ([`check_windows_across`]), and edited images are disjoint unless they are the same edit. Then
//! ([`CheckedComposition::compose`]):
//!
//! 1. each fragment is applied alone to the official update and must reproduce its own pinned
//!    output;
//! 2. every distinct replacement and image edit is rebuilt at once (each window checked on stock
//!    again, and the rebuild's bounded diff and verification);
//! 3. the composed application must equal, at every byte a fragment changes, that fragment's own
//!    verified output, and stock everywhere else, apart from the version string
//!    ([`check_composed`]).
//!
//! A composed build has no pin of its own: it is checked through its fragments'. A combination is
//! a new update that needs its own rehearsal; which combinations are offered, and at which tier,
//! is decided outside the engine.

pub use super::compose_check::check_composed;
use super::compose_check::{changed_bytes, check_changes};
pub use super::compose_error::ComposeError;
use super::output::OutputIdentities;
use super::precondition::DeclaredHash;
use super::rebuild::RebuiltUpdate;
use super::recipe::{
    RecipeTarget, check_label_and_version, check_output_pins, check_recipe_v2, rebuild_recipe,
    recipe_target, unknown_release,
};
use super::recipe_checks::RecipeChecks;
use super::{APPLICATION_SECTION_OFFSET, decode_section, main_document};
use crate::upd::parse_upd;
use patch_schema::{ImageEdit, RecipeV2, Replacement, check_windows_across};
use std::ops::Range;

/// The label and the reported version of a composed build.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Composition<'a> {
    /// MAIN version label of the output (`VerX.YY`), higher than the release's own version.
    pub label: &'a str,
    /// Version the composed application reports (`X.YY`), lower than the release's own version.
    pub reported_version: &'a str,
}

/// A composed update, and the identities each fragment produced alone (in fragment order), each
/// matching that fragment's pins.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedUpdate {
    pub rebuilt: RebuiltUpdate,
    pub fragments: Vec<OutputIdentities>,
}

/// Fragments that passed [`check_composition`], and the recipe of their composed build.
#[derive(Debug, Clone)]
pub struct CheckedComposition<'a> {
    fragments: &'a [RecipeV2],
    target: &'a RecipeTarget<'a>,
    composed: RecipeV2,
}

/// Composes `fragments` for the release they name, from `input`, its official update.
///
/// # Errors
///
/// [`ComposeError::NoFragments`], a [`ComposeError::Composition`] for an unknown release, or any
/// [`compose_recipes_to`] error.
pub fn compose_recipes(
    fragments: &[RecipeV2],
    composition: Composition<'_>,
    input: &[u8],
    checks: RecipeChecks<'_>,
) -> Result<ComposedUpdate, ComposeError> {
    let first = fragments.first().ok_or(ComposeError::NoFragments)?;
    let target = recipe_target(&first.target.release)
        .ok_or_else(|| ComposeError::Composition(unknown_release(&first.target.release)))?;
    compose_recipes_to(fragments, target, composition, input, checks)
}

/// Composes `fragments` for an explicit `target` (tests pass synthetic targets):
/// [`check_composition`], then [`CheckedComposition::compose`].
///
/// # Errors
///
/// Every refusal listed on [`ComposeError`]. A fragment for another release is refused by its own
/// checks ([`RecipeError::TargetMismatch`](super::RecipeError::TargetMismatch)).
pub fn compose_recipes_to(
    fragments: &[RecipeV2],
    target: &RecipeTarget<'_>,
    composition: Composition<'_>,
    input: &[u8],
    checks: RecipeChecks<'_>,
) -> Result<ComposedUpdate, ComposeError> {
    check_composition(fragments, target, composition, checks)?.compose(input)
}

/// The checks of a composition that need no firmware, so that a caller can run them before it
/// reads the input: every fragment has its own `recipe_id`, passes [`check_recipe_v2`] and
/// `checks` and pins its output application; precondition windows are disjoint across fragments
/// except for exact repeats ([`check_windows_across`]); edited images are disjoint unless they are
/// the same edit; and the composition's label and reported version pass the same rules as a
/// recipe's.
///
/// # Errors
///
/// [`ComposeError::NoFragments`], [`ComposeError::Duplicate`], [`ComposeError::Fragment`],
/// [`ComposeError::Unpinned`], [`ComposeError::Overlap`], [`ComposeError::ImageOverlap`] or
/// [`ComposeError::Composition`].
pub fn check_composition<'a>(
    fragments: &'a [RecipeV2],
    target: &'a RecipeTarget<'a>,
    composition: Composition<'_>,
    checks: RecipeChecks<'_>,
) -> Result<CheckedComposition<'a>, ComposeError> {
    let first = fragments.first().ok_or(ComposeError::NoFragments)?;
    for (second, fragment) in fragments.iter().enumerate() {
        let id = &fragment.recipe_id;
        if let Some(first) = fragments[..second].iter().position(|f| &f.recipe_id == id) {
            return Err(ComposeError::Duplicate {
                first,
                second,
                recipe_id: id.clone(),
            });
        }
    }
    for (index, fragment) in fragments.iter().enumerate() {
        let refused = |source| fragment_error(index, fragment, source);
        check_recipe_v2(fragment, target).map_err(refused)?;
        checks.run(fragment, target).map_err(refused)?;
        let pinned = fragment.expected.as_ref();
        if pinned.and_then(|e| e.application_sha256.as_ref()).is_none() {
            return Err(ComposeError::Unpinned {
                index,
                recipe_id: fragment.recipe_id.clone(),
            });
        }
    }
    check_windows_across(fragments)?;
    check_images_across(fragments)?;
    check_label_and_version(composition.label, composition.reported_version, target)
        .map_err(ComposeError::Composition)?;
    Ok(CheckedComposition {
        fragments,
        target,
        composed: composed_recipe(first, fragments, composition),
    })
}

impl CheckedComposition<'_> {
    /// Builds the composition from `input`, its official update, in the order the module docs
    /// describe. The fragments' own checks are not run again.
    ///
    /// # Errors
    ///
    /// A [`ComposeError::Fragment`] for an input that is not the pinned file or a fragment that
    /// does not reproduce its pins, a [`ComposeError::Composition`] for the composed rebuild, or
    /// a refusal of [`check_composed`].
    pub fn compose(&self, input: &[u8]) -> Result<ComposedUpdate, ComposeError> {
        let version = self
            .target
            .release
            .version_block
            .expect("check_label_and_version requires a version block")
            .text_range();
        let mut identities = Vec::with_capacity(self.fragments.len());
        let mut changes = Vec::with_capacity(self.fragments.len());
        let mut stock = None;
        for (index, fragment) in self.fragments.iter().enumerate() {
            let rebuilt = rebuild_recipe(fragment, self.target, input, DeclaredHash::Compare)
                .and_then(|(rebuilt, _)| check_output_pins(fragment, rebuilt))
                .map_err(|source| fragment_error(index, fragment, source))?;
            // The rebuild accepted the input as the pinned file: its application decodes.
            if stock.is_none() {
                stock = Some(decoded_application(input)?);
            }
            let stock = stock.as_deref().expect("decoded above");
            identities.push(OutputIdentities::of(&rebuilt));
            // Only the changed bytes are kept, so one decoded output is held at a time.
            let output = decoded_application(rebuilt.bytes())?;
            changes.push(changed_bytes(stock, &output, &version)?);
        }
        let stock = stock.expect("at least one fragment");
        // Every part was checked within its fragment, and the parts are disjoint across fragments
        // (exact repeats and same edits kept once), so the composed recipe holds the invariants
        // `rebuild_recipe` relies on.
        let (rebuilt, _) =
            rebuild_recipe(&self.composed, self.target, input, DeclaredHash::Compare)
                .map_err(ComposeError::Composition)?;
        let named: Vec<(&str, &[(usize, u8)])> = self
            .fragments
            .iter()
            .zip(&changes)
            .map(|(fragment, changed)| (fragment.recipe_id.as_str(), changed.as_slice()))
            .collect();
        let composed = decoded_application(rebuilt.bytes())?;
        check_changes(&stock, &named, &composed, &version)?;
        Ok(ComposedUpdate {
            rebuilt,
            fragments: identities,
        })
    }
}

fn fragment_error(index: usize, fragment: &RecipeV2, source: super::RecipeError) -> ComposeError {
    ComposeError::Fragment {
        index,
        recipe_id: fragment.recipe_id.clone(),
        source,
    }
}

/// Refuses images of different fragments that overlap, unless they are the same edit.
fn check_images_across(fragments: &[RecipeV2]) -> Result<(), ComposeError> {
    let images: Vec<(usize, String, &ImageEdit, Range<u64>)> = fragments
        .iter()
        .enumerate()
        .flat_map(|(position, fragment)| {
            fragment
                .image_edits
                .iter()
                .enumerate()
                .map(move |(index, edit)| {
                    let name = format!("{} image_edits[{index}]", fragment.recipe_id);
                    (position, name, edit, edit.window().expect("validated"))
                })
        })
        .collect();
    for (at, (first_recipe, first, a, a_window)) in images.iter().enumerate() {
        for (second_recipe, second, b, b_window) in &images[at + 1..] {
            let overlap = a_window.start < b_window.end && b_window.start < a_window.end;
            if overlap && !a.same_edit(b) {
                return Err(ComposeError::ImageOverlap {
                    first: first.clone(),
                    first_recipe: *first_recipe,
                    second: second.clone(),
                    second_recipe: *second_recipe,
                });
            }
        }
    }
    Ok(())
}

/// Every distinct replacement and image edit of `fragments`, sorted by offset, under the
/// composition's label and reported version. It declares no output identity: a composed build is
/// checked through its fragments' pins.
fn composed_recipe(
    first: &RecipeV2,
    fragments: &[RecipeV2],
    composition: Composition<'_>,
) -> RecipeV2 {
    let mut replacements: Vec<Replacement> = Vec::new();
    for replacement in fragments.iter().flat_map(|f| &f.replacements) {
        if !replacements.iter().any(|kept| kept.repeats(replacement)) {
            replacements.push(replacement.clone());
        }
    }
    replacements.sort_by_key(|replacement| replacement.offset);
    let mut image_edits: Vec<ImageEdit> = Vec::new();
    for edit in fragments.iter().flat_map(|f| &f.image_edits) {
        if !image_edits.iter().any(|kept| kept.same_edit(edit)) {
            image_edits.push(edit.clone());
        }
    }
    image_edits.sort_by_key(|edit| edit.offset);
    let ids: Vec<&str> = fragments.iter().map(|f| f.recipe_id.as_str()).collect();
    RecipeV2 {
        recipe_id: format!("composed: {}", ids.join(" + ")),
        description: "Recipes composed into one build".to_owned(),
        label: composition.label.to_owned(),
        reported_version: composition.reported_version.to_owned(),
        replacements,
        image_edits,
        expected: None,
        ..first.clone()
    }
}

/// The decoded application of the update `bytes`, read back from the bytes themselves rather than
/// taken from the rebuild. The section is decoded directly, as for any rebuild output: the loader
/// region is the input's.
fn decoded_application(bytes: &[u8]) -> Result<Vec<u8>, ComposeError> {
    let parsed = parse_upd(bytes)?;
    let image = main_document(&parsed)?.image()?;
    Ok(decode_section(image.bytes(), APPLICATION_SECTION_OFFSET)?.into_decoded())
}
