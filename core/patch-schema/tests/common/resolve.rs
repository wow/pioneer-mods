//! Helpers for the resolution tests: profiles, resolving the fixture, reading the result.

use super::Fixture;
use patch_schema::catalog::{Maturity, Profile, Resolution, Status, resolve};
use serde_json::{Value, json};

pub fn profile(features: Value, screens: Value) -> Value {
    json!({
        "schema_version": 1, "player": "p-1.0", "screens": screens, "features": features,
        "label": "Ver1.16", "reported_version": "0.12", "maturity": "experimental"
    })
}

pub fn resolved(fixture: &Fixture, profile: Value) -> Resolution {
    let profile: Profile = serde_json::from_value(profile).expect("profile");
    fixture.check().expect("a consistent catalog");
    resolve(&fixture.catalog(), &fixture.parsed_recipes(), &profile).expect("resolves")
}

pub fn on(recipe: &str) -> Status {
    Status::On {
        recipe: recipe.to_owned(),
        maturity: Maturity::Experimental,
        limits: Vec::new(),
    }
}

pub fn status<'a>(resolution: &'a Resolution, feature: &str) -> &'a Status {
    let found = resolution.features.iter().find(|f| f.feature == feature);
    &found.expect("listed").status
}

pub fn off(resolution: &Resolution, feature: &str) -> String {
    match status(resolution, feature) {
        Status::Off { reason } => reason.clone(),
        other => panic!("{feature} is {other:?}"),
    }
}

pub fn perform(resolution: &Resolution) -> &Status {
    &resolution.screens[1].status
}

pub fn recipes(resolution: &Resolution) -> Vec<&str> {
    resolution
        .fragments
        .iter()
        .map(|f| f.recipe.as_str())
        .collect()
}

/// `labelled` with only its stock-style implementation.
pub fn stock_style_only(fixture: &mut Fixture) {
    let stock_style = fixture.features[1]["implementations"]["p-1.0"][0].clone();
    fixture.features[1]["implementations"]["p-1.0"] = json!([stock_style]);
    fixture.recipes.remove("recipes/p/labelled-table.json");
}

/// The fixture with a skin `dark-pads` whose only implementation draws labels no feature gives.
pub fn skin_drawing_nothing_chosen() -> Fixture {
    let mut fixture = Fixture::new();
    let mut odd = super::implementation("recipes/p/skin.json");
    odd["labels"] = json!({ "pad": ["1", "2", "4"] });
    fixture.skins[0]["implementations"]["p-1.0"] = json!([odd]);
    fixture
}

/// The fixture with a second relabelling feature, `second`, on a new slot `perform.pad2`, which
/// leaves its labels to the skin.
pub fn with_second_feature() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.screens[1]["slots"]["pad2"] = json!({ "count": 1, "description": "pad 2" });
    let mut second = fixture.features[0].clone();
    second["id"] = json!("second");
    second["conflicts"] = json!([]);
    second["requires"]["slots"] = json!(["perform.pad2"]);
    second["labels"] = json!({ "perform.pad2": ["A"] });
    second["implementations"]["p-1.0"] = json!([super::implementation("recipes/p/second.json")]);
    fixture.features.push(second);
    fixture
        .recipes
        .insert("recipes/p/second.json".to_owned(), super::recipe("p-1.0"));
    fixture
}
