//! Schema-v2 preconditions on synthetic releases: windows checked on stock, relative to their
//! span, inside the application, outside protected ranges, and not revealing stock bytes.

mod common;

use common::recipe::{Fixture, PADDING, replacement, stock_application};
use patch_core::sha256_hex;
use patch_core::xdj700::RecipeError;
use patch_schema::Replacement;

#[test]
fn refuses_a_replacement_whose_original_bytes_differ() {
    let fixture = Fixture::new();
    let mut wrong = replacement(0x900, &[0xde, 0xad]);
    wrong.precondition.sha256 = "00".repeat(32);

    let result = fixture.apply(&fixture.recipe(vec![wrong]));

    assert!(
        matches!(result, Err(RecipeError::Precondition { index: 0, .. })),
        "{result:?}"
    );
}

#[test]
fn refuses_a_replacement_past_the_application() {
    let fixture = Fixture::new();
    let mut past = replacement(0xff0, &[1, 2, 3, 4]);
    past.precondition.after += 1;

    let result = fixture.apply(&fixture.recipe(vec![past]));

    assert_eq!(
        result,
        Err(RecipeError::OutOfBounds {
            index: 0,
            end: 0x1001,
            len: 0x1000
        })
    );
    let exact_end = replacement(0xffc, &[1, 2, 3, 4]);
    assert_eq!(exact_end.precondition_window(), Some(0xfd8..0x1000));
    assert!(
        fixture.apply(&fixture.recipe(vec![exact_end])).is_ok(),
        "a window may end exactly at the application's end"
    );
}

#[test]
fn nearby_replacements_with_outward_windows_apply() {
    let fixture = Fixture::new();
    let stock = stock_application();
    // Spans at 0x900 and 0x904; the first window ends at its span, the second starts at its own.
    let mut first = replacement(0x900, &[0xde, 0xad]);
    first.precondition.before = 30;
    first.precondition.after = 0;
    first.precondition.sha256 = sha256_hex(&stock[0x8e2..0x902]);
    let mut second = replacement(0x904, &[0xbe, 0xef]);
    second.precondition.before = 0;
    second.precondition.after = 30;
    second.precondition.sha256 = sha256_hex(&stock[0x904..0x924]);

    let output = fixture
        .apply(&fixture.recipe(vec![first, second]))
        .expect("apply");

    assert_eq!(
        &output[0x900..0x906],
        &[0xde, 0xad, stock[0x902], stock[0x903], 0xbe, 0xef]
    );
}

#[test]
fn refuses_a_precondition_window_in_a_protected_range() {
    let fixture = Fixture::new();
    let mut reaching = replacement(0x808, &[1]);
    reaching.precondition.before = 16;
    reaching.precondition.after = 15;

    let result = fixture.apply(&fixture.recipe(vec![reaching]));

    assert!(
        matches!(
            result,
            Err(RecipeError::Protected {
                index: 0,
                what: "precondition window",
                ..
            })
        ),
        "{result:?}"
    );
}

#[test]
fn refuses_a_window_over_padding() {
    let fixture = Fixture::new();
    let stock = stock_application();
    // 32 zero bytes, then a 1-byte span: the hash would reveal that byte in 256 tries.
    let mut padded = replacement(PADDING.end, &[1]);
    padded.precondition.before = 32;
    padded.precondition.after = 7;
    padded.precondition.sha256 = sha256_hex(&stock[PADDING.end - 32..PADDING.end + 8]);

    let result = fixture.apply(&fixture.recipe(vec![padded]));

    // The zero byte and the three next most common values: 32 + 3 of 40 bytes.
    assert_eq!(
        result,
        Err(RecipeError::PredictableWindow {
            index: 0,
            top: 4,
            count: 35,
            len: 40
        })
    );
}

/// A 1-byte span at the end of [`PADDING`], with 16 zero bytes before it in its window and
/// `varied` bytes from the span on, which are distinct and not zero.
fn after_padding(varied: usize) -> Replacement {
    let stock = stock_application();
    let mut window = replacement(PADDING.end, &[1]);
    window.precondition.before = 16;
    window.precondition.after = varied as u64 - 1;
    window.precondition.sha256 = sha256_hex(&stock[PADDING.end - 16..PADDING.end + varied]);
    window
}

#[test]
fn the_four_most_common_values_may_fill_exactly_half_of_a_window() {
    let fixture = Fixture::new();

    // Zero and three other values fill 16 + 3 bytes: exactly half of 38, more than half of 37.
    assert!(
        fixture
            .apply(&fixture.recipe(vec![after_padding(22)]))
            .is_ok()
    );
    assert_eq!(
        fixture.apply(&fixture.recipe(vec![after_padding(21)])),
        Err(RecipeError::PredictableWindow {
            index: 0,
            top: 4,
            count: 19,
            len: 37
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

/// A 32-byte span at `0x900` that keeps `unchanged` stock bytes after its first one, with a window
/// of the span and `before` bytes before it.
fn partly_unchanged(unchanged: usize, before: usize) -> Replacement {
    let stock = stock_application();
    let span = 0x900..0x920;
    let mut bytes: Vec<u8> = stock[span.clone()].iter().map(|byte| !byte).collect();
    bytes[1..=unchanged].copy_from_slice(&stock[span.start + 1..=span.start + unchanged]);
    let mut partly = replacement(span.start, &bytes);
    partly.precondition.before = before as u64;
    partly.precondition.after = 0;
    partly.precondition.sha256 = sha256_hex(&stock[span.start - before..span.end]);
    partly
}

#[test]
fn at_most_half_of_a_span_may_equal_stock() {
    let fixture = Fixture::new();

    assert!(
        fixture
            .apply(&fixture.recipe(vec![partly_unchanged(16, 16)]))
            .is_ok()
    );
    assert_eq!(
        fixture.apply(&fixture.recipe(vec![partly_unchanged(17, 16)])),
        Err(RecipeError::MostlyUnchangedSpan {
            index: 0,
            unchanged: 17,
            len: 32
        })
    );
}

#[test]
fn stock_bytes_the_recipe_publishes_do_not_count_towards_the_window() {
    let fixture = Fixture::new();

    // 16 of the span's 32 bytes are published in bytes_hex, so the window needs 16 more before it.
    assert_eq!(
        fixture.apply(&fixture.recipe(vec![partly_unchanged(16, 15)])),
        Err(RecipeError::TooFewUnpublishedBytes {
            index: 0,
            unpublished: 31,
            min: 32
        })
    );
}

#[test]
fn hashes_and_pins_compare_without_regard_to_case() {
    let fixture = Fixture::new();
    let mut upper = replacement(0x900, &[0xde, 0xad]);
    upper.precondition.sha256 = upper.precondition.sha256.to_uppercase();
    upper.bytes_hex = upper.bytes_hex.to_uppercase();
    let mut recipe = fixture.recipe(vec![upper]);
    recipe.target.upd_sha256 = recipe.target.upd_sha256.to_uppercase();
    recipe.target.application_sha256 = recipe.target.application_sha256.to_uppercase();

    assert!(fixture.apply(&recipe).is_ok());
}

#[test]
fn a_mistyped_offset_moves_the_window_and_fails_the_precondition() {
    let fixture = Fixture::new();
    let mut typo = replacement(0x900, &[0xde, 0xad]);
    typo.offset = 0x904;

    let result = fixture.apply(&fixture.recipe(vec![typo]));

    assert!(
        matches!(result, Err(RecipeError::Precondition { index: 0, .. })),
        "{result:?}"
    );
}
