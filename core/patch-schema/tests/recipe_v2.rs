//! Static validation of schema-v2 recipes (no firmware needed).

use patch_schema::{RecipeV2, RecipeV2Error, Replacement, SchemaVersionProbe};
use serde_json::json;

fn replacement(offset: u64, bytes_hex: &str) -> Replacement {
    Replacement {
        offset,
        original_sha256: "ab".repeat(32),
        bytes_hex: bytes_hex.to_owned(),
        purpose: "test".to_owned(),
    }
}

fn recipe_json() -> serde_json::Value {
    json!({
        "schema_version": 2,
        "recipe_id": "xdj700-example",
        "description": "example",
        "target": {
            "release": "xdj700-v1.15",
            "upd_sha256": "00".repeat(32),
            "application_sha256": "11".repeat(32)
        },
        "label": "Ver1.16",
        "reported_version": "0.10",
        "replacements": [
            {"offset": 2048, "original_sha256": "ab".repeat(32), "bytes_hex": "DEad", "purpose": "x"}
        ],
        "expected": {"upd_sha256": "22".repeat(32)}
    })
}

/// Changes one field of a valid recipe.
type Mutation = fn(&mut RecipeV2);

fn recipe() -> RecipeV2 {
    serde_json::from_value(recipe_json()).expect("valid JSON")
}

#[test]
fn a_well_formed_recipe_validates() {
    let recipe = recipe();

    assert_eq!(recipe.validate(), Ok(()));
    assert_eq!(recipe.replacements[0].bytes(), Some(vec![0xde, 0xad]));
    assert_eq!(recipe.replacements[0].len(), 2);
    let probe: SchemaVersionProbe = serde_json::from_value(recipe_json()).expect("probe");
    assert_eq!(probe.schema_version, 2);
}

#[test]
fn unknown_fields_are_refused() {
    let mut value = recipe_json();
    value["replacements"][0]["original_bytes"] = json!("00");

    assert!(serde_json::from_value::<RecipeV2>(value).is_err());
}

#[test]
fn replacements_and_expected_are_optional() {
    let mut value = recipe_json();
    let object = value.as_object_mut().expect("object");
    object.remove("replacements");
    object.remove("expected");

    let recipe: RecipeV2 = serde_json::from_value(value).expect("valid JSON");

    assert_eq!(recipe.validate(), Ok(()));
    assert!(recipe.replacements.is_empty());
}

#[test]
fn identity_and_version_fields_are_checked() {
    let cases: Vec<(Mutation, RecipeV2Error)> = vec![
        (|r| r.schema_version = 1, RecipeV2Error::SchemaVersion(1)),
        (
            |r| r.recipe_id = " ".to_owned(),
            RecipeV2Error::EmptyRecipeId,
        ),
        (
            |r| r.target.release = String::new(),
            RecipeV2Error::EmptyRelease,
        ),
        (
            |r| r.target.upd_sha256 = "zz".repeat(32),
            RecipeV2Error::InvalidSha256 {
                field: "target.upd_sha256".to_owned(),
            },
        ),
        (
            |r| r.target.application_sha256 = "00".to_owned(),
            RecipeV2Error::InvalidSha256 {
                field: "target.application_sha256".to_owned(),
            },
        ),
        (
            |r| r.label = "1.16".to_owned(),
            RecipeV2Error::InvalidLabel("1.16".to_owned()),
        ),
        (
            |r| r.reported_version = "Ver0.10".to_owned(),
            RecipeV2Error::InvalidReportedVersion("Ver0.10".to_owned()),
        ),
        (
            |r| r.expected.as_mut().expect("expected").upd_sha256 = Some("0".to_owned()),
            RecipeV2Error::InvalidSha256 {
                field: "expected.upd_sha256".to_owned(),
            },
        ),
    ];

    for (mutate, error) in cases {
        let mut recipe = recipe();
        mutate(&mut recipe);
        assert_eq!(recipe.validate(), Err(error));
    }
}

#[test]
fn replacement_fields_are_checked() {
    for bytes_hex in ["", "abc", "zz"] {
        let mut recipe = recipe();
        recipe.replacements = vec![replacement(0x800, bytes_hex)];
        assert_eq!(
            recipe.validate(),
            Err(RecipeV2Error::InvalidReplacementBytes { index: 0 }),
            "{bytes_hex:?}"
        );
    }
    let mut recipe = recipe();
    recipe.replacements = vec![Replacement {
        purpose: " ".to_owned(),
        ..replacement(0x800, "00")
    }];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::EmptyPurpose { index: 0 })
    );
    recipe.replacements = vec![Replacement {
        original_sha256: "ab".to_owned(),
        ..replacement(0x800, "00")
    }];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::InvalidSha256 {
            field: "replacements[0].original_sha256".to_owned()
        })
    );
    recipe.replacements = vec![replacement(u64::MAX, "0000")];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::OffsetOverflow { index: 0 })
    );
}

#[test]
fn replacements_must_be_ascending_and_disjoint() {
    let mut recipe = recipe();
    recipe.replacements = vec![replacement(0x800, "0000"), replacement(0x802, "00")];
    assert_eq!(recipe.validate(), Ok(()), "adjacent spans are fine");

    for second in [0x801, 0x700] {
        recipe.replacements = vec![replacement(0x800, "0000"), replacement(second, "00")];
        assert_eq!(
            recipe.validate(),
            Err(RecipeV2Error::UnorderedOrOverlapping {
                index: 1,
                previous: 0
            }),
            "{second:#x}"
        );
    }
}
