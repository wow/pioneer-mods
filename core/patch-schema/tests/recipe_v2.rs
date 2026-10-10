//! Static validation of schema-v2 recipes (no firmware needed).

use patch_schema::{
    MAX_PRECONDITION_LEN, MIN_PRECONDITION_LEN, Precondition, RecipeV2, RecipeV2Error, Replacement,
    SchemaVersionProbe,
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
            "precondition": {"before": 8, "after": 24, "sha256": "ab".repeat(32)},
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
    assert_eq!(
        recipe.replacements[0].pattern(),
        Some(vec![Some(0xde), Some(0xad)])
    );
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
fn kept_bytes_are_parsed_and_edges_must_be_written() {
    let mut recipe = recipe();
    recipe.replacements = vec![replacement(0x800, "de--AD")];
    assert_eq!(recipe.validate(), Ok(()));
    assert_eq!(
        recipe.replacements[0].pattern(),
        Some(vec![Some(0xde), None, Some(0xad)])
    );
    for bytes_hex in ["--ad", "de--", "--", "----"] {
        recipe.replacements = vec![replacement(0x800, bytes_hex)];
        assert_eq!(
            recipe.validate(),
            Err(RecipeV2Error::KeptSpanEdge { index: 0 }),
            "{bytes_hex:?}"
        );
    }
    // Parsing alone does not judge where kept bytes are.
    assert_eq!(
        replacement(0x800, "--ad").pattern(),
        Some(vec![None, Some(0xad)])
    );
    for bytes_hex in ["de-d", "d--e", "de-"] {
        recipe.replacements = vec![replacement(0x800, bytes_hex)];
        assert_eq!(
            recipe.validate(),
            Err(RecipeV2Error::InvalidReplacementBytes { index: 0 }),
            "{bytes_hex:?}"
        );
        assert_eq!(recipe.replacements[0].pattern(), None, "{bytes_hex:?}");
    }
}

#[test]
fn fewer_than_min_precondition_len_bytes_in_a_row_may_be_kept() {
    let kept = |len: usize| format!("aa{}bb{}cc", "--".repeat(3), "--".repeat(len));
    let mut recipe = recipe();
    let limit = MIN_PRECONDITION_LEN as usize;

    recipe.replacements = vec![replacement(0x800, &kept(limit - 1))];
    assert_eq!(recipe.validate(), Ok(()));
    recipe.replacements = vec![replacement(0x800, &kept(limit))];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::LongKeptRun {
            index: 0,
            len: limit
        })
    );
}

#[test]
fn written_runs_are_split_by_kept_bytes() {
    assert_eq!(
        replacement(0x800, "aa--BBcc----dd").written_runs(),
        Some(vec![
            (0, vec![0xaa]),
            (2, vec![0xbb, 0xcc]),
            (6, vec![0xdd])
        ])
    );
    assert_eq!(
        replacement(0x800, "ab").written_runs(),
        Some(vec![(0, vec![0xab])])
    );
    assert_eq!(replacement(0x800, "zz").written_runs(), None);
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
    // Adjacent spans with windows reaching outward (before the first, after the second).
    let mut first = replacement(0x800, "0000");
    first.precondition.before = 32;
    first.precondition.after = 0;
    recipe.replacements = vec![first, replacement(0x802, "00")];
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
        after: 24,
        sha256: "ab".repeat(32),
    };
    assert_eq!(window.precondition_window(), Some(0x7f8..0x81a));
    recipe.replacements = vec![window.clone()];
    assert_eq!(
        recipe.validate(),
        Ok(()),
        "8 + 24 = 32 bytes outside the span"
    );

    // The span's own bytes do not count.
    window.precondition.after = 23;
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

/// The leak a reviewer demonstrated: 1-byte replacements whose windows slide one byte at a time
/// share all but one byte, so each hash would reveal one more stock byte. Windows must not overlap.
#[test]
fn overlapping_precondition_windows_are_refused() {
    let mut recipe = recipe();
    recipe.replacements = (0..4)
        .map(|i| Replacement {
            precondition: Precondition {
                before: 32,
                after: 0,
                sha256: "ab".repeat(32),
            },
            ..replacement(0x900 + i, "00")
        })
        .collect();

    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::OverlappingPreconditions {
            index: 1,
            previous: 0
        })
    );

    // Touching windows are fine: each byte is in exactly one window.
    recipe.replacements = vec![
        Replacement {
            precondition: Precondition {
                before: 32,
                after: 0,
                sha256: "ab".repeat(32),
            },
            ..replacement(0x900, "00")
        },
        replacement(0x901, "00"),
    ];
    assert_eq!(recipe.validate(), Ok(()));
}

#[test]
fn precondition_windows_are_capped() {
    let mut recipe = recipe();
    let mut long = replacement(0x800, "00");
    long.precondition.after = MAX_PRECONDITION_LEN;
    recipe.replacements = vec![long.clone()];
    assert_eq!(
        recipe.validate(),
        Err(RecipeV2Error::PreconditionTooLong { index: 0 })
    );

    long.precondition.after = MAX_PRECONDITION_LEN - 1;
    recipe.replacements = vec![long];
    assert_eq!(recipe.validate(), Ok(()), "exactly the cap");
}

#[test]
fn is_empty_agrees_with_len() {
    let odd = replacement(0x800, "a");
    assert_eq!(odd.len(), 0);
    assert!(odd.is_empty());
}
