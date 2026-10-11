//! Each catalog file checked on its own (`validate`), and strict parsing.

mod common;

use common::Fixture;
use patch_schema::catalog::{CatalogEntry, EntryError, Feature, Player, Profile, Screen, Skin};
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
    bad["implementations"]["p-1.0"][0]["draws_labels"] = json!({ "perform": "stock" });
    assert!(is_rule(feature(bad)));
    // Labels drawn on a screen the feature gives none on.
    let mut bad = labelled();
    bad["implementations"]["p-1.0"][0]["draws_labels"] = json!({ "main": "stock" });
    assert!(is_rule(feature(bad)));
    // Only the stock style, for now: a skin cannot yet leave slots for a feature to draw.
    let mut bad = labelled();
    bad["implementations"]["p-1.0"][0]["draws_labels"] = json!({ "perform": "dark-pads" });
    assert!(is_rule(feature(bad)));
    // Text shown to the owner holds no control character.
    // Nor a format or separator character: a bidirectional override, a line separator, a
    // zero-width space.
    for text in [
        "one line\ntier: stable",
        "x\u{202E}y",
        "x\u{2028}y",
        "x\u{200B}y",
    ] {
        let mut bad = plain();
        bad["implementations"]["p-1.0"][0]["limits"] = json!([text]);
        assert!(
            matches!(feature(bad), Err(EntryError::Control { .. })),
            "{text:?}"
        );
    }
    // Other text, accents and dashes included, is fine.
    let mut fine = plain();
    fine["implementations"]["p-1.0"][0]["limits"] = json!(["Déjà vu – fine"]);
    assert_eq!(feature(fine), Ok(()));
    // Two implementations for one player that draw the labels the same way.
    let mut bad = labelled();
    bad["implementations"]["p-1.0"][1]["draws_labels"] = json!({ "perform": "stock" });
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
    for guide in [
        "../docs/p.md",
        "docs/p.txt",
        "docs/../p.md",
        "notes/p.md",
        "docs\\p.md",
    ] {
        let mut bad = fixture.player.clone();
        bad["restore"]["guide"] = json!(guide);
        assert!(is_rule(player(bad)), "{guide}");
    }
    let mut bad = fixture.player.clone();
    bad["restore"]["backup"] = json!("one line\nanother");
    assert!(player(bad).is_err());

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

    // Its authors: at least one, each named once, in plain text.
    let mut bad = fixture.skins[0].clone();
    bad["authors"] = json!([]);
    assert!(matches!(skin(bad), Err(EntryError::Empty { .. })));
    let mut bad = fixture.skins[0].clone();
    bad["authors"] = json!(["a", "a"]);
    assert!(matches!(skin(bad), Err(EntryError::Repeated { .. })));
    let mut bad = fixture.skins[0].clone();
    bad["authors"] = json!(["a\nb"]);
    assert!(skin(bad).is_err());
    // Its licence: the one community skins are published under.
    let mut bad = fixture.skins[0].clone();
    bad["licence"] = json!("MIT");
    assert!(serde_json::from_value::<Skin>(bad).is_err());
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

/// The parse error of `text` as a `T`.
fn parse_error<T: serde::de::DeserializeOwned + std::fmt::Debug>(text: &str) -> String {
    serde_json::from_str::<T>(text)
        .expect_err("refused")
        .to_string()
}

#[test]
fn every_other_map_refuses_a_key_given_twice() {
    let screen = r#"{ "schema_version": 1, "id": "s", "player": "p", "title": "t",
        "slots": {
            "a": { "count": 1, "description": "d" },
            "a": { "count": 2, "description": "d" }
        },
        "maintainer": "m" }"#;
    let labels = r#"{ "schema_version": 1, "id": "f", "title": "f", "description": "f",
        "labels": { "s.a": ["1"], "s.a": ["2"] }, "implementations": {}, "maintainer": "m" }"#;
    let requires = r#"{ "schema_version": 1, "id": "f", "title": "f", "description": "f",
        "requires": { "capabilities": { "x": [], "x": ["1"] } },
        "implementations": {}, "maintainer": "m" }"#;
    let skin = |implementations: &str| {
        format!(
            r#"{{ "schema_version": 1, "id": "k", "title": "k", "screen": "s",
            "requires": {{ "screen_class": {{ "width": 1, "height": 1, "pixels": "rgb565" }} }},
            "art": "original", "implementations": {implementations}, "maintainer": "m" }}"#
        )
    };
    let skin_players = skin(r#"{ "p": [], "p": [] }"#);
    let skin_labels = skin(
        r#"{ "p": [{ "recipe": "recipes/a.json", "maturity": "dev", "evidence": "e",
        "labels": { "a": ["1"], "a": ["2"] } }] }"#,
    );
    let profile = r#"{ "schema_version": 1, "player": "p",
        "screens": { "main": "stock", "main": "dark" },
        "label": "Ver1.16", "reported_version": "0.12" }"#;

    let errors = [
        ("screen.slots", parse_error::<Screen>(screen)),
        ("feature.labels", parse_error::<Feature>(labels)),
        ("requires.capabilities", parse_error::<Feature>(requires)),
        ("skin.implementations", parse_error::<Skin>(&skin_players)),
        ("skin labels", parse_error::<Skin>(&skin_labels)),
        ("profile.screens", parse_error::<Profile>(profile)),
    ];
    for (map, error) in errors {
        assert!(error.contains("given twice"), "{map}: {error}");
    }
}
