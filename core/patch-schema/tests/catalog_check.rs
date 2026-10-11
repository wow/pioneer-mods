//! The catalog as a whole (`Catalog::check`) on the synthetic catalog of `common`.

mod common;

use common::{APP, Fixture, UPD, implementation};
use patch_schema::catalog::{Catalog, CatalogError};
use serde_json::{Value, json};
use std::collections::BTreeMap;

#[test]
fn a_consistent_catalog_checks() {
    assert_eq!(Fixture::new().check(), Ok(()));
}

#[test]
fn the_check_validates_every_file_itself() {
    let mut catalog: Catalog = Fixture::new().catalog();
    catalog.features[0].id = "Plain".to_owned();

    let error = catalog.check(&BTreeMap::new()).expect_err("refused");

    assert!(matches!(error, CatalogError::Invalid { .. }));
    // The cause is part of the message, and not repeated as a source.
    assert!(std::error::Error::source(&error).is_none());
    assert_eq!(error.to_string().matches("is not an id").count(), 1);
}

#[test]
fn ids_are_unique_and_screens_match_their_players() {
    let mut fixture = Fixture::new();
    fixture.features.push(fixture.features[0].clone());
    assert_eq!(fixture.problem(), "feature plain is defined twice");

    let mut fixture = Fixture::new();
    fixture.screens.remove(0);
    assert_eq!(
        fixture.problem(),
        "player p-1.0: screen p-1.0/main is not in the catalog"
    );

    let mut fixture = Fixture::new();
    fixture.player["screens"] = json!(["perform"]);
    assert_eq!(
        fixture.problem(),
        "screen p-1.0/main: player p-1.0 does not list it"
    );
}

#[test]
fn conflicts_are_listed_on_both_sides() {
    let mut fixture = Fixture::new();
    fixture.features[1]["conflicts"] = json!([]);
    assert!(
        fixture
            .problem()
            .contains("conflicts with labelled, which does not list it")
    );

    let mut fixture = Fixture::new();
    fixture.features[0]["conflicts"] = json!(["missing"]);
    assert_eq!(
        fixture.problem(),
        "feature plain: feature missing is not in the catalog"
    );
}

#[test]
fn features_that_relabel_one_slot_must_conflict() {
    let mut fixture = Fixture::new();
    // A third feature relabelling the same slot, without a conflict.
    let mut third = fixture.features[1].clone();
    third["id"] = json!("third");
    third["conflicts"] = json!([]);
    fixture.features.push(third);

    assert_eq!(
        fixture.problem(),
        "feature labelled: it and third both relabel perform.pad; they must list each other in \
         conflicts"
    );
}

#[test]
fn an_implementation_must_meet_its_requirements() {
    let unmet = |change: fn(&mut Fixture)| {
        let mut fixture = Fixture::new();
        change(&mut fixture);
        fixture.problem()
    };

    let message = unmet(|f| f.features[0]["requires"]["capabilities"]["lengths"] = json!(["64"]));
    assert!(
        message.contains("needs capability lengths with [\"64\"]"),
        "{message}"
    );
    let message =
        unmet(|f| f.features[0]["requires"]["capabilities"] = json!({ "jog_display": [] }));
    assert!(
        message.contains("needs capability jog_display"),
        "{message}"
    );
    let message = unmet(|f| f.features[0]["requires"]["slots"] = json!(["main.pad"]));
    assert!(
        message.contains("needs slot main.pad, which p-1.0 does not have"),
        "{message}"
    );
    let message = unmet(|f| {
        f.features[0]["implementations"] =
            json!({ "q-1.0": [implementation("recipes/p/plain.json")] })
    });
    assert_eq!(message, "feature plain: player q-1.0 is not in the catalog");
    let message = unmet(|f| f.features[1]["labels"]["perform.pad"] = json!(["1", "2"]));
    assert!(
        message.contains("2 labels for perform.pad, which has 3 elements"),
        "{message}"
    );
}

#[test]
fn each_recipe_must_be_for_its_player_and_pin_its_output() {
    let refused = |change: fn(&mut Value)| {
        let mut fixture = Fixture::new();
        change(
            fixture
                .recipes
                .get_mut("recipes/p/plain.json")
                .expect("recipe"),
        );
        fixture.problem()
    };

    let message = refused(|r| r["target"]["release"] = json!("q-1.0"));
    assert!(
        message.contains("it is for release q-1.0, not p-1.0"),
        "{message}"
    );
    let message = refused(|r| r["target"]["application_sha256"] = json!(UPD));
    assert!(
        message.contains("target.application_sha256 differs"),
        "{message}"
    );
    let message = refused(|r| r["target"]["upd_sha256"] = json!(APP));
    assert!(message.contains("target.upd_sha256 differs"), "{message}");
    let message = refused(|r| r["expected"] = json!(null));
    assert!(message.contains("does not pin its output"), "{message}");

    // The second implementation of a feature is checked too.
    let mut fixture = Fixture::new();
    let table = fixture.recipes.get_mut("recipes/p/labelled-table.json");
    table.expect("recipe")["expected"] = json!(null);
    let message = fixture.problem();
    assert!(
        message.contains("recipe recipes/p/labelled-table.json: it does not pin its output"),
        "{message}"
    );

    let mut fixture = Fixture::new();
    fixture.recipes.remove("recipes/p/plain.json");
    assert_eq!(
        fixture.problem(),
        "feature plain: recipe recipes/p/plain.json is not in the catalog"
    );
}

#[test]
fn a_skin_must_fit_its_players_screen() {
    let refused = |change: fn(&mut Value)| {
        let mut fixture = Fixture::new();
        change(&mut fixture.skins[0]);
        fixture.problem()
    };

    let message = refused(|s| s["requires"]["screen_class"]["width"] = json!(1024));
    assert!(message.contains("has another screen class"), "{message}");
    let message = refused(|s| s["screen"] = json!("browse"));
    assert!(
        message.contains("player p-1.0 has no screen browse"),
        "{message}"
    );
    let message = refused(|s| s["requires"]["slots"] = json!(["pad", "jog"]));
    assert!(message.contains("has no slot jog"), "{message}");
    let message = refused(|s| s["implementations"]["p-1.0"][1]["labels"]["pad"] = json!(["1"]));
    assert!(
        message.contains("1 labels for perform.pad, which has 3 elements"),
        "{message}"
    );
}

#[test]
fn a_skins_recipes_are_its_own() {
    // A skin naming a feature's recipe: whose licence it is under would be unclear.
    let mut fixture = Fixture::new();
    fixture.skins[0]["implementations"]["p-1.0"][0]["recipe"] = json!("recipes/p/plain.json");
    assert_eq!(
        fixture.problem(),
        "skin dark-pads: recipe recipes/p/plain.json is also named by feature plain; a skin's \
         recipes are its own, under its licence"
    );

    // Two skins naming one recipe.
    let mut fixture = Fixture::new();
    let mut other = fixture.skins[0].clone();
    other["id"] = json!("light-pads");
    other["implementations"]["p-1.0"] = json!([fixture.skins[0]["implementations"]["p-1.0"][0]]);
    fixture.skins.push(other);
    let message = fixture.problem();
    assert!(
        message.contains("recipe recipes/p/stock-labels.json is also named by skin"),
        "{message}"
    );
}
