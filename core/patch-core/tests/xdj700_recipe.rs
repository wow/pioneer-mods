//! The schema-v2 recipe engine (`xdj700::apply_recipe_v2_to`) on synthetic releases: the
//! release, label and version rules, protected spans and output identities. Preconditions and
//! their windows are in `xdj700_recipe_preconditions.rs`.

mod common;

use common::recipe::{Fixture, replacement, stock_application};
use patch_core::RebuildError;
use patch_core::xdj700::{
    RecipeChecks, RecipeError, VERSION_STRING_OFFSET, apply_recipe_v2, apply_recipe_v2_to,
};
use patch_schema::ExpectedV2;
#[test]
fn a_version_only_recipe_changes_only_the_version_string() {
    let fixture = Fixture::new();

    let output = fixture.apply(&fixture.recipe(Vec::new())).expect("apply");

    let mut expected = stock_application();
    expected[VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + 4].copy_from_slice(b"0.10");
    assert_eq!(output, expected);
}

#[test]
fn replacements_are_applied_and_nothing_else_changes() {
    let fixture = Fixture::new();
    let recipe = fixture.recipe(vec![
        replacement(0x900, &[0xde, 0xad]),
        replacement(0xa00, &[1, 2, 3, 4]),
    ]);

    let output = fixture.apply(&recipe).expect("apply");

    let mut expected = stock_application();
    expected[VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + 4].copy_from_slice(b"0.10");
    expected[0x900..0x902].copy_from_slice(&[0xde, 0xad]);
    expected[0xa00..0xa04].copy_from_slice(&[1, 2, 3, 4]);
    assert_eq!(output, expected);
    let again = apply_recipe_v2_to(
        &recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    )
    .expect("again");
    let first = apply_recipe_v2_to(
        &recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    )
    .expect("first");
    assert_eq!(again, first, "deterministic");
}

#[test]
fn refuses_a_replacement_in_a_protected_range() {
    let fixture = Fixture::new();

    for offset in [0x0, 0x740, 0x7fe] {
        let result = fixture.apply(&fixture.recipe(vec![replacement(offset, &[1, 2, 3, 4])]));

        assert!(
            matches!(result, Err(RecipeError::Protected { index: 0, .. })),
            "{offset:#x}: {result:?}"
        );
    }
    assert!(
        fixture
            .apply(&fixture.recipe(vec![replacement(0x800, &[1])]))
            .is_ok()
    );
}

#[test]
fn refuses_labels_and_versions_the_updater_would_mishandle() {
    let fixture = Fixture::new();
    let mut low_label = fixture.recipe(Vec::new());
    low_label.label = "Ver1.15".to_owned();
    let mut high_version = fixture.recipe(Vec::new());
    high_version.reported_version = "1.15".to_owned();

    assert_eq!(
        fixture.apply(&low_label),
        Err(RecipeError::LabelNotHigher {
            label: "Ver1.15".to_owned(),
            stock: "1.15".to_owned()
        })
    );
    assert_eq!(
        fixture.apply(&high_version),
        Err(RecipeError::Rebuild(
            RebuildError::ReportedVersionNotLower {
                version: "1.15".to_owned(),
                official: "1.15".to_owned()
            }
        ))
    );
}

#[test]
fn refuses_a_recipe_whose_pins_differ_from_the_release() {
    let fixture = Fixture::new();
    let mut wrong_upd = fixture.recipe(Vec::new());
    wrong_upd.target.upd_sha256 = "11".repeat(32);
    let mut wrong_application = fixture.recipe(Vec::new());
    wrong_application.target.application_sha256 = "22".repeat(32);

    for (recipe, field) in [
        (wrong_upd, "target.upd_sha256"),
        (wrong_application, "target.application_sha256"),
    ] {
        let result = fixture.apply(&recipe);
        assert!(
            matches!(&result, Err(RecipeError::TargetMismatch { field: f, .. }) if *f == field),
            "{result:?}"
        );
    }
}

#[test]
fn refuses_an_unknown_release() {
    let fixture = Fixture::new();

    assert_eq!(
        apply_recipe_v2(
            &fixture.recipe(Vec::new()),
            &fixture.update,
            RecipeChecks::NONE
        )
        .map(|_| ()),
        Err(RecipeError::UnknownRelease {
            release: "synthetic".to_owned(),
            known: "xdj700-v1.15".to_owned()
        })
    );
}

#[test]
fn checks_declared_output_identities() {
    let fixture = Fixture::new();
    let mut recipe = fixture.recipe(Vec::new());
    let rebuilt = apply_recipe_v2_to(
        &recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    )
    .expect("apply");
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some(rebuilt.application_sha256().to_owned()),
        upd_sha256: Some(rebuilt.sha256().to_uppercase()),
    });
    assert!(
        apply_recipe_v2_to(
            &recipe,
            &fixture.target(),
            &fixture.update,
            RecipeChecks::NONE
        )
        .is_ok()
    );

    for (expected, field) in [
        (
            ExpectedV2 {
                application_sha256: None,
                upd_sha256: Some("33".repeat(32)),
            },
            "upd_sha256",
        ),
        (
            ExpectedV2 {
                application_sha256: Some("44".repeat(32)),
                upd_sha256: None,
            },
            "application_sha256",
        ),
    ] {
        recipe.expected = Some(expected);
        let result = apply_recipe_v2_to(
            &recipe,
            &fixture.target(),
            &fixture.update,
            RecipeChecks::NONE,
        );

        assert!(
            matches!(&result, Err(RecipeError::UnexpectedOutput { field: f, .. }) if *f == field),
            "{result:?}"
        );
    }
}

#[test]
fn refuses_an_invalid_recipe_before_anything_else() {
    let fixture = Fixture::new();
    let mut recipe = fixture.recipe(Vec::new());
    recipe.schema_version = 1;

    assert!(matches!(
        apply_recipe_v2_to(
            &recipe,
            &fixture.target(),
            b"not an update",
            RecipeChecks::NONE
        ),
        Err(RecipeError::Invalid(_))
    ));
}

#[test]
fn refuses_a_recipe_naming_another_release_than_its_target() {
    let fixture = Fixture::new();
    let mut recipe = fixture.recipe(Vec::new());
    recipe.target.release = "xdj700-v1.15".to_owned();

    let result = fixture.apply(&recipe);

    assert!(
        matches!(
            &result,
            Err(RecipeError::TargetMismatch {
                field: "target.release",
                ..
            })
        ),
        "{result:?}"
    );
}
