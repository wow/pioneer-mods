//! Resolving a profile: how chosen skins meet the features that leave their labels to them, on
//! the synthetic catalog of `common` (see `catalog_resolve.rs`).

mod common;

use common::resolve::{
    off, on, perform, profile, recipes, resolved, skin_drawing_nothing_chosen, skin_on, status,
    stock_style_only, with_more_slots, with_second_feature,
};
use common::{Fixture, implementation};
use patch_schema::catalog::{MAX_CONFIGURATIONS, Maturity, Profile, ResolveError, Status, resolve};
use serde_json::{Value, json};

#[test]
fn a_skin_draws_the_labels_a_feature_leaves_to_it() {
    let screens = json!({ "perform": "dark-pads" });
    let resolution = resolved(
        &Fixture::new(),
        profile(json!(["labelled"]), screens.clone()),
    );

    assert_eq!(
        status(&resolution, "labelled"),
        &on("recipes/p/labelled-table.json")
    );
    assert_eq!(perform(&resolution), &on("recipes/p/skin.json"));
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
fn a_kept_skin_switches_off_the_features_it_does_not_draw() {
    // `dark-pads` cannot draw `labelled`'s labels, but has an implementation without labels.
    let mut fixture = Fixture::new();
    fixture.skins[0]["implementations"]["p-1.0"][1]["labels"]["pad"] = json!(["1", "2", "4"]);
    let resolution = resolved(
        &fixture,
        profile(json!(["labelled"]), json!({ "perform": "dark-pads" })),
    );

    assert_eq!(
        off(&resolution, "labelled"),
        "skin dark-pads on perform has no implementation for p-1.0 drawing its labels; it has an \
         implementation drawing them in the stock style, for the stock skin on perform"
    );
    assert_eq!(perform(&resolution), &on("recipes/p/stock-labels.json"));

    // A feature whose only implementation draws its labels in the stock style.
    let mut fixture = Fixture::new();
    stock_style_only(&mut fixture);
    let resolution = resolved(
        &fixture,
        profile(json!(["labelled"]), json!({ "perform": "dark-pads" })),
    );
    assert_eq!(
        off(&resolution, "labelled"),
        "it draws the perform labels in the stock style, and perform uses skin dark-pads"
    );
}

#[test]
fn a_skin_that_can_draw_nothing_is_off_and_the_features_fit_stock_again() {
    // `labelled` with both implementations: it first leaves its labels to the skin.
    let resolution = resolved(
        &skin_drawing_nothing_chosen(),
        profile(json!(["labelled"]), json!({ "perform": "dark-pads" })),
    );
    assert_eq!(
        perform(&resolution),
        &Status::Off {
            reason: "skin dark-pads has no implementation for p-1.0 drawing the labels of some \
                     of the chosen features, or none"
                .to_owned()
        }
    );
    assert_eq!(
        status(&resolution, "labelled"),
        &on("recipes/p/labelled.json")
    );

    // `labelled` with only its stock-style implementation: it fits once the skin is off.
    let mut fixture = skin_drawing_nothing_chosen();
    stock_style_only(&mut fixture);
    let resolution = resolved(
        &fixture,
        profile(json!(["labelled"]), json!({ "perform": "dark-pads" })),
    );
    assert_eq!(
        perform(&resolution),
        &Status::Off {
            reason: "skin dark-pads has no implementation for p-1.0 without a feature's labels"
                .to_owned()
        }
    );
    assert_eq!(
        status(&resolution, "labelled"),
        &on("recipes/p/labelled.json")
    );
    assert_eq!(recipes(&resolution), ["recipes/p/labelled.json"]);
}

#[test]
fn a_kept_skin_draws_as_many_of_the_features_as_it_can() {
    let resolution = resolved(
        &with_second_feature(),
        profile(
            json!(["labelled", "second"]),
            json!({ "perform": "dark-pads" }),
        ),
    );

    // `dark-pads` draws `labelled`'s labels, not `second`'s: `labelled` stays on.
    assert_eq!(
        status(&resolution, "labelled"),
        &on("recipes/p/labelled-table.json")
    );
    assert_eq!(
        off(&resolution, "second"),
        "skin dark-pads on perform has no implementation for p-1.0 drawing its labels"
    );
    assert_eq!(perform(&resolution), &on("recipes/p/skin.json"));
}

#[test]
fn a_feature_whose_implementations_all_miss_lists_each_reason() {
    // A feature on two screens: each implementation draws one in the stock style and leaves the
    // other to a skin, so with the stock skin on both, none fits.
    let mut fixture = with_second_feature();
    fixture.screens[0]["slots"]["info"] = json!({ "count": 1, "description": "info" });
    let mut wide = fixture.features[2].clone();
    wide["id"] = json!("wide");
    wide["requires"]["slots"] = json!(["main.info", "perform.pad2"]);
    wide["labels"] = json!({ "main.info": ["W"], "perform.pad2": ["A"] });
    wide["conflicts"] = json!(["second"]);
    fixture.features[2]["conflicts"] = json!(["wide"]);
    let mut main_stock = implementation("recipes/p/second.json");
    main_stock["draws_labels"] = json!({ "main": "stock" });
    let mut perform_stock = implementation("recipes/p/plain.json");
    perform_stock["draws_labels"] = json!({ "perform": "stock" });
    wide["implementations"]["p-1.0"] = json!([main_stock, perform_stock]);
    fixture.features.push(wide);

    let resolution = resolved(&fixture, profile(json!(["wide"]), json!({})));
    assert_eq!(
        off(&resolution, "wide"),
        "none of its implementations for p-1.0 fits the skins: it leaves the perform labels to a \
         skin, and perform uses the stock skin, which draws none; it leaves the main labels to a \
         skin, and main uses the stock skin, which draws none"
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
        perform(&resolution),
        &Status::Off {
            reason: "skin dark-pads has no implementation for p-1.0 without a feature's labels"
                .to_owned()
        }
    );
    assert!(resolution.fragments.is_empty());
}

#[test]
fn the_stock_style_note_names_only_an_accepted_implementation() {
    // As above, but the stock-style implementation is `dev`: the note would name nothing usable.
    let mut fixture = Fixture::new();
    fixture.skins[0]["implementations"]["p-1.0"][1]["labels"]["pad"] = json!(["1", "2", "4"]);
    fixture.features[1]["implementations"]["p-1.0"][0]["maturity"] = json!("dev");
    let resolution = resolved(
        &fixture,
        profile(json!(["labelled"]), json!({ "perform": "dark-pads" })),
    );

    assert_eq!(
        off(&resolution, "labelled"),
        "skin dark-pads on perform has no implementation for p-1.0 drawing its labels"
    );
}

#[test]
fn a_search_too_large_is_refused() {
    // Two skins of 256 implementations each: 257 * 257 configurations, counting each skin left
    // out, is more than the bound.
    let mut fixture = with_more_slots();
    let label_sets = |slot: &str| -> Vec<Value> {
        (0..256)
            .map(|n| json!({ slot: [format!("{n}")] }))
            .collect()
    };
    skin_on(
        &mut fixture,
        "main-look",
        "main",
        json!(["x"]),
        &label_sets("x"),
    );
    skin_on(
        &mut fixture,
        "dark-pads",
        "perform",
        json!(["pad2"]),
        &label_sets("pad2"),
    );
    let profile: Profile = serde_json::from_value(profile(
        json!([]),
        json!({ "main": "main-look", "perform": "dark-pads" }),
    ))
    .expect("profile");
    assert_eq!(
        resolve(&fixture.checked(), &profile),
        Err(ResolveError::TooManyConfigurations(257 * 257))
    );
    const { assert!(257 * 257 > MAX_CONFIGURATIONS) };
}
