//! The schema-v2 recipe engine (`xdj700::apply_recipe_v2_to`) on synthetic releases.

mod common;

use common::xdj700::{release, stock_update};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, RecipeError, RecipeTarget, StockRelease, VERSION_STRING_OFFSET,
    VersionBlock, apply_recipe_v2, apply_recipe_v2_to, decode_section, main_document,
};
use patch_core::{RebuildError, parse_upd, sha256_hex};
use patch_schema::{ExpectedV2, RecipeV2, Replacement, TargetV2};
use std::ops::Range;

const PROTECTED: &[Range<usize>] = &[Range {
    start: 0,
    end: 0x800,
}];

/// A synthetic stock application: the zero seed, a pattern, and `1.15` at the version offset.
fn stock_application() -> Vec<u8> {
    let mut decoded: Vec<u8> = (0..0x1000u32).map(|i| (i * 7 + 3) as u8).collect();
    decoded[..19].fill(0);
    decoded[VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + 5].copy_from_slice(b"1.15\0");
    decoded
}

/// The synthetic stock update, its SHA-256, and the stock application's SHA-256.
struct Fixture {
    update: Vec<u8>,
    upd_sha256: String,
    application_sha256: String,
}

impl Fixture {
    fn new() -> Self {
        let application = stock_application();
        let update = stock_update(&application);
        Self {
            upd_sha256: sha256_hex(&update),
            application_sha256: sha256_hex(&application),
            update,
        }
    }

    fn target(&self) -> RecipeTarget<'_> {
        RecipeTarget {
            id: "synthetic",
            release: StockRelease {
                version_block: Some(VersionBlock {
                    offset: VERSION_STRING_OFFSET,
                    stock_version: "1.15",
                    stock_application_sha256: &self.application_sha256,
                }),
                ..release(self.update.len(), &self.upd_sha256)
            },
            protected: PROTECTED,
        }
    }

    fn recipe(&self, replacements: Vec<Replacement>) -> RecipeV2 {
        RecipeV2 {
            schema_version: 2,
            recipe_id: "synthetic-test".to_owned(),
            description: "test".to_owned(),
            target: TargetV2 {
                release: "synthetic".to_owned(),
                upd_sha256: self.upd_sha256.clone(),
                application_sha256: self.application_sha256.clone(),
            },
            label: "Ver1.16".to_owned(),
            reported_version: "0.10".to_owned(),
            replacements,
            expected: None,
        }
    }

    fn apply(&self, recipe: &RecipeV2) -> Result<Vec<u8>, RecipeError> {
        let rebuilt = apply_recipe_v2_to(recipe, &self.target(), &self.update)?;
        let parsed = parse_upd(rebuilt.bytes()).expect("parse");
        let image = main_document(&parsed)
            .expect("main")
            .image()
            .expect("image");
        // The synthetic loader is not a verified one, so decode the section directly.
        let section = decode_section(image.bytes(), APPLICATION_SECTION_OFFSET).expect("decode");
        Ok(section.decoded().to_vec())
    }
}

/// A replacement of `bytes` at `offset`, with the stock bytes' SHA-256 as its precondition.
fn replacement(offset: usize, bytes: &[u8]) -> Replacement {
    let stock = stock_application();
    Replacement {
        offset: offset as u64,
        original_sha256: sha256_hex(&stock[offset..offset + bytes.len()]),
        bytes_hex: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        purpose: "test".to_owned(),
    }
}

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
    let again = apply_recipe_v2_to(&recipe, &fixture.target(), &fixture.update).expect("again");
    let first = apply_recipe_v2_to(&recipe, &fixture.target(), &fixture.update).expect("first");
    assert_eq!(again, first, "deterministic");
}

#[test]
fn refuses_a_replacement_whose_original_bytes_differ() {
    let fixture = Fixture::new();
    let mut wrong = replacement(0x900, &[0xde, 0xad]);
    wrong.original_sha256 = "00".repeat(32);

    let result = fixture.apply(&fixture.recipe(vec![wrong]));

    assert!(
        matches!(result, Err(RecipeError::Precondition { index: 0, .. })),
        "{result:?}"
    );
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
fn refuses_a_replacement_past_the_application() {
    let fixture = Fixture::new();
    let mut past = replacement(0xffc, &[1, 2, 3, 4]);
    past.offset = 0xffe;

    let result = fixture.apply(&fixture.recipe(vec![past]));

    assert_eq!(
        result,
        Err(RecipeError::OutOfBounds {
            index: 0,
            end: 0x1002,
            len: 0x1000
        })
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
        apply_recipe_v2(&fixture.recipe(Vec::new()), &fixture.update).map(|_| ()),
        Err(RecipeError::UnknownRelease("synthetic".to_owned()))
    );
}

#[test]
fn checks_declared_output_identities() {
    let fixture = Fixture::new();
    let mut recipe = fixture.recipe(Vec::new());
    let rebuilt = apply_recipe_v2_to(&recipe, &fixture.target(), &fixture.update).expect("apply");
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some(rebuilt.application_sha256().to_owned()),
        upd_sha256: Some(rebuilt.sha256().to_uppercase()),
    });
    assert!(apply_recipe_v2_to(&recipe, &fixture.target(), &fixture.update).is_ok());

    recipe.expected = Some(ExpectedV2 {
        application_sha256: None,
        upd_sha256: Some("33".repeat(32)),
    });
    let result = apply_recipe_v2_to(&recipe, &fixture.target(), &fixture.update);

    assert!(
        matches!(
            result,
            Err(RecipeError::UnexpectedOutput {
                field: "upd_sha256",
                ..
            })
        ),
        "{result:?}"
    );
}

#[test]
fn refuses_an_invalid_recipe_before_anything_else() {
    let fixture = Fixture::new();
    let mut recipe = fixture.recipe(Vec::new());
    recipe.schema_version = 1;

    assert!(matches!(
        apply_recipe_v2_to(&recipe, &fixture.target(), b"not an update"),
        Err(RecipeError::Invalid(_))
    ));
}
