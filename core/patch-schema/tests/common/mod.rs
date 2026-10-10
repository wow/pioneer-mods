//! A small synthetic catalog for the catalog tests: one player with two screens, two
//! conflicting features (one relabelling a slot, with an implementation that draws its labels in
//! the stock style and one that leaves them to the skin) and a skin with two label sets.

// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

pub mod resolve;

use patch_schema::RecipeV2;
use patch_schema::catalog::{Catalog, CatalogError, CheckedCatalog};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub const UPD: &str = "1111111111111111111111111111111111111111111111111111111111111111";
pub const APP: &str = "2222222222222222222222222222222222222222222222222222222222222222";

pub struct Fixture {
    pub player: Value,
    pub screens: Vec<Value>,
    pub features: Vec<Value>,
    pub skins: Vec<Value>,
    pub recipes: BTreeMap<String, Value>,
}

pub fn parse<T: serde::de::DeserializeOwned>(values: &[Value]) -> Vec<T> {
    values
        .iter()
        .map(|value| serde_json::from_value(value.clone()).expect("parses"))
        .collect()
}

pub fn recipe(release: &str) -> Value {
    json!({
        "schema_version": 2, "recipe_id": "r", "description": "d",
        "target": { "release": release, "upd_sha256": UPD, "application_sha256": APP },
        "label": "Ver1.16", "reported_version": "0.12",
        "expected": { "application_sha256": APP }
    })
}

pub fn implementation(recipe: &str) -> Value {
    json!({ "recipe": recipe, "maturity": "experimental", "evidence": "rehearsed" })
}

pub fn skin(id: &str, screen: &str, slots: Value, implementations: Value) -> Value {
    json!({
        "schema_version": 1, "id": id, "title": id, "screen": screen,
        "requires": {
            "screen_class": { "width": 800, "height": 480, "pixels": "rgb565" },
            "slots": slots
        },
        "art": "original",
        "implementations": { "p-1.0": implementations },
        "maintainer": "m"
    })
}

impl Fixture {
    pub fn new() -> Self {
        let screen = |id: &str, slots: Value| {
            json!({ "schema_version": 1, "id": id, "player": "p-1.0", "title": id,
                    "slots": slots, "maintainer": "m" })
        };
        let feature = |id: &str, other: &str, implementations: Value| {
            json!({
                "schema_version": 1, "id": id, "title": id, "description": id,
                "requires": { "capabilities": { "lengths": ["32"] }, "slots": ["perform.pad"] },
                "conflicts": [other],
                "implementations": { "p-1.0": implementations },
                "maintainer": "m"
            })
        };
        let mut stock_labels = implementation("recipes/p/labelled.json");
        stock_labels["draws_labels"] = json!({ "perform": "stock" });
        let mut labelled = feature(
            "labelled",
            "plain",
            json!([
                stock_labels,
                implementation("recipes/p/labelled-table.json")
            ]),
        );
        labelled["labels"] = json!({ "perform.pad": ["1", "2", "32"] });
        let mut skin_labels = implementation("recipes/p/skin.json");
        skin_labels["labels"] = json!({ "pad": ["1", "2", "32"] });
        let skin_implementations =
            json!([implementation("recipes/p/stock-labels.json"), skin_labels]);
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
                feature(
                    "plain",
                    "labelled",
                    json!([implementation("recipes/p/plain.json")]),
                ),
                labelled,
            ],
            skins: vec![skin(
                "dark-pads",
                "perform",
                json!(["pad"]),
                skin_implementations,
            )],
            recipes: [
                "plain",
                "labelled",
                "labelled-table",
                "skin",
                "stock-labels",
            ]
            .into_iter()
            .map(|name| (format!("recipes/p/{name}.json"), recipe("p-1.0")))
            .collect(),
        }
    }

    pub fn catalog(&self) -> Catalog {
        Catalog {
            players: parse(std::slice::from_ref(&self.player)),
            screens: parse(&self.screens),
            features: parse(&self.features),
            skins: parse(&self.skins),
        }
    }

    pub fn parsed_recipes(&self) -> BTreeMap<String, RecipeV2> {
        self.recipes
            .iter()
            .map(|(path, value)| {
                (
                    path.clone(),
                    serde_json::from_value(value.clone()).expect("recipe"),
                )
            })
            .collect()
    }

    pub fn check(&self) -> Result<(), CatalogError> {
        self.catalog().check(&self.parsed_recipes())
    }

    pub fn checked(&self) -> CheckedCatalog {
        self.catalog()
            .into_checked(self.parsed_recipes())
            .expect("a consistent catalog")
    }

    pub fn problem(&self) -> String {
        self.check().expect_err("refused").to_string()
    }
}
