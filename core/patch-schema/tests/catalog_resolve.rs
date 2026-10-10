//! Resolving a profile (`resolve`) on the synthetic catalog of `common`: `plain` (no labels) and
//! `labelled` (labels on `perform.pad`; one implementation drawing them in the stock style, one
//! leaving them to the skin) conflict; the skin `dark-pads` on `perform` has an implementation
//! without labels and one drawing `labelled`'s.

mod common;

use common::{Fixture, implementation};
use patch_schema::catalog::{
    Fragment, Maturity, Profile, Resolution, ResolveError, Status, resolve,
};
use serde_json::{Value, json};

fn profile(features: Value, screens: Value) -> Value {
    json!({
        "schema_version": 1, "player": "p-1.0", "screens": screens, "features": features,
        "label": "Ver1.16", "reported_version": "0.12", "maturity": "experimental"
    })
}

fn resolved(fixture: &Fixture, profile: Value) -> Resolution {
    let profile: Profile = serde_json::from_value(profile).expect("profile");
    let catalog = fixture.catalog();
    fixture.check().expect("a consistent catalog");
    resolve(&catalog, &profile).expect("resolves")
}

fn on(recipe: &str) -> Status {
    Status::On {
        recipe: recipe.to_owned(),
        maturity: Maturity::Experimental,
        limits: Vec::new(),
    }
}

fn off(resolution: &Resolution, feature: &str) -> String {
    let found = resolution.features.iter().find(|f| f.feature == feature);
    match &found.expect("listed").status {
        Status::Off { reason } => reason.clone(),
        other => panic!("{feature} is {other:?}"),
    }
}

fn recipes(resolution: &Resolution) -> Vec<&str> {
    resolution
        .fragments
        .iter()
        .map(|f| f.recipe.as_str())
        .collect()
}

#[test]
fn a_feature_with_the_stock_skins_uses_its_stock_style_implementation() {
    let resolution = resolved(&Fixture::new(), profile(json!(["labelled"]), json!({})));

    assert_eq!(resolution.features[0].status, on("recipes/p/labelled.json"));
    let screens: Vec<(&str, &Status)> = resolution
        .screens
        .iter()
        .map(|s| (s.screen.as_str(), &s.status))
        .collect();
    assert_eq!(
        screens,
        [("main", &Status::Stock), ("perform", &Status::Stock)]
    );
    assert_eq!(
        resolution.fragments,
        [Fragment {
            recipe: "recipes/p/labelled.json".to_owned(),
            maturity: Maturity::Experimental,
            builds: vec!["feature labelled".to_owned()],
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
        resolution.screens[1].status,
        Status::Off {
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
            "player p-1.0 has no screen browse",
        ]
    );
}

#[test]
fn conflicting_features_are_both_switched_off() {
    let resolution = resolved(
        &Fixture::new(),
        profile(json!(["plain", "labelled"]), json!({})),
    );

    assert_eq!(
        off(&resolution, "plain"),
        "it conflicts with labelled, also chosen; choose one"
    );
    assert_eq!(
        off(&resolution, "labelled"),
        "it conflicts with plain, also chosen; choose one"
    );
    assert!(resolution.fragments.is_empty());
}

#[test]
fn a_skin_draws_the_labels_a_feature_leaves_to_it() {
    let screens = json!({ "perform": "dark-pads" });
    let resolution = resolved(
        &Fixture::new(),
        profile(json!(["labelled"]), screens.clone()),
    );

    assert_eq!(
        resolution.features[0].status,
        on("recipes/p/labelled-table.json")
    );
    assert_eq!(resolution.screens[1].status, on("recipes/p/skin.json"));
    assert_eq!(
        recipes(&resolution),
        ["recipes/p/labelled-table.json", "recipes/p/skin.json"]
    );
    // A combination is experimental at most.
    assert_eq!(resolution.tier(), Some(Maturity::Experimental));

    // Without a feature's labels, the skin's own implementation.
    let resolution = resolved(&Fixture::new(), profile(json!([]), screens));
    assert_eq!(recipes(&resolution), ["recipes/p/stock-labels.json"]);
}

#[test]
fn a_skin_that_cannot_draw_a_features_labels_switches_the_feature_off() {
    let mut fixture = Fixture::new();
    fixture.skins[0]["implementations"]["p-1.0"][1]["labels"]["pad"] = json!(["1", "2", "4"]);
    let resolution = resolved(
        &fixture,
        profile(json!(["labelled"]), json!({ "perform": "dark-pads" })),
    );

    assert_eq!(
        off(&resolution, "labelled"),
        "skin dark-pads on perform has no implementation for p-1.0 drawing its labels; with the \
         stock skin on perform, its stock-style implementation applies"
    );
    // The skin stays, with its implementation for no feature's labels.
    assert_eq!(recipes(&resolution), ["recipes/p/stock-labels.json"]);

    // A feature whose only implementation draws its labels in the stock style.
    let mut fixture = Fixture::new();
    let stock_style = fixture.features[1]["implementations"]["p-1.0"][0].clone();
    fixture.features[1]["implementations"]["p-1.0"] = json!([stock_style]);
    fixture.recipes.remove("recipes/p/labelled-table.json");
    let resolution = resolved(
        &fixture,
        profile(json!(["labelled"]), json!({ "perform": "dark-pads" })),
    );
    assert_eq!(
        off(&resolution, "labelled"),
        "it draws the perform labels as skin stock, but perform uses skin dark-pads"
    );
}

#[test]
fn a_skin_without_an_implementation_for_no_labels_keeps_the_stock_skin() {
    let mut fixture = Fixture::new();
    let drawing = fixture.skins[0]["implementations"]["p-1.0"][1].clone();
    fixture.skins[0]["implementations"]["p-1.0"] = json!([drawing]);
    fixture.recipes.remove("recipes/p/stock-labels.json");
    let resolution = resolved(
        &fixture,
        profile(json!([]), json!({ "perform": "dark-pads" })),
    );

    assert_eq!(
        resolution.screens[1].status,
        Status::Off {
            reason: "skin dark-pads has no implementation for p-1.0 without a feature's labels"
                .to_owned()
        }
    );
    assert!(resolution.fragments.is_empty());
}

#[test]
fn a_recipe_shared_by_two_features_is_one_fragment() {
    let mut fixture = Fixture::new();
    let mut other = fixture.features[0].clone();
    other["id"] = json!("other");
    other["conflicts"] = json!([]);
    other["implementations"]["p-1.0"] = json!([implementation("recipes/p/plain.json")]);
    fixture.features.push(other);
    let resolution = resolved(&fixture, profile(json!(["plain", "other"]), json!({})));

    // `plain` conflicts with `labelled`, which is not chosen.
    assert_eq!(recipes(&resolution), ["recipes/p/plain.json"]);
    assert_eq!(
        resolution.fragments[0].builds,
        ["feature plain", "feature other"]
    );
    assert_eq!(resolution.tier(), Some(Maturity::Experimental));
}

#[test]
fn an_invalid_profile_or_an_unknown_player_is_refused() {
    let catalog = Fixture::new().catalog();
    let mut value = profile(json!([]), json!({}));
    value["player"] = json!("q-1.0");
    let unknown: Profile = serde_json::from_value(value).expect("profile");
    assert_eq!(
        resolve(&catalog, &unknown),
        Err(ResolveError::UnknownPlayer("q-1.0".to_owned()))
    );

    let mut value = profile(json!([]), json!({}));
    value["maturity"] = json!("dev");
    let invalid: Profile = serde_json::from_value(value).expect("profile");
    assert!(matches!(
        resolve(&catalog, &invalid),
        Err(ResolveError::Profile(_))
    ));
}

#[test]
fn a_combination_is_experimental_at_most() {
    let fragment = |recipe: &str, maturity| Fragment {
        recipe: recipe.to_owned(),
        maturity,
        builds: Vec::new(),
    };
    let mut resolution = resolved(&Fixture::new(), profile(json!([]), json!({})));
    resolution.fragments = vec![fragment("recipes/a.json", Maturity::Stable)];
    assert_eq!(resolution.tier(), Some(Maturity::Stable));
    resolution
        .fragments
        .push(fragment("recipes/b.json", Maturity::Stable));
    assert_eq!(resolution.tier(), Some(Maturity::Experimental));
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
        "it leaves the perform labels to the skin, and perform uses the stock skin, which draws \
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
