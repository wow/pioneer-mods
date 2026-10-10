//! Precondition windows of schema-v2 recipes: where a window lies, and whether the recipe would
//! reveal stock bytes, through the window's hash or through `bytes_hex`.
//!
//! A window's hash covers the span's stock bytes too, but those may follow from the replacement (a
//! flipped bit, a changed condition), so only the bytes outside the span count: `validate` checks
//! that there are at least [`MIN_PRECONDITION_LEN`] of them, and here they must not be dominated
//! by a few byte values. A span must change its first and last bytes, and of the bytes it writes at
//! most half may equal stock, fewer than [`MIN_PRECONDITION_LEN`] in a row: `bytes_hex` publishes
//! them, and a longer unchanged stretch belongs between two spans, where it can be window bytes
//! instead. A kept byte (`--`) is not published, so it does not count.
//!
//! These are heuristic guards against accidental leaks, not a proof: windows belong over code, not
//! over strings or tables, and review is the backstop. They hold for one recipe; the windows of
//! all committed recipes are checked to be disjoint, unless a recipe repeats another's
//! replacement exactly, by the `committed_recipes` test.
//!
//! [`precondition_hashes`] helps an author complete a draft recipe: it rebuilds the draft, hashing
//! each window only after these checks, so it never shows the hash of a window the engine would
//! refuse, and it computes the output identities that `expected` pins.

use super::output::OutputIdentities;
use super::recipe::{RecipeError, RecipeTarget, check_recipe_v2, rebuild_recipe};
use super::recipe_checks::{RecipeChecks, bounded, out_of_bounds};
use crate::identity::sha256_hex;
use patch_schema::{MIN_PRECONDITION_LEN, RecipeV2, Replacement, WindowOwner};
use std::ops::Range;

/// How many of the most common byte values may together fill at most half of the bytes around a
/// span.
const TOP_VALUES: usize = 4;

/// The SHA-256 of every precondition window of `recipe`, computed on `input`, the official update
/// of `target`, for an author completing a draft, and the identities of its output. The recipe
/// first passes [`check_recipe_v2`] and the checks in `checks`; it is then rebuilt, each window
/// passing its bounds and leak checks before its hash is computed. Nothing is returned unless the
/// whole rebuild succeeds. The hashes and identities the recipe declares are not compared (a draft
/// holds placeholders); applying the recipe compares them.
///
/// A hash covers whatever is at the declared offset, so it cannot show that the offset is the
/// intended one: check offsets against your own analysis first. It then catches later changes.
/// The same holds for an image's offset and the output identities.
///
/// # Errors
///
/// As [`check_recipe_v2`] and the checks in `checks`; [`RecipeError::OutOfBounds`] or a leak
/// check's error for a window; [`RecipeError::ImageOutOfBounds`] for an image;
/// [`RecipeError::Rebuild`] for an input that is not the release's official update, or for an
/// edited application the rebuild refuses (a change outside the declared spans and image rows, an
/// application that no longer fits, or a failed verification, including the version rule).
pub fn precondition_hashes(
    recipe: &RecipeV2,
    target: &RecipeTarget<'_>,
    input: &[u8],
    checks: RecipeChecks<'_>,
) -> Result<PreconditionHashes, RecipeError> {
    check_recipe_v2(recipe, target)?;
    checks.run(recipe, target)?;
    let (rebuilt, replacements) = rebuild_recipe(recipe, target, input, DeclaredHash::Ignore)?;
    Ok(PreconditionHashes {
        replacements,
        output: OutputIdentities::of(&rebuilt),
    })
}

/// What [`precondition_hashes`] computes: one hash per replacement window, in recipe order, and
/// the output identities.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreconditionHashes {
    pub replacements: Vec<String>,
    pub output: OutputIdentities,
}

/// Whether [`checked_window`] compares the window with the hash the recipe declares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum DeclaredHash {
    /// Applying a recipe: the window must hash to the declared SHA-256.
    Compare,
    /// Computing hashes for a draft, whose declared hashes are placeholders.
    Ignore,
}

/// The replacement's precondition window on the stock application, after every per-window rule:
/// inside the application, matching the declared hash when `declared` asks for it, and the leak
/// checks. Applying a recipe and [`precondition_hashes`] both go through it, so the hashes shown
/// to authors and the windows `patch` accepts cannot drift apart. The hash is compared first, so
/// a mistyped offset reads as a mismatch rather than as a leak rule; nothing is printed either
/// way.
pub(super) fn checked_window(
    index: usize,
    replacement: &Replacement,
    stock: &[u8],
    declared: DeclaredHash,
) -> Result<Range<usize>, RecipeError> {
    let range = window(index, replacement, stock.len())?;
    let expected = &replacement.precondition.sha256;
    if declared == DeclaredHash::Compare
        && !sha256_hex(&stock[range.clone()]).eq_ignore_ascii_case(expected)
    {
        return Err(RecipeError::Precondition {
            index,
            expected: expected.clone(),
        });
    }
    check_leaks(index, replacement, stock, range.clone())?;
    Ok(range)
}

/// The replacement's precondition window, if it lies inside an application of `application_len`.
pub(super) fn window(
    index: usize,
    replacement: &Replacement,
    application_len: usize,
) -> Result<Range<usize>, RecipeError> {
    bounded(
        replacement.precondition_window(),
        application_len,
        out_of_bounds(WindowOwner::Replacement(index), application_len),
    )
}

/// On the stock application: the span's first and last bytes change, of the bytes it writes at
/// most half equal stock and fewer than [`MIN_PRECONDITION_LEN`] in a row (a kept byte ends a
/// run), and the window bytes outside the span are not dominated by a few values.
fn check_leaks(
    index: usize,
    replacement: &Replacement,
    stock: &[u8],
    window: Range<usize>,
) -> Result<(), RecipeError> {
    let pattern = replacement.pattern().expect("validated pattern");
    // Inside the window, which `window` has bounds-checked.
    let span_start = usize::try_from(replacement.offset).expect("inside the window");
    let span = span_start..span_start + pattern.len();
    let stock_span = &stock[span.clone()];
    // `validate` requires the first and last bytes to be written.
    if stock_span.first().copied() == pattern[0]
        || stock_span.last().copied() == pattern[pattern.len() - 1]
    {
        return Err(RecipeError::UnchangedSpanEdge { index });
    }
    let (mut written, mut unchanged, mut run, mut longest_run) = (0, 0, 0, 0);
    for (&stock_byte, byte) in stock_span.iter().zip(&pattern) {
        written += usize::from(byte.is_some());
        run = if *byte == Some(stock_byte) {
            run + 1
        } else {
            0
        };
        unchanged += usize::from(run > 0);
        longest_run = longest_run.max(run);
    }
    if unchanged * 2 > written || longest_run >= MIN_PRECONDITION_LEN as usize {
        return Err(RecipeError::UnchangedSpanBytes {
            index,
            unchanged,
            longest_run,
            len: written,
        });
    }
    let outside: Vec<u8> = stock[window.start..span.start]
        .iter()
        .chain(&stock[span.end..window.end])
        .copied()
        .collect();
    check_unpredictable(index, &outside)
}

/// Refuses bytes in which the [`TOP_VALUES`] most common byte values fill more than half: padding,
/// fill, two-valued data, or 16-bit text and tables, whose hash would reveal the rest by brute
/// force.
fn check_unpredictable(index: usize, bytes: &[u8]) -> Result<(), RecipeError> {
    let mut counts = [0usize; 256];
    for &byte in bytes {
        counts[usize::from(byte)] += 1;
    }
    counts.sort_unstable_by(|a, b| b.cmp(a));
    let count: usize = counts[..TOP_VALUES].iter().sum();
    if count * 2 > bytes.len() {
        return Err(RecipeError::PredictableWindow {
            index,
            top: TOP_VALUES,
            count,
            len: bytes.len(),
        });
    }
    Ok(())
}
