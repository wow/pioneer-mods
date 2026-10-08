//! Static validation of schema-v2 recipes (no firmware needed).

use patch_schema::{
    MIN_PRECONDITION_LEN, Precondition, RecipeV2, RecipeV2Error, Replacement, SchemaVersionProbe,
};
use serde_json::json;

/// A replacement whose precondition window is the minimum length, mostly after the span.
fn replacement(offset: u64, bytes_hex: &str) -> Replacement {
    Replacement {
        offset,
        bytes_hex: bytes_hex.to_owned(),
        precondition: Precondition {
            before: 0,
            after: MIN_PRECONDITION_LEN,
            sha256: "ab".repeat(32),
        },
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
        "replacements": [{
            "offset": 2048,
            "bytes_hex": "DEad",
            "precondition": {"before": 8, "after": 22, "sha256": "ab".repeat(32)},
            "purpose": "x"
        }],
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
        (
            |r| r.expected.as_mut().expect("expected").application_sha256 = Some("0".to_owned()),
            RecipeV2Error::InvalidSha256 {
                field: "expected.application_sha256".to_owned(),
            },
        ),
        (
            |r| r.expected.as_mut().expect("expected").upd_sha256 = None,
            RecipeV2Error::EmptyExpected,
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
    let mut short_hash = replacement(0x800, "00");
    short_hash.precondition.sha256 = "ab".to_owned();
    recipe.replacements = vec![short_hash];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::InvalidSha256 {
            field: "replacements[0].precondition.sha256".to_owned()
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

#[test]
fn the_precondition_window_is_relative_and_long_enough() {
    let mut recipe = recipe();
    let mut window = replacement(0x800, "0000");
    window.precondition = Precondition {
        before: 8,
        after: 22,
        sha256: "ab".repeat(32),
    };
    assert_eq!(window.precondition_window(), Some(0x7f8..0x818));
    recipe.replacements = vec![window.clone()];
    assert_eq!(recipe.validate(), Ok(()), "8 + 2 + 22 = 32 bytes");

    window.precondition.after = 21;
    recipe.replacements = vec![window.clone()];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::PreconditionTooShort { index: 0 })
    );

    window.precondition.before = 0x801;
    window.precondition.after = 32;
    recipe.replacements = vec![window.clone()];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::PreconditionBeforeStart { index: 0 })
    );

    window.precondition.before = 0;
    window.precondition.after = u64::MAX;
    recipe.replacements = vec![window];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::OffsetOverflow { index: 0 })
    );
}
