//! Resolving a profile (`resolve`) on the synthetic catalog of `common`: `plain` (no labels) and
//! `labelled` (labels on `perform.pad`; one implementation drawing them in the stock style, one
//! leaving them to the skin) conflict; the skin `dark-pads` on `perform` has an implementation
//! without labels and one drawing `labelled`'s. How skins meet features is in
//! `catalog_resolve_skins.rs`.

mod common;

use common::resolve::{off, on, perform, profile, recipes, resolved, status, stock_style_only};
use common::{Fixture, OUT, implementation};
use patch_schema::catalog::{
    BuildError, Fragment, Maturity, Profile, ResolveError, Status, resolve,
};
use serde_json::json;

#[test]
fn a_feature_with_the_stock_skins_uses_its_stock_style_implementation() {
    let resolution = resolved(&Fixture::new(), profile(json!(["labelled"]), json!({})));

    assert_eq!(
        status(&resolution, "labelled"),
        &on("recipes/p/labelled.json")
    );
    assert_eq!(resolution.screens[0].status, Status::Stock);
    assert_eq!(perform(&resolution), &Status::Stock);
    assert_eq!(
        resolution.fragments,
        [Fragment {
            recipe: "recipes/p/labelled.json".to_owned(),
            maturity: Maturity::Experimental,
            builds: vec!["feature labelled".to_owned()],
            pinned_update: Some(OUT.to_owned()),
            as_pinned: true,
        }]
    );
    assert_eq!(resolution.tier(), Some(Maturity::Experimental));
}

#[test]
fn the_profile_maturity_and_unknown_names_switch_items_off() {
    let mut stable = profile(json!(["plain"]), json!({ "perform": "dark-pads" }));
    stable["maturity"] = json!("stable");
    let resolution = resolved(&Fixture::new(), stable);
    assert_eq!(
        off(&resolution, "plain"),
        "its implementation for p-1.0 is experimental; the profile accepts stable only"
    );
    assert_eq!(
        perform(&resolution),
        &Status::Off {
            reason: "skin dark-pads: its implementations for p-1.0 are experimental; the \
                     profile accepts stable only"
                .to_owned()
        }
    );
    assert_eq!(resolution.tier(), None);

    let screens = json!({ "main": "dark-pads", "perform": "missing", "browse": "stock" });
    let resolution = resolved(&Fixture::new(), profile(json!(["missing"]), screens));
    assert_eq!(off(&resolution, "missing"), "it is not in the catalog");
    let reasons: Vec<String> = resolution
        .screens
        .iter()
        .map(|s| match &s.status {
            Status::Off { reason } => reason.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(
        reasons,
        [
            "skin dark-pads is for screen perform",
            "skin missing is not in the catalog",
        ]
    );
    // A screen the player lacks is listed apart, whatever skin the profile chose for it.
    assert_eq!(resolution.missing_screens, ["browse"]);
}

#[test]
fn conflicting_features_are_both_switched_off_whatever_the_skins() {
    let conflict = |other: &str| format!("it conflicts with {other}, also chosen; choose one");
    for screens in [json!({}), json!({ "perform": "dark-pads" })] {
        // With only its stock-style implementation, `labelled` cannot fit `dark-pads`; the
        // conflict does not depend on that.
        let mut fixture = Fixture::new();
        stock_style_only(&mut fixture);
        let resolution = resolved(&fixture, profile(json!(["plain", "labelled"]), screens));

        assert_eq!(off(&resolution, "plain"), conflict("labelled"));
        assert_eq!(off(&resolution, "labelled"), conflict("plain"));
        assert!(
            resolution
                .fragments
                .iter()
                .all(|f| f.builds[0].starts_with("skin"))
        );
    }
}

#[test]
fn labels_left_to_the_skin_need_a_skin_other_than_stock() {
    // A feature whose only implementation leaves its labels to the skin.
    let mut fixture = Fixture::new();
    let table_only = fixture.features[1]["implementations"]["p-1.0"][1].clone();
    fixture.features[1]["implementations"]["p-1.0"] = json!([table_only]);
    fixture.recipes.remove("recipes/p/labelled.json");

    let resolution = resolved(&fixture, profile(json!(["labelled"]), json!({})));
    assert_eq!(
        off(&resolution, "labelled"),
        "it leaves the perform labels to a skin, and perform uses the stock skin, which draws \
         none"
    );
    assert!(resolution.fragments.is_empty());

    // With a skin that draws them, it is on.
    let resolution = resolved(
        &fixture,
        profile(json!(["labelled"]), json!({ "perform": "dark-pads" })),
    );
    assert_eq!(
        recipes(&resolution),
        ["recipes/p/labelled-table.json", "recipes/p/skin.json"]
    );
}

#[test]
fn a_recipe_shared_by_two_features_is_one_fragment_at_the_lower_maturity() {
    let mut fixture = Fixture::new();
    fixture.features[0]["implementations"]["p-1.0"][0]["maturity"] = json!("stable");
    let mut other = fixture.features[0].clone();
    other["id"] = json!("other");
    other["conflicts"] = json!([]);
    other["implementations"]["p-1.0"] = json!([implementation("recipes/p/plain.json")]);
    fixture.features.push(other);
    let resolution = resolved(&fixture, profile(json!(["plain", "other"]), json!({})));

    assert_eq!(recipes(&resolution), ["recipes/p/plain.json"]);
    assert_eq!(
        resolution.fragments[0].builds,
        ["feature plain", "feature other"]
    );
    assert_eq!(resolution.fragments[0].maturity, Maturity::Experimental);
}

#[test]
fn only_a_lone_fragment_under_its_own_label_and_version_keeps_its_tier() {
    let mut fixture = Fixture::new();
    fixture.features[0]["implementations"]["p-1.0"][0]["maturity"] = json!("stable");
    let mut stable = profile(json!(["plain"]), json!({}));
    stable["maturity"] = json!("stable");
    let resolution = resolved(&fixture, stable.clone());
    assert!(resolution.fragments[0].as_pinned);
    assert_eq!(resolution.tier(), Some(Maturity::Stable));
    assert_eq!(resolution.buildable(), Ok(Maturity::Stable));
    assert_eq!(resolution.pinned_output(), Some(OUT));

    // A recipe that pins no update file: its output is not a pinned file.
    let mut unpinned = Fixture::new();
    unpinned.features[0]["implementations"]["p-1.0"][0]["maturity"] = json!("stable");
    unpinned
        .recipes
        .get_mut("recipes/p/plain.json")
        .expect("plain")["expected"]
        .as_object_mut()
        .expect("expected")
        .remove("upd_sha256");
    let resolution = resolved(&unpinned, stable.clone());
    assert_eq!(resolution.fragments[0].pinned_update, None);
    assert!(!resolution.fragments[0].as_pinned);
    assert_eq!(resolution.pinned_output(), None);
    assert_eq!(resolution.tier(), Some(Maturity::Experimental));

    // Another label: a new update.
    let mut relabelled = stable.clone();
    relabelled["label"] = json!("Ver1.17");
    assert!(!resolved(&fixture, relabelled).fragments[0].as_pinned);

    // Another reported version: a new update.
    stable["reported_version"] = json!("0.13");
    let mut resolution = resolved(&fixture, stable);
    assert!(!resolution.fragments[0].as_pinned);
    assert_eq!(resolution.tier(), Some(Maturity::Experimental));

    // Two stable fragments, each under its own label and version: a combination.
    resolution.fragments[0].as_pinned = true;
    let mut second = resolution.fragments[0].clone();
    second.recipe = "recipes/p/other.json".to_owned();
    resolution.fragments.push(second);
    assert_eq!(resolution.tier(), Some(Maturity::Experimental));
}

#[test]
fn an_invalid_profile_or_an_unknown_player_is_refused() {
    let fixture = Fixture::new();
    let checked = fixture.checked();
    let mut value = profile(json!([]), json!({}));
    value["player"] = json!("q-1.0");
    let unknown: Profile = serde_json::from_value(value).expect("profile");
    assert_eq!(
        resolve(&checked, &unknown),
        Err(ResolveError::UnknownPlayer("q-1.0".to_owned()))
    );

    let mut value = profile(json!([]), json!({}));
    value["maturity"] = json!("dev");
    let invalid: Profile = serde_json::from_value(value).expect("profile");
    assert!(matches!(
        resolve(&checked, &invalid),
        Err(ResolveError::Profile(_))
    ));
}

#[test]
fn a_conflict_with_a_feature_already_off_switches_nothing_off() {
    // `labelled` is experimental only, so a stable profile leaves it off: `plain` stays on.
    let mut fixture = Fixture::new();
    fixture.features[0]["implementations"]["p-1.0"][0]["maturity"] = json!("stable");
    let mut stable = profile(json!(["plain", "labelled"]), json!({}));
    stable["maturity"] = json!("stable");
    let resolution = resolved(&fixture, stable);

    assert!(matches!(status(&resolution, "plain"), Status::On { .. }));
    assert_eq!(
        off(&resolution, "labelled"),
        "its implementations for p-1.0 are experimental; the profile accepts stable only"
    );
}

#[test]
fn a_build_needs_something_on_and_a_tier_the_profile_accepts() {
    let resolution = resolved(&Fixture::new(), profile(json!([]), json!({})));
    assert_eq!(resolution.buildable(), Err(BuildError::NothingOn));

    // A stable implementation under another reported version: a new update, experimental at most.
    let mut fixture = Fixture::new();
    fixture.features[0]["implementations"]["p-1.0"][0]["maturity"] = json!("stable");
    let mut stable = profile(json!(["plain"]), json!({}));
    stable["maturity"] = json!("stable");
    stable["reported_version"] = json!("0.13");
    let resolution = resolved(&fixture, stable);
    let refused = resolution.buildable().expect_err("refused");
    assert_eq!(
        refused,
        BuildError::LessSettled {
            tier: Maturity::Experimental,
            accepts: Maturity::Stable
        }
    );
    assert!(
        refused.to_string().ends_with(
            "is a new update and experimental at most); the profile accepts stable only"
        ),
        "{refused}"
    );
    assert_eq!(resolution.pinned_output(), None);

    // Experimental accepted: the same build may be built.
    let resolution = resolved(&Fixture::new(), profile(json!(["plain"]), json!({})));
    assert_eq!(resolution.buildable(), Ok(Maturity::Experimental));
}
