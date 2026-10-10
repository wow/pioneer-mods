//! Catalog files on their own, and the catalog as a whole, on a small synthetic catalog: one
//! player with two screens, two conflicting features and a skin.

use patch_schema::RecipeV2;
use patch_schema::catalog::{
    Catalog, CatalogError, EntryError, Feature, Player, Profile, Screen, Skin,
};
use serde_json::{Value, json};
use std::collections::BTreeMap;

const UPD: &str = "1111111111111111111111111111111111111111111111111111111111111111";
const APP: &str = "2222222222222222222222222222222222222222222222222222222222222222";

struct Fixture {
    player: Value,
    screens: Vec<Value>,
    features: Vec<Value>,
    skins: Vec<Value>,
    recipes: BTreeMap<String, Value>,
}

fn parse<T: serde::de::DeserializeOwned>(values: &[Value]) -> Vec<T> {
    values
        .iter()
        .map(|value| serde_json::from_value(value.clone()).expect("parses"))
        .collect()
}

fn recipe(release: &str) -> Value {
    json!({
        "schema_version": 2, "recipe_id": "r", "description": "d",
        "target": { "release": release, "upd_sha256": UPD, "application_sha256": APP },
        "label": "Ver1.16", "reported_version": "0.12",
        "expected": { "application_sha256": APP }
    })
}

fn implementation(recipe: &str) -> Value {
    json!({ "recipe": recipe, "maturity": "experimental", "evidence": "rehearsed" })
}

impl Fixture {
    fn new() -> Self {
        let screen = |id: &str, slots: Value| {
            json!({ "schema_version": 1, "id": id, "player": "p-1.0", "title": id,
                    "slots": slots, "maintainer": "m" })
        };
        let feature = |id: &str, other: &str, recipe: &str| {
            json!({
                "schema_version": 1, "id": id, "title": id, "description": id,
                "requires": { "capabilities": { "lengths": ["32"] }, "slots": ["perform.pad"] },
                "conflicts": [other],
                "implementations": { "p-1.0": implementation(recipe) },
                "maintainer": "m"
            })
        };
        let mut labelled = feature("labelled", "plain", "recipes/p/labelled.json");
        labelled["labels"] = json!({ "perform.pad": ["1", "2", "32"] });
        labelled["implementations"]["p-1.0"]["draws_labels"] = json!("stock");
        let mut skin_implementation = implementation("recipes/p/skin.json");
        skin_implementation["labels"] = json!({ "pad": ["1", "2", "32"] });
        Fixture {
            player: json!({
                "schema_version": 1, "id": "p-1.0", "model": "P",
                "firmware": { "file": "P.UPD", "upd_sha256": UPD, "application_sha256": APP },
                "screen_class": { "width": 800, "height": 480, "pixels": "rgb565" },
                "budgets": { "compressed_main_growth_bytes": 1024 },
                "capabilities": {
                    "lengths": { "values": ["16", "32"], "evidence": "e" },
                    "jog_display": { "unavailable": "absent on this model", "evidence": "e" }
                },
                "screens": ["main", "perform"],
                "maintainer": "m"
            }),
            screens: vec![
                screen("main", json!({})),
                screen(
                    "perform",
                    json!({ "pad": { "count": 3, "description": "pads" } }),
                ),
            ],
            features: vec![
                feature("plain", "labelled", "recipes/p/plain.json"),
                labelled,
            ],
            skins: vec![json!({
                "schema_version": 1, "id": "dark-pads", "title": "Dark pads", "screen": "perform",
                "requires": {
                    "screen_class": { "width": 800, "height": 480, "pixels": "rgb565" },
                    "slots": ["pad"]
                },
                "art": "original",
                "implementations": {
                    "p-1.0": [implementation("recipes/p/stock-labels.json"), skin_implementation]
                },
                "maintainer": "m"
            })],
            recipes: ["plain", "labelled", "skin", "stock-labels"]
                .into_iter()
                .map(|name| (format!("recipes/p/{name}.json"), recipe("p-1.0")))
                .collect(),
        }
    }

    fn check(&self) -> Result<(), CatalogError> {
        let catalog = Catalog {
            players: parse(std::slice::from_ref(&self.player)),
            screens: parse(&self.screens),
            features: parse(&self.features),
            skins: parse(&self.skins),
        };
        let recipes: BTreeMap<String, RecipeV2> = self
            .recipes
            .iter()
            .map(|(path, v)| {
                (
                    path.clone(),
                    serde_json::from_value(v.clone()).expect("recipe"),
                )
            })
            .collect();
        catalog.check(&recipes)
    }

    fn problem(&self) -> String {
        self.check().expect_err("refused").to_string()
    }
}

#[test]
fn a_consistent_catalog_checks() {
    assert_eq!(Fixture::new().check(), Ok(()));
}

fn feature(value: Value) -> Result<(), EntryError> {
    serde_json::from_value::<Feature>(value)
        .expect("parses")
        .validate()
}

#[test]
fn files_are_checked_on_their_own() {
    let fixture = Fixture::new();
    let plain = || fixture.features[0].clone();

    let mut bad = plain();
    bad["id"] = json!("Plain");
    assert!(matches!(feature(bad), Err(EntryError::Id { .. })));
    let mut bad = plain();
    bad["schema_version"] = json!(2);
    assert_eq!(feature(bad), Err(EntryError::SchemaVersion(2)));
    let mut bad = plain();
    bad["conflicts"] = json!(["plain"]);
    assert!(matches!(feature(bad), Err(EntryError::Rule { .. })));
    let mut bad = plain();
    bad["requires"]["slots"] = json!(["perform"]);
    assert!(matches!(feature(bad), Err(EntryError::SlotRef { .. })));
    // Labels for a slot the feature does not require, and labels drawn with none given.
    let mut bad = plain();
    bad["labels"] = json!({ "main.pad": ["1"] });
    assert!(matches!(feature(bad), Err(EntryError::Rule { .. })));
    let mut bad = plain();
    bad["implementations"]["p-1.0"]["draws_labels"] = json!("stock");
    assert!(matches!(feature(bad), Err(EntryError::Rule { .. })));
    for label in ["", " 1", "a label far too long", "1\n"] {
        let mut bad = fixture.features[1].clone();
        bad["labels"]["perform.pad"][0] = json!(label);
        assert!(
            matches!(feature(bad), Err(EntryError::Label { .. })),
            "{label:?}"
        );
    }
    for path in [
        "../x.json",
        "recipes/../x.json",
        "/recipes/x.json",
        "recipes/x.txt",
        "recipes//x.json",
    ] {
        let mut bad = plain();
        bad["implementations"]["p-1.0"]["recipe"] = json!(path);
        assert!(
            matches!(feature(bad), Err(EntryError::RecipePath { .. })),
            "{path}"
        );
    }
    // Unknown fields are refused while parsing.
    let mut bad = plain();
    bad["provides"] = json!([]);
    assert!(serde_json::from_value::<Feature>(bad).is_err());
}

#[test]
fn players_skins_and_profiles_are_checked_on_their_own() {
    let fixture = Fixture::new();
    let player = |value: Value| {
        serde_json::from_value::<Player>(value)
            .expect("parses")
            .validate()
    };
    let skin = |value: Value| {
        serde_json::from_value::<Skin>(value)
            .expect("parses")
            .validate()
    };

    // A capability gives values or the reason it is unavailable, not both or neither.
    let mut bad = fixture.player.clone();
    bad["capabilities"]["jog_display"]["values"] = json!(["1"]);
    assert!(matches!(player(bad), Err(EntryError::Rule { .. })));
    let mut bad = fixture.player.clone();
    bad["capabilities"]["lengths"]["values"] = json!([]);
    assert!(matches!(player(bad), Err(EntryError::Rule { .. })));
    let mut bad = fixture.player.clone();
    bad["firmware"]["upd_sha256"] = json!("abc");
    assert!(matches!(player(bad), Err(EntryError::Sha256 { .. })));
    let mut bad = fixture.player.clone();
    bad["firmware"]["file"] = json!("../P.UPD");
    assert!(matches!(player(bad), Err(EntryError::Rule { .. })));

    let mut bad = fixture.skins[0].clone();
    bad["id"] = json!("stock");
    assert!(matches!(skin(bad), Err(EntryError::Rule { .. })));
    // Two implementations for one player and label set.
    let mut bad = fixture.skins[0].clone();
    bad["implementations"]["p-1.0"][1]["labels"] = json!({});
    assert!(matches!(skin(bad), Err(EntryError::Rule { .. })));

    let profile = |changes: Value| {
        let mut value = json!({
            "schema_version": 1, "player": "p-1.0", "screens": { "perform": "dark-pads" },
            "features": ["plain"], "label": "Ver1.16", "reported_version": "0.12"
        });
        for (key, change) in changes.as_object().expect("object") {
            value[key] = change.clone();
        }
        serde_json::from_value::<Profile>(value)
            .expect("parses")
            .validate()
    };
    assert_eq!(profile(json!({})), Ok(()));
    assert!(matches!(
        profile(json!({ "maturity": "dev" })),
        Err(EntryError::Rule { .. })
    ));
    assert!(matches!(
        profile(json!({ "label": "1.16" })),
        Err(EntryError::Rule { .. })
    ));
    assert!(matches!(
        profile(json!({ "features": ["plain", "plain"] })),
        Err(EntryError::Repeated { .. })
    ));
    let screen = serde_json::from_value::<Screen>(fixture.screens[1].clone()).expect("parses");
    assert_eq!(screen.validate(), Ok(()));
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
            json!({ "q-1.0": implementation("recipes/p/plain.json") })
    });
    assert_eq!(message, "feature plain: player q-1.0 is not in the catalog");
    let message = unmet(|f| f.features[1]["labels"]["perform.pad"] = json!(["1", "2"]));
    assert!(
        message.contains("2 labels for perform.pad, which has 3 elements"),
        "{message}"
    );
    let message =
        unmet(|f| f.features[1]["implementations"]["p-1.0"]["draws_labels"] = json!("missing"));
    assert_eq!(
        message,
        "feature labelled: skin missing is not in the catalog"
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
    let message = refused(|r| r["expected"] = json!(null));
    assert!(message.contains("does not pin its output"), "{message}");

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
