//! Each catalog file checked on its own (`validate`), and strict parsing.

mod common;

use common::Fixture;
use patch_schema::catalog::{EntryError, Feature, Player, Profile, Screen, Skin};
use serde_json::{Value, json};

fn feature(value: Value) -> Result<(), EntryError> {
    serde_json::from_value::<Feature>(value)
        .expect("parses")
        .validate()
}

fn player(value: Value) -> Result<(), EntryError> {
    serde_json::from_value::<Player>(value)
        .expect("parses")
        .validate()
}

fn skin(value: Value) -> Result<(), EntryError> {
    serde_json::from_value::<Skin>(value)
        .expect("parses")
        .validate()
}

fn is_rule(result: Result<(), EntryError>) -> bool {
    matches!(result, Err(EntryError::Rule { .. }))
}

#[test]
fn features_are_checked_on_their_own() {
    let fixture = Fixture::new();
    let plain = || fixture.features[0].clone();
    let labelled = || fixture.features[1].clone();
    assert_eq!(feature(plain()), Ok(()));
    assert_eq!(feature(labelled()), Ok(()));

    let mut bad = plain();
    bad["id"] = json!("Plain");
    assert!(matches!(feature(bad), Err(EntryError::Id { .. })));
    let mut bad = plain();
    bad["schema_version"] = json!(2);
    assert_eq!(feature(bad), Err(EntryError::SchemaVersion(2)));
    let mut bad = plain();
    bad["conflicts"] = json!(["plain"]);
    assert!(is_rule(feature(bad)));
    let mut bad = plain();
    bad["requires"]["slots"] = json!(["perform"]);
    assert!(matches!(feature(bad), Err(EntryError::SlotRef { .. })));
    let mut bad = plain();
    bad["requires"]["slots"] = json!(["perform.pad", "perform.pad"]);
    assert!(matches!(feature(bad), Err(EntryError::Repeated { .. })));
    // Labels for a slot the feature does not require, and labels drawn with none given.
    let mut bad = plain();
    bad["labels"] = json!({ "main.pad": ["1"] });
    assert!(is_rule(feature(bad)));
    let mut bad = plain();
    bad["implementations"]["p-1.0"][0]["draws_labels"] = json!("stock");
    assert!(is_rule(feature(bad)));
    // Two implementations for one player that draw the labels the same way.
    let mut bad = labelled();
    bad["implementations"]["p-1.0"][1]["draws_labels"] = json!("stock");
    assert!(is_rule(feature(bad)));
    let mut bad = plain();
    bad["implementations"]["p-1.0"] = json!([]);
    assert!(matches!(feature(bad), Err(EntryError::Empty { .. })));
    for label in ["", " 1", "a label far too long", "1\n"] {
        let mut bad = labelled();
        bad["labels"]["perform.pad"][0] = json!(label);
        assert!(
            matches!(feature(bad), Err(EntryError::Label { .. })),
            "{label:?}"
        );
    }
    let paths = [
        "../x.json",
        "recipes/../x.json",
        "recipes/./x.json",
        "/recipes/x.json",
        "recipes/x.txt",
        "recipes//x.json",
        "recipes/a\\..\\x.json",
        "recipes/C:x.json",
    ];
    for path in paths {
        let mut bad = plain();
        bad["implementations"]["p-1.0"][0]["recipe"] = json!(path);
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
fn players_and_screens_are_checked_on_their_own() {
    let fixture = Fixture::new();
    assert_eq!(player(fixture.player.clone()), Ok(()));

    // A capability gives values or the reason it is unavailable, not both or neither.
    let mut bad = fixture.player.clone();
    bad["capabilities"]["jog_display"]["values"] = json!(["1"]);
    assert!(is_rule(player(bad)));
    let mut bad = fixture.player.clone();
    bad["capabilities"]["lengths"]["values"] = json!([]);
    assert!(is_rule(player(bad)));
    let mut bad = fixture.player.clone();
    bad["capabilities"]["lengths"]["values"] = json!(["32", "32"]);
    assert!(matches!(player(bad), Err(EntryError::Repeated { .. })));
    let mut bad = fixture.player.clone();
    bad["firmware"]["upd_sha256"] = json!("abc");
    assert!(matches!(player(bad), Err(EntryError::Sha256 { .. })));
    let mut bad = fixture.player.clone();
    bad["firmware"]["file"] = json!("../P.UPD");
    assert!(is_rule(player(bad)));
    let mut bad = fixture.player.clone();
    bad["screen_class"]["height"] = json!(0);
    assert!(is_rule(player(bad)));

    let screen = |value: Value| {
        serde_json::from_value::<Screen>(value)
            .expect("parses")
            .validate()
    };
    assert_eq!(screen(fixture.screens[1].clone()), Ok(()));
    let mut bad = fixture.screens[1].clone();
    bad["slots"]["pad"]["count"] = json!(0);
    assert!(is_rule(screen(bad)));
}

#[test]
fn skins_are_checked_on_their_own() {
    let fixture = Fixture::new();
    assert_eq!(skin(fixture.skins[0].clone()), Ok(()));

    let mut bad = fixture.skins[0].clone();
    bad["id"] = json!("stock");
    assert!(is_rule(skin(bad)));
    // Two implementations for one player and label set.
    let mut bad = fixture.skins[0].clone();
    bad["implementations"]["p-1.0"][1]["labels"] = json!({});
    assert!(is_rule(skin(bad)));
    // Labels for a slot the skin does not require.
    let mut bad = fixture.skins[0].clone();
    bad["requires"]["slots"] = json!([]);
    assert!(is_rule(skin(bad)));
}

#[test]
fn profiles_are_checked_on_their_own() {
    let profile = |changes: Value| {
        let mut value = json!({
            "schema_version": 1, "player": "p-1.0", "screens": { "perform": "dark-pads" },
            "features": ["plain"], "label": "Ver1.16", "reported_version": "0.12"
        });
        for (key, change) in changes.as_object().expect("object") {
            value[key] = change.clone();
        }
        let profile = serde_json::from_value::<Profile>(value).expect("parses");
        profile.validate().map(|()| profile.maturity)
    };

    assert_eq!(
        profile(json!({})),
        Ok(patch_schema::catalog::Maturity::Stable)
    );
    assert!(matches!(
        profile(json!({ "maturity": "dev" })),
        Err(EntryError::Rule { .. })
    ));
    assert!(matches!(
        profile(json!({ "label": "1.16" })),
        Err(EntryError::Rule { .. })
    ));
    assert!(matches!(
        profile(json!({ "reported_version": "0.1" })),
        Err(EntryError::Rule { .. })
    ));
    assert!(matches!(
        profile(json!({ "features": ["plain", "plain"] })),
        Err(EntryError::Repeated { .. })
    ));
}

#[test]
fn a_key_given_twice_is_refused() {
    // serde would keep the last value of a repeated key; every map in the format refuses it.
    let implementations = r#"{
        "schema_version": 1, "id": "f", "title": "f", "description": "f",
        "implementations": {
            "p-1.0": [{ "recipe": "recipes/a.json", "maturity": "stable", "evidence": "e" }],
            "p-1.0": [{ "recipe": "recipes/b.json", "maturity": "dev", "evidence": "e" }]
        },
        "maintainer": "m"
    }"#;
    let error = serde_json::from_str::<Feature>(implementations).expect_err("refused");
    assert!(
        error.to_string().contains("\"p-1.0\" given twice"),
        "{error}"
    );

    let capabilities = r#"{
        "schema_version": 1, "id": "p", "model": "P",
        "firmware": { "file": "P.UPD", "upd_sha256": "x", "application_sha256": "x" },
        "screen_class": { "width": 1, "height": 1, "pixels": "rgb565" },
        "budgets": { "compressed_main_growth_bytes": 0 },
        "capabilities": {
            "lengths": { "values": ["16"], "evidence": "e" },
            "lengths": { "values": ["32"], "evidence": "e" }
        },
        "screens": ["main"], "maintainer": "m"
    }"#;
    let error = serde_json::from_str::<Player>(capabilities).expect_err("refused");
    assert!(
        error.to_string().contains("\"lengths\" given twice"),
        "{error}"
    );
}
