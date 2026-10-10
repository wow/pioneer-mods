//! Precondition windows across recipes: per release, disjoint unless the replacements are
//! identical.

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
fn touching_windows_and_windows_of_other_releases_are_accepted() {
    let a = recipe("a", "r", &[(0x900, 32, 0)]);
    let touching = recipe("touching", "r", &[(0x901, 0, 32)]);
    let other_release = recipe("other", "s", &[(0x8f0, 16, 16)]);

    assert_eq!(
        check_windows_across([&a, &touching, &other_release]),
        Ok(())
    );
}

/// The same window with another change: each recipe's span would sit among the other's
/// "unpublished" window bytes, and their kept stock bytes would add up. Another hash (a
/// placeholder, or a mistyped one) is another change too.
#[test]
fn identical_windows_of_different_replacements_are_refused() {
    let a = recipe("a", "r", &[(0x900, 32, 0)]);
    // The same window 0x8e0..0x901 around another span.
    let moved_span = recipe("moved", "r", &[(0x8f0, 16, 16)]);
    let mut other_bytes = recipe("bytes", "r", &[(0x900, 32, 0)]);
    other_bytes.replacements[0].bytes_hex = "01".to_owned();
    let mut other_hash = recipe("hash", "r", &[(0x900, 32, 0)]);
    other_hash.replacements[0].precondition.sha256 = "cd".repeat(32);

    for other in [&moved_span, &other_bytes, &other_hash] {
        let error = check_windows_across([&a, other]).expect_err("overlap");
        // Equal windows sort by recipe order, so `a` comes first.
        assert_eq!(error.first, "a replacements[0]");
        assert_eq!(error.first_window, 0x8e0..0x901);
        assert_eq!(error.second, format!("{} replacements[0]", other.recipe_id));
        assert_eq!(error.second_window, 0x8e0..0x901);
        assert!(
            error
                .to_string()
                .contains("repeats another's replacement exactly")
        );
    }
}

/// A replacement repeated exactly publishes nothing new, so a recipe may build on another's change;
/// only the purpose may differ, and hex is compared without regard to case.
#[test]
fn identical_replacements_across_recipes_are_accepted() {
    let mut a = recipe("a", "r", &[(0x900, 32, 0)]);
    a.replacements[0].bytes_hex = "0d0a".to_owned();
    let mut repeated = recipe("repeated", "r", &[(0x900, 32, 0), (0x1000, 0, 32)]);
    repeated.replacements[0].bytes_hex = "0D0A".to_owned();
    repeated.replacements[0].purpose = "the same change, described again".to_owned();
    repeated.replacements[0].precondition.sha256 = "AB".repeat(32);

    assert_eq!(check_windows_across([&a, &repeated]), Ok(()));

    // A third recipe whose window overlaps the shared one is still refused.
    let shifted = recipe("shifted", "r", &[(0x901, 32, 0)]);
    let error = check_windows_across([&a, &repeated, &shifted]).expect_err("overlap");
    assert_eq!(error.second, "shifted replacements[0]");
    assert_eq!(error.second_window, 0x8e1..0x902);
}
