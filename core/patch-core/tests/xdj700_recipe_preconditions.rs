//! Schema-v2 preconditions on synthetic releases: windows checked on stock, relative to their
//! span, inside the application, outside protected ranges, and not over padding.

mod common;

use common::recipe::{Fixture, PADDING, replacement, stock_application};
use patch_core::sha256_hex;
use patch_core::xdj700::RecipeError;

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

    assert_eq!(
        result,
        Err(RecipeError::PredictableWindow {
            index: 0,
            byte: 0,
            count: 32,
            len: 40
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
