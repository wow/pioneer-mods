//! Composes several schema-v2 recipes ("fragments") into one rebuild, with one label and one
//! reported version (`docs/modular-builds.md`, "Building a profile").
//!
//! Before the input is used ([`check_composition`], which needs no firmware), each fragment passes
//! [`check_recipe_v2`] and the caller's checks and pins its output application; the composition's
//! label and reported version pass the same rules as a recipe's; precondition windows are disjoint
//! across fragments except for exact repeats ([`check_windows_across`]), and edited images are
//! disjoint unless they are the same edit. Then:
//!
//! 1. each fragment is applied alone to the official update and must reproduce its own pinned
//!    output ([`apply_recipe_v2_to`]);
//! 2. every distinct replacement and image edit is rebuilt at once (each window checked on stock
//!    again, and the rebuild's bounded diff and verification);
//! 3. the composed application must equal, at every byte a fragment changes, that fragment's own
//!    verified output, and stock everywhere else, apart from the version string.
//!
//! A composed build has no pin of its own: it is checked through its fragments'. A combination is
//! a new update that needs its own rehearsal; which combinations are offered, and at which tier,
//! is decided outside the engine.

pub use super::compose_error::ComposeError;
use super::output::OutputIdentities;
use super::precondition::DeclaredHash;
use super::rebuild::RebuiltUpdate;
use super::recipe::{
    RecipeTarget, apply_recipe_v2_to, check_recipe_v2, rebuild_recipe, recipe_target,
    unknown_release,
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

/// Composes `fragments` for an explicit `target` (tests pass synthetic targets), in the order the
/// module docs describe: first every [`check_composition`] check, then the input.
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
    let composed = composed_and_checked(fragments, target, composition, checks)?;
    let mut identities = Vec::with_capacity(fragments.len());
    let mut outputs = Vec::with_capacity(fragments.len());
    for (index, fragment) in fragments.iter().enumerate() {
        let rebuilt = apply_recipe_v2_to(fragment, target, input, checks)
            .map_err(|source| fragment_error(index, fragment, source))?;
        identities.push(OutputIdentities::of(&rebuilt));
        outputs.push(decoded_application(rebuilt.bytes())?);
    }
    // Every part was checked within its fragment, and the parts are disjoint across fragments
    // (exact repeats and same edits kept once), so the composed recipe holds the invariants
    // `rebuild_recipe` relies on.
    let (rebuilt, _) = rebuild_recipe(&composed, target, input, DeclaredHash::Compare)
        .map_err(ComposeError::Composition)?;
    let version = target
        .release
        .version_block
        .expect("check_recipe_v2 requires a version block")
        .text_range();
    let named: Vec<(&str, &[u8])> = fragments
        .iter()
        .zip(&outputs)
        .map(|(fragment, output)| (fragment.recipe_id.as_str(), output.as_slice()))
        .collect();
    check_composed(
        &decoded_application(input)?,
        &named,
        &decoded_application(rebuilt.bytes())?,
        version,
    )?;
    Ok(ComposedUpdate {
        rebuilt,
        fragments: identities,
    })
}

/// The checks of a composition that need no firmware, so that a caller can run them before it
/// reads the input: each fragment passes [`check_recipe_v2`] and `checks` and pins its output
/// application; precondition windows are disjoint across fragments except for exact repeats
/// ([`check_windows_across`]); edited images are disjoint unless they are the same edit; and the
/// composition's label and reported version pass the same rules as a recipe's.
/// [`compose_recipes_to`] runs them again first.
///
/// # Errors
///
/// [`ComposeError::NoFragments`], [`ComposeError::Fragment`], [`ComposeError::Unpinned`],
/// [`ComposeError::Overlap`], [`ComposeError::ImageOverlap`] or [`ComposeError::Composition`].
pub fn check_composition(
    fragments: &[RecipeV2],
    target: &RecipeTarget<'_>,
    composition: Composition<'_>,
    checks: RecipeChecks<'_>,
) -> Result<(), ComposeError> {
    composed_and_checked(fragments, target, composition, checks).map(|_| ())
}

/// The [`check_composition`] checks, then the composed recipe.
fn composed_and_checked(
    fragments: &[RecipeV2],
    target: &RecipeTarget<'_>,
    composition: Composition<'_>,
    checks: RecipeChecks<'_>,
) -> Result<RecipeV2, ComposeError> {
    let first = fragments.first().ok_or(ComposeError::NoFragments)?;
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
    let composed = composed_recipe(first, fragments, composition);
    // The label, the reported version and the pins, under the same rules as a recipe's: a recipe
    // with the composition's label and version and no changes of its own.
    let header = RecipeV2 {
        replacements: Vec::new(),
        image_edits: Vec::new(),
        ..composed.clone()
    };
    check_recipe_v2(&header, target).map_err(ComposeError::Composition)?;
    Ok(composed)
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
    let images: Vec<(String, &ImageEdit, Range<u64>)> = fragments
        .iter()
        .flat_map(|fragment| {
            fragment
                .image_edits
                .iter()
                .enumerate()
                .map(|(index, edit)| {
                    let name = format!("{} image_edits[{index}]", fragment.recipe_id);
                    (name, edit, edit.window().expect("validated"))
                })
        })
        .collect();
    for (at, (first, a, a_window)) in images.iter().enumerate() {
        for (second, b, b_window) in &images[at + 1..] {
            let overlap = a_window.start < b_window.end && b_window.start < a_window.end;
            if overlap && !a.same_edit(b) {
                return Err(ComposeError::ImageOverlap {
                    first: first.clone(),
                    second: second.clone(),
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

/// The decoded application of the update `bytes`. The section is decoded directly, as for any
/// rebuild output: the loader region is the input's.
fn decoded_application(bytes: &[u8]) -> Result<Vec<u8>, ComposeError> {
    let parsed = parse_upd(bytes)?;
    let image = main_document(&parsed)?.image()?;
    Ok(decode_section(image.bytes(), APPLICATION_SECTION_OFFSET)?.into_decoded())
}

/// Checks that the decoded application `composed` equals, at every byte a fragment changes, that
/// fragment's own decoded output, and `stock` everywhere else, outside `version` (the version
/// string, which the composition sets). `outputs` pairs each fragment's name with its output.
///
/// # Errors
///
/// [`ComposeError::Disagree`] when two outputs change one byte differently, or
/// [`ComposeError::Mismatch`] at the first byte (or length) `composed` gets wrong.
pub fn check_composed(
    stock: &[u8],
    outputs: &[(&str, &[u8])],
    composed: &[u8],
    version: Range<usize>,
) -> Result<(), ComposeError> {
    let lengths = std::iter::once(composed.len()).chain(outputs.iter().map(|(_, o)| o.len()));
    if let Some(length) = lengths.filter(|&length| length != stock.len()).min() {
        return Err(ComposeError::Mismatch {
            offset: length.min(stock.len()),
            expected: "stock (the application's length)".to_owned(),
        });
    }
    for (offset, (&stock_byte, &composed_byte)) in stock.iter().zip(composed).enumerate() {
        if version.contains(&offset) {
            continue;
        }
        let mut changed_by: Option<(&str, u8)> = None;
        for &(name, output) in outputs {
            let byte = output[offset];
            if byte == stock_byte {
                continue;
            }
            match changed_by {
                Some((first, first_byte)) if first_byte != byte => {
                    return Err(ComposeError::Disagree {
                        offset,
                        first: first.to_owned(),
                        second: name.to_owned(),
                    });
                }
                Some(_) => {}
                None => changed_by = Some((name, byte)),
            }
        }
        let expected = changed_by.map_or(stock_byte, |(_, byte)| byte);
        if composed_byte != expected {
            return Err(ComposeError::Mismatch {
                offset,
                expected: changed_by
                    .map_or("stock".to_owned(), |(name, _)| format!("{name}'s output")),
            });
        }
    }
    Ok(())
}
