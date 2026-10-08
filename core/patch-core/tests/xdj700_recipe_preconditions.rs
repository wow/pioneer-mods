//! Schema-v2 preconditions on synthetic releases: windows checked on stock, relative to their
//! span, inside the application and outside protected ranges. The leak checks are in
//! `xdj700_recipe_leaks.rs`.

mod common;

use common::recipe::{Fixture, replacement, stock_application, windowed};
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
    assert_eq!(exact_end.precondition_window(), Some(0xfdc..0x1000));
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
    let first = windowed(0x900, &[0xde, 0xad], 32, 0);
    let second = windowed(0x904, &[0xbe, 0xef], 0, 32);

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
    reaching.precondition.after = 16;

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

#[test]
fn a_mistyped_offset_is_reported_as_a_mismatch_before_any_leak_rule() {
    let fixture = Fixture::new();
    let mut typo = replacement(0x900, &[0xde, 0xad]);
    // The stock byte at 0x98d equals the span's first byte, so a leak rule alone would blame the
    // span's edge instead of the offset.
    assert_eq!(stock_application()[0x98d], 0xde);
    typo.offset = 0x98d;

    let result = fixture.apply(&fixture.recipe(vec![typo]));

    assert!(
        matches!(result, Err(RecipeError::Precondition { index: 0, .. })),
        "{result:?}"
    );
}
