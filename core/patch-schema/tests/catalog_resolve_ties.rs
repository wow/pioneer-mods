//! Resolving a profile: the order of screens, ties, and the reasons that name what decided, on
//! the synthetic catalog of `common` (see `catalog_resolve.rs`).

mod common;

use common::Fixture;
use common::resolve::{
    feature_on, off, on, perform, profile, recipes, resolved, skin_on, status, with_more_slots,
};
use patch_schema::catalog::{Resolution, Status};
use serde_json::{Value, json};

#[test]
fn the_result_does_not_depend_on_the_order_of_the_screens() {
    // `f` labels both screens; `dark-pads` draws `f` with `g`, or `h` alone; `main-look` draws no
    // labels. Both skins can be kept only with `h`.
    let build = |screens: Value| {
        let mut fixture = with_more_slots();
        fixture.player["screens"] = screens;
        let leave = json!({});
        feature_on(
            &mut fixture,
            "f",
            json!({ "main.x": ["F"], "perform.pad2": ["F2"] }),
            std::slice::from_ref(&leave),
        );
        feature_on(
            &mut fixture,
            "g",
            json!({ "perform.pad3": ["G"] }),
            std::slice::from_ref(&leave),
        );
        feature_on(
            &mut fixture,
            "h",
            json!({ "perform.pad4": ["H"] }),
            std::slice::from_ref(&leave),
        );
        skin_on(&mut fixture, "main-look", "main", json!([]), &[json!({})]);
        let both = json!({ "pad2": ["F2"], "pad3": ["G"] });
        let pads = json!(["pad2", "pad3", "pad4"]);
        skin_on(
            &mut fixture,
            "dark-pads",
            "perform",
            pads,
            &[both, json!({ "pad4": ["H"] })],
        );
        let skins = json!({ "main": "main-look", "perform": "dark-pads" });
        resolved(&fixture, profile(json!(["f", "g", "h"]), skins))
    };

    for screens in [json!(["main", "perform"]), json!(["perform", "main"])] {
        let resolution = build(screens);
        assert_eq!(status(&resolution, "h"), &on("recipes/p/h-0.json"));
        // The reason names the skin that cannot draw `f` at all, in either order.
        assert_eq!(
            off(&resolution, "f"),
            "skin main-look on main has no implementation for p-1.0 drawing its labels"
        );
        assert!(matches!(status(&resolution, "g"), Status::Off { .. }));
        let mut built = recipes(&resolution);
        built.sort_unstable();
        assert_eq!(
            built,
            [
                "recipes/p/dark-pads-1.json",
                "recipes/p/h-0.json",
                "recipes/p/main-look-0.json"
            ]
        );
    }
}

#[test]
fn of_two_skins_that_cannot_both_be_kept_the_one_keeping_more_features_stays() {
    // `wide` labels both screens and draws one of them in the stock style either way, so it
    // never fits two skins. `main-look` draws its main label; `dark-pads` draws no labels.
    let mut fixture = with_more_slots();
    feature_on(
        &mut fixture,
        "wide",
        json!({ "main.x": ["W"], "perform.pad2": ["W2"] }),
        &[json!({ "perform": "stock" }), json!({ "main": "stock" })],
    );
    skin_on(
        &mut fixture,
        "main-look",
        "main",
        json!(["x"]),
        &[json!({ "x": ["W"] })],
    );
    skin_on(
        &mut fixture,
        "dark-pads",
        "perform",
        json!([]),
        &[json!({})],
    );
    let skins = json!({ "main": "main-look", "perform": "dark-pads" });
    let resolution = resolved(&fixture, profile(json!(["wide"]), skins));

    assert_eq!(status(&resolution, "wide"), &on("recipes/p/wide-0.json"));
    assert_eq!(
        resolution.screens[0].status,
        on("recipes/p/main-look-0.json")
    );
    assert_eq!(
        perform(&resolution),
        &Status::Off {
            reason: "the chosen skins cannot all be kept; keeping skin main-look on main keeps \
                     more of the chosen features"
                .to_owned()
        }
    );
}

/// The fixture with a third screen, `browse`, with a slot `z`.
fn with_browse() -> Fixture {
    let mut fixture = with_more_slots();
    fixture.player["screens"] = json!(["main", "perform", "browse"]);
    let mut browse = fixture.screens[0].clone();
    browse["id"] = json!("browse");
    browse["slots"] = json!({ "z": { "count": 1, "description": "z" } });
    fixture.screens.push(browse);
    fixture
}

/// Two skins that cannot both be kept, each keeping one feature alone: `wide` labels both
/// screens and draws one of them in the stock style either way.
fn two_equal_skins(screens: Value) -> Resolution {
    let mut fixture = with_more_slots();
    fixture.player["screens"] = screens;
    feature_on(
        &mut fixture,
        "wide",
        json!({ "main.x": ["W"], "perform.pad2": ["W2"] }),
        &[json!({ "perform": "stock" }), json!({ "main": "stock" })],
    );
    skin_on(
        &mut fixture,
        "main-look",
        "main",
        json!(["x"]),
        &[json!({ "x": ["W"] })],
    );
    let pads = json!(["pad2"]);
    skin_on(
        &mut fixture,
        "dark-pads",
        "perform",
        pads,
        &[json!({ "pad2": ["W2"] })],
    );
    let skins = json!({ "main": "main-look", "perform": "dark-pads" });
    resolved(&fixture, profile(json!(["wide"]), skins))
}

#[test]
fn a_tie_between_skins_goes_to_the_first_screen_and_says_so() {
    let resolution = two_equal_skins(json!(["main", "perform"]));
    assert_eq!(status(&resolution, "wide"), &on("recipes/p/wide-0.json"));
    assert_eq!(
        perform(&resolution),
        &Status::Off {
            reason: "the chosen skins cannot all be kept; keeping skin main-look on main keeps \
                     as many of the chosen features and comes first in screen order"
                .to_owned()
        }
    );

    // The other order keeps the other skin.
    let resolution = two_equal_skins(json!(["perform", "main"]));
    assert_eq!(status(&resolution, "wide"), &on("recipes/p/wide-1.json"));
    let main = resolution.screens.iter().find(|s| s.screen == "main");
    assert!(matches!(main.expect("main").status, Status::Off { .. }));
}

#[test]
fn a_tie_between_implementations_goes_to_the_first() {
    // `dark-pads` draws `a`'s labels or `b`'s, not both: the first implementation is taken.
    let mut fixture = with_more_slots();
    feature_on(
        &mut fixture,
        "a",
        json!({ "perform.pad2": ["A"] }),
        &[json!({})],
    );
    feature_on(
        &mut fixture,
        "b",
        json!({ "perform.pad3": ["B"] }),
        &[json!({})],
    );
    let sets = [json!({ "pad2": ["A"] }), json!({ "pad3": ["B"] })];
    skin_on(
        &mut fixture,
        "dark-pads",
        "perform",
        json!(["pad2", "pad3"]),
        &sets,
    );
    let resolution = resolved(
        &fixture,
        profile(json!(["a", "b"]), json!({ "perform": "dark-pads" })),
    );

    assert_eq!(status(&resolution, "a"), &on("recipes/p/a-0.json"));
    assert_eq!(
        off(&resolution, "b"),
        "skin dark-pads on perform does not draw its labels in the implementation that keeps the \
         most of the chosen features"
    );
}

#[test]
fn a_skin_dropped_for_keeping_more_skins_says_so() {
    // `f` fits only with the stock skin on main and perform; `browse-look` draws only `f`'s
    // label. Keeping main-look and dark-pads (no feature) beats keeping browse-look (one).
    let mut fixture = with_browse();
    feature_on(
        &mut fixture,
        "f",
        json!({ "main.x": ["F"], "perform.pad2": ["F2"], "browse.z": ["F3"] }),
        &[json!({ "main": "stock", "perform": "stock" })],
    );
    skin_on(&mut fixture, "main-look", "main", json!([]), &[json!({})]);
    skin_on(
        &mut fixture,
        "dark-pads",
        "perform",
        json!([]),
        &[json!({})],
    );
    skin_on(
        &mut fixture,
        "browse-look",
        "browse",
        json!(["z"]),
        &[json!({ "z": ["F3"] })],
    );
    let skins = json!({ "main": "main-look", "perform": "dark-pads", "browse": "browse-look" });
    let resolution = resolved(&fixture, profile(json!(["f"]), skins));

    assert!(matches!(status(&resolution, "f"), Status::Off { .. }));
    assert_eq!(
        resolution.screens[2].status,
        Status::Off {
            reason: "the chosen skins cannot all be kept; keeping skin main-look on main and \
                     skin dark-pads on perform keeps more of the chosen skins"
                .to_owned()
        }
    );
}

#[test]
fn when_every_skin_could_draw_a_feature_the_reason_names_the_first_screen() {
    // Both skins have an implementation drawing `f`, but drawing `k` and `g` instead keeps two
    // features: `f` is off, and the reason names main, the first screen leaving it out.
    let mut fixture = with_more_slots();
    fixture.screens[0]["slots"]["y"] = json!({ "count": 1, "description": "y" });
    let leave = json!({});
    feature_on(
        &mut fixture,
        "f",
        json!({ "main.x": ["F"], "perform.pad2": ["F2"] }),
        std::slice::from_ref(&leave),
    );
    feature_on(
        &mut fixture,
        "g",
        json!({ "perform.pad3": ["G"] }),
        std::slice::from_ref(&leave),
    );
    feature_on(
        &mut fixture,
        "k",
        json!({ "main.y": ["K"] }),
        std::slice::from_ref(&leave),
    );
    let main_sets = [json!({ "x": ["F"] }), json!({ "y": ["K"] })];
    skin_on(
        &mut fixture,
        "main-look",
        "main",
        json!(["x", "y"]),
        &main_sets,
    );
    let pad_sets = [json!({ "pad2": ["F2"] }), json!({ "pad3": ["G"] })];
    skin_on(
        &mut fixture,
        "dark-pads",
        "perform",
        json!(["pad2", "pad3"]),
        &pad_sets,
    );
    let skins = json!({ "main": "main-look", "perform": "dark-pads" });
    let resolution = resolved(&fixture, profile(json!(["f", "g", "k"]), skins));

    assert!(matches!(status(&resolution, "g"), Status::On { .. }));
    assert!(matches!(status(&resolution, "k"), Status::On { .. }));
    assert_eq!(
        off(&resolution, "f"),
        "skin main-look on main does not draw its labels in the implementation that keeps the \
         most of the chosen features"
    );
}
