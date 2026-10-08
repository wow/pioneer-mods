//! Precondition windows of schema-v2 recipes: where a window lies, and whether the recipe would
//! reveal the stock bytes in it.
//!
//! A window's hash covers stock bytes. Any of them the recipe itself publishes (a replacement byte
//! equal to stock) are known to every reader, so they do not count towards the window; the bytes
//! that remain must be at least [`MIN_PRECONDITION_LEN`] and not dominated by a few byte values.
//! A span must also change its first and last bytes, and at most half of its bytes may equal
//! stock, so that `bytes_hex` cannot carry stock code verbatim.
//!
//! These are heuristic guards against accidental leaks, not a proof: windows belong over code, not
//! over strings or tables, and review is the backstop. They hold for one recipe; the windows of
//! all committed recipes are checked to be disjoint by the `committed_recipes` test.

use super::recipe::RecipeError;
use patch_schema::{MIN_PRECONDITION_LEN, Replacement};
use std::ops::Range;

/// How many of the most common byte values may together fill at most half of the unpublished
/// bytes of a window.
const TOP_VALUES: usize = 4;

/// The replacement's precondition window, if it lies inside an application of `application_len`.
pub(super) fn window(
    index: usize,
    replacement: &Replacement,
    application_len: usize,
) -> Result<Range<usize>, RecipeError> {
    // `validate` has checked that the window neither starts before 0 nor overflows.
    let Range { start, end } = replacement
        .precondition_window()
        .unwrap_or(u64::MAX..u64::MAX);
    let out_of_bounds = || RecipeError::OutOfBounds {
        index,
        end,
        len: application_len,
    };
    let start = usize::try_from(start).map_err(|_| out_of_bounds())?;
    let end_usize = usize::try_from(end).map_err(|_| out_of_bounds())?;
    if end_usize > application_len {
        return Err(out_of_bounds());
    }
    Ok(start..end_usize)
}

/// On the stock application: the span's first and last bytes change, at most half of its bytes
/// equal stock, at least [`MIN_PRECONDITION_LEN`] window bytes are not published by the recipe,
/// and those bytes are not dominated by a few values.
pub(super) fn check_unpublished(
    index: usize,
    replacement: &Replacement,
    stock: &[u8],
    window: Range<usize>,
) -> Result<(), RecipeError> {
    let bytes = replacement.bytes().expect("validated hex");
    // Inside the window, which `window` has bounds-checked.
    let span_start = usize::try_from(replacement.offset).expect("inside the window");
    let span = span_start..span_start + bytes.len();
    let stock_span = &stock[span.clone()];
    if stock_span.first() == bytes.first() || stock_span.last() == bytes.last() {
        return Err(RecipeError::UnchangedSpanEdge { index });
    }
    let unchanged = stock_span
        .iter()
        .zip(&bytes)
        .filter(|(stock, replacement)| stock == replacement)
        .count();
    if unchanged * 2 > bytes.len() {
        return Err(RecipeError::MostlyUnchangedSpan {
            index,
            unchanged,
            len: bytes.len(),
        });
    }
    let unpublished: Vec<u8> = window
        .filter(|&offset| !span.contains(&offset) || stock[offset] != bytes[offset - span.start])
        .map(|offset| stock[offset])
        .collect();
    let min = MIN_PRECONDITION_LEN as usize;
    if unpublished.len() < min {
        return Err(RecipeError::TooFewUnpublishedBytes {
            index,
            unpublished: unpublished.len(),
            min,
        });
    }
    check_unpredictable(index, &unpublished)
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
