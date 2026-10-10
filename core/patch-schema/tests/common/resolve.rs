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
    resolve(&fixture.checked(), &profile).expect("resolves")
}

pub fn on(recipe: &str) -> Status {
    Status::On {
        recipe: recipe.to_owned(),
        maturity: Maturity::Experimental,
        evidence: "rehearsed".to_owned(),
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

/// A feature on `slots` with `labels`, one implementation per `draws` map, each its own recipe.
pub fn feature_on(fixture: &mut Fixture, id: &str, labels: Value, draws: &[Value]) {
    let mut feature = fixture.features[0].clone();
    feature["id"] = json!(id);
    feature["conflicts"] = json!([]);
    let slots: Vec<&String> = labels.as_object().expect("labels").keys().collect();
    feature["requires"]["slots"] = json!(slots);
    feature["labels"] = labels;
    let implementations: Vec<Value> = draws
        .iter()
        .enumerate()
        .map(|(index, draws)| {
            let recipe = format!("recipes/p/{id}-{index}.json");
            fixture
                .recipes
                .insert(recipe.clone(), super::recipe("p-1.0"));
            let mut implementation = super::implementation(&recipe);
            implementation["draws_labels"] = draws.clone();
            implementation
        })
        .collect();
    feature["implementations"]["p-1.0"] = json!(implementations);
    fixture.features.push(feature);
}

/// A skin `id` on `screen` requiring `slots`, with one implementation per label set.
pub fn skin_on(fixture: &mut Fixture, id: &str, screen: &str, slots: Value, label_sets: &[Value]) {
    let implementations: Vec<Value> = label_sets
        .iter()
        .enumerate()
        .map(|(index, labels)| {
            let recipe = format!("recipes/p/{id}-{index}.json");
            fixture
                .recipes
                .insert(recipe.clone(), super::recipe("p-1.0"));
            let mut implementation = super::implementation(&recipe);
            implementation["labels"] = labels.clone();
            implementation
        })
        .collect();
    fixture.skins.retain(|skin| skin["screen"] != json!(screen));
    fixture
        .skins
        .push(super::skin(id, screen, slots, json!(implementations)));
}

/// Slots `x` on main and `pad2` to `pad4` on perform, one element each.
pub fn with_more_slots() -> Fixture {
    let mut fixture = Fixture::new();
    fixture.screens[0]["slots"]["x"] = json!({ "count": 1, "description": "x" });
    for pad in ["pad2", "pad3", "pad4"] {
        fixture.screens[1]["slots"][pad] = json!({ "count": 1, "description": pad });
    }
    fixture
}
