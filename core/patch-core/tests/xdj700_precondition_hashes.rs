//! `xdj700::precondition_hashes` on a synthetic release: the hashes an author needs to complete a
//! draft, computed only after every check.

mod common;

use common::recipe::{Fixture, PADDING, replacement, stock_application, windowed};
use patch_core::RebuildError;
use patch_core::xdj700::{RecipeError, precondition_hashes};
use patch_schema::Replacement;

const PLACEHOLDER: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// `replacement` as a draft: its precondition hash is a placeholder.
fn draft(mut replacement: Replacement) -> Replacement {
    replacement.precondition.sha256 = PLACEHOLDER.to_owned();
    replacement
}

#[test]
fn hashes_every_window_of_a_draft_and_the_completed_recipe_applies() {
    let fixture = Fixture::new();
    let complete = [replacement(0x900, &[0xde, 0xad]), replacement(0xa00, &[1])];
    let mut recipe = fixture.recipe(complete.iter().cloned().map(draft).collect());

    let hashes = precondition_hashes(&recipe, &fixture.target(), &fixture.update).expect("hash");

    let expected: Vec<&str> = complete
        .iter()
        .map(|replacement| replacement.precondition.sha256.as_str())
        .collect();
    assert_eq!(hashes, expected);
    for (replacement, sha256) in recipe.replacements.iter_mut().zip(hashes) {
        replacement.precondition.sha256 = sha256;
    }
    assert!(fixture.apply(&recipe).is_ok());
}

#[test]
fn hashes_nothing_for_a_recipe_without_replacements() {
    let fixture = Fixture::new();

    let hashes = precondition_hashes(&fixture.recipe(vec![]), &fixture.target(), &fixture.update);

    assert_eq!(hashes, Ok(vec![]));
}

#[test]
fn refuses_a_window_over_padding_instead_of_hashing_it() {
    let fixture = Fixture::new();
    let padded = draft(windowed(PADDING.end, &[1], 32, 7));

    let result = precondition_hashes(
        &fixture.recipe(vec![padded]),
        &fixture.target(),
        &fixture.update,
    );

    assert!(
        matches!(result, Err(RecipeError::PredictableWindow { index: 0, .. })),
        "{result:?}"
    );
}

#[test]
fn refuses_a_span_that_would_publish_stock_bytes_instead_of_hashing_it() {
    let fixture = Fixture::new();
    let stock = stock_application();
    let unchanged = draft(replacement(0x900, &stock[0x900..0x902]));

    let result = precondition_hashes(
        &fixture.recipe(vec![unchanged]),
        &fixture.target(),
        &fixture.update,
    );

    assert_eq!(result, Err(RecipeError::UnchangedSpanEdge { index: 0 }));
}

#[test]
fn runs_the_firmware_free_checks_before_looking_at_the_input() {
    let fixture = Fixture::new();
    let mut header = draft(replacement(0x808, &[1]));
    header.precondition.before = 16;
    header.precondition.after = 16;

    let result = precondition_hashes(&fixture.recipe(vec![header]), &fixture.target(), b"not it");

    assert!(
        matches!(result, Err(RecipeError::Protected { index: 0, .. })),
        "{result:?}"
    );
}

#[test]
fn refuses_an_input_that_is_not_the_release() {
    let fixture = Fixture::new();
    let recipe = fixture.recipe(vec![draft(replacement(0x900, &[0xde, 0xad]))]);

    let result = precondition_hashes(&recipe, &fixture.target(), b"not it");

    assert!(
        matches!(
            result,
            Err(RecipeError::Rebuild(RebuildError::UnpinnedInput { .. }))
        ),
        "{result:?}"
    );
}
