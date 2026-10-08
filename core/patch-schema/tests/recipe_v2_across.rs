//! Precondition windows across recipes: per release, identical or disjoint.

use patch_schema::{RecipeV2, WindowOverlap, check_windows_across};
use serde_json::json;

/// A recipe `id` for `release` with one replacement per `(offset, before, after)`, each a 1-byte
/// span.
fn recipe(id: &str, release: &str, windows: &[(u64, u64, u64)]) -> RecipeV2 {
    let replacements: Vec<_> = windows
        .iter()
        .map(|&(offset, before, after)| {
            json!({
                "offset": offset,
                "bytes_hex": "00",
                "precondition": {"before": before, "after": after, "sha256": "ab".repeat(32)},
                "purpose": "test"
            })
        })
        .collect();
    serde_json::from_value(json!({
        "schema_version": 2,
        "recipe_id": id,
        "description": "test",
        "target": {
            "release": release,
            "upd_sha256": "00".repeat(32),
            "application_sha256": "11".repeat(32)
        },
        "label": "Ver1.16",
        "reported_version": "0.10",
        "replacements": replacements
    }))
    .expect("recipe JSON")
}

#[test]
fn windows_shifted_across_recipes_are_refused() {
    // Windows 0x8e0..0x901 and 0x900..0x921 share one byte.
    let a = recipe("a", "r", &[(0x900, 32, 0)]);
    let b = recipe("b", "r", &[(0x900, 0, 32), (0x1000, 0, 32)]);

    assert_eq!(
        check_windows_across([&b, &a]),
        Err(WindowOverlap {
            first: "a replacements[0]".to_owned(),
            first_window: 0x8e0..0x901,
            second: "b replacements[0]".to_owned(),
            second_window: 0x900..0x921,
        })
    );
}

#[test]
fn touching_identical_and_other_release_windows_are_accepted() {
    let a = recipe("a", "r", &[(0x900, 32, 0)]);
    let touching = recipe("touching", "r", &[(0x901, 0, 32)]);
    let identical = recipe("identical", "r", &[(0x900, 32, 0)]);
    let other_release = recipe("other", "s", &[(0x8f0, 16, 16)]);

    assert_eq!(
        check_windows_across([&a, &touching, &identical, &other_release]),
        Ok(())
    );
}

#[test]
fn an_identical_window_does_not_hide_an_overlap_behind_it() {
    let a = recipe("a", "r", &[(0x900, 32, 0)]);
    let identical = recipe("identical", "r", &[(0x900, 32, 0)]);
    let shifted = recipe("shifted", "r", &[(0x8f0, 0, 32)]);

    let result = check_windows_across([&a, &identical, &shifted]);

    assert!(
        matches!(&result, Err(overlap) if overlap.second_window == (0x8f0..0x911)),
        "{result:?}"
    );
}
