//! Precondition windows of schema-v2 recipes: where a window lies, and whether the recipe would
//! reveal stock bytes, through the window's hash or through `bytes_hex`.
//!
//! A window's hash covers the span's stock bytes too, but those may follow from the replacement (a
//! flipped bit, a changed condition), so only the bytes outside the span count: `validate` checks
//! that there are at least [`MIN_PRECONDITION_LEN`] of them, and here they must not be dominated
//! by a few byte values. A span must change its first and last bytes, and may keep at most half
//! of its stock bytes, fewer than [`MIN_PRECONDITION_LEN`] in a row: `bytes_hex` publishes them,
//! and a longer unchanged stretch belongs between two spans, where it can be window bytes instead.
//!
//! These are heuristic guards against accidental leaks, not a proof: windows belong over code, not
//! over strings or tables, and review is the backstop. They hold for one recipe; the windows of
//! all committed recipes are checked to be disjoint by the `committed_recipes` test.

use super::recipe::RecipeError;
use patch_schema::{MIN_PRECONDITION_LEN, Replacement};
use std::ops::Range;

/// How many of the most common byte values may together fill at most half of the bytes around a
/// span.
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
/// equal stock and fewer than [`MIN_PRECONDITION_LEN`] in a row, and the window bytes outside the
/// span are not dominated by a few values.
pub(super) fn check_leaks(
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
    let (mut unchanged, mut run, mut longest_run) = (0, 0, 0);
    for (stock_byte, byte) in stock_span.iter().zip(&bytes) {
        run = if stock_byte == byte { run + 1 } else { 0 };
        unchanged += usize::from(run > 0);
        longest_run = longest_run.max(run);
    }
    if unchanged * 2 > bytes.len() || longest_run >= MIN_PRECONDITION_LEN as usize {
        return Err(RecipeError::UnchangedSpanBytes {
            index,
            unchanged,
            longest_run,
            len: bytes.len(),
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
