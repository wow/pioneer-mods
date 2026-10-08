//! Schema-v2 leak checks on synthetic releases: a recipe must not reveal stock bytes through a
//! window's hash (the bytes around the span must not be predictable) or through `bytes_hex` (a
//! span changes its edges and keeps few stock bytes).

mod common;

use common::recipe::{Fixture, PADDING, replacement, stock_application, windowed};
use patch_core::xdj700::RecipeError;
use patch_schema::Replacement;

#[test]
fn refuses_a_window_over_padding() {
    let fixture = Fixture::new();
    // 32 zero bytes before a 1-byte span leave only the 7 bytes after it unknown.
    let padded = windowed(PADDING.end, &[1], 32, 7);

    let result = fixture.apply(&fixture.recipe(vec![padded]));

    // The zero byte and the three next most common values: 32 + 3 of 39 bytes.
    assert_eq!(
        result,
        Err(RecipeError::PredictableWindow {
            index: 0,
            top: 4,
            count: 35,
            len: 39
        })
    );
}

#[test]
fn the_four_most_common_values_may_fill_exactly_half_of_the_bytes_around_a_span() {
    let fixture = Fixture::new();
    // 16 zero bytes before a 1-byte span at the end of the padding, distinct non-zero bytes after
    // it: zero and three other values fill 16 + 3 bytes, exactly half of 38, more than half of 37.
    let around = |after| windowed(PADDING.end, &[1], 16, after);

    assert!(fixture.apply(&fixture.recipe(vec![around(22)])).is_ok());
    assert_eq!(
        fixture.apply(&fixture.recipe(vec![around(21)])),
        Err(RecipeError::PredictableWindow {
            index: 0,
            top: 4,
            count: 19,
            len: 37
        })
    );
}

#[test]
fn only_the_bytes_around_the_span_are_judged() {
    let fixture = Fixture::new();
    let stock = stock_application();
    // 16 zero and 16 distinct bytes before a span of 16 distinct bytes: predictable around the
    // span (19 of 32), but not over the whole window (19 of 48).
    let span = PADDING.end + 16..PADDING.end + 32;
    let bytes: Vec<u8> = stock[span.clone()].iter().map(|byte| !byte).collect();

    let result = fixture.apply(&fixture.recipe(vec![windowed(span.start, &bytes, 32, 0)]));

    assert_eq!(
        result,
        Err(RecipeError::PredictableWindow {
            index: 0,
            top: 4,
            count: 19,
            len: 32
        })
    );
}

#[test]
fn refuses_a_span_whose_first_or_last_byte_is_unchanged() {
    let fixture = Fixture::new();
    let stock = stock_application();
    let (first, second) = (stock[0x900], stock[0x901]);

    for bytes in [
        vec![first, !second],
        vec![!first, second],
        stock[0x900..0x910].to_vec(),
    ] {
        assert_eq!(
            fixture.apply(&fixture.recipe(vec![replacement(0x900, &bytes)])),
            Err(RecipeError::UnchangedSpanEdge { index: 0 }),
            "{bytes:02x?}"
        );
    }
}

/// A span of `len` changed bytes at `0x900`, except the `kept` runs, `(start, len)` within the
/// span, which equal stock, with a window of 32 bytes before it.
fn keeping(len: usize, kept: &[(usize, usize)]) -> Replacement {
    let stock = stock_application();
    let span = 0x900..0x900 + len;
    let mut bytes: Vec<u8> = stock[span.clone()].iter().map(|byte| !byte).collect();
    for &(start, run) in kept {
        let stock_run = span.start + start..span.start + start + run;
        bytes[start..start + run].copy_from_slice(&stock[stock_run]);
    }
    windowed(span.start, &bytes, 32, 0)
}

fn kept_too_many(unchanged: usize, longest_run: usize, len: usize) -> Result<Vec<u8>, RecipeError> {
    Err(RecipeError::UnchangedSpanBytes {
        index: 0,
        unchanged,
        longest_run,
        len,
    })
}

#[test]
fn at_most_half_of_a_span_may_equal_stock() {
    let fixture = Fixture::new();
    let apply = |kept: &[_]| fixture.apply(&fixture.recipe(vec![keeping(32, kept)]));

    assert!(apply(&[(1, 16)]).is_ok(), "16 of 32");
    assert_eq!(apply(&[(1, 17)]), kept_too_many(17, 17, 32));
    assert_eq!(apply(&[(1, 9), (11, 9)]), kept_too_many(18, 9, 32));
}

#[test]
fn unchanged_runs_inside_a_span_are_shorter_than_a_window() {
    let fixture = Fixture::new();
    let apply = |kept: &[_]| fixture.apply(&fixture.recipe(vec![keeping(70, kept)]));

    assert!(apply(&[(1, 31)]).is_ok(), "31 in a row");
    assert_eq!(apply(&[(1, 32)]), kept_too_many(32, 32, 70));
}
