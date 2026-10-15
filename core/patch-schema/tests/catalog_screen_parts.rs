//! A screen's parts (`parts.<name>`): where each is touched and drawn, its images, what it does.
//! Needs no firmware: records, handlers, offsets and positions only.

mod common;

use common::Fixture;
use patch_schema::catalog::{CatalogEntry, PartKind, Screen};
use serde_json::{Value, json};

fn part() -> Value {
    json!({
        "title": "REV", "kind": "button",
        "touch": [{ "record": "0xd5edc", "area": { "x": 18, "y": 76, "w": 68, "h": 53 },
                    "handler": "0x8dd0b10" }],
        "drawn": [{ "at": { "x": 18, "y": 78 }, "images": [
            { "offset": "0x808b80", "width": 72, "height": 53, "seen": "a track loaded" }] }],
        "does": "Plays the track in reverse while on.",
        "evidence": "Measured."
    })
}

fn screen(parts: Value) -> Value {
    json!({
        "schema_version": 1, "id": "perform", "player": "xdj700-v1.15", "title": "PERFORM",
        "slots": { "pad": { "count": 6, "description": "Pads." } },
        "parts": parts,
        "maintainer": "m"
    })
}

fn checked(value: Value) -> Result<Screen, String> {
    let screen: Screen = serde_json::from_value(value).map_err(|error| error.to_string())?;
    screen.validate().map_err(|error| error.to_string())?;
    Ok(screen)
}

#[test]
fn a_part_names_its_touch_zones_placements_and_images() {
    let mut two = part();
    let zone = json!({ "record": "0xd5ea0", "area": { "x": 642, "y": 154, "w": 70, "h": 53 },
                       "handler": "0x8dd0b20", "when": "while a loop plays" });
    two["touch"].as_array_mut().expect("touch").push(zone);
    let place = json!({ "at": { "x": 330, "y": 232 }, "images": [
        { "offset": "0x808b80", "width": 72, "height": 53, "seen": "always" }] });
    two["drawn"].as_array_mut().expect("drawn").push(place);
    let pads = json!({ "title": "Pads", "kind": "buttons", "slot": "pad",
                       "does": "Loops.", "evidence": "Measured." });
    let loaded = checked(screen(json!({ "rev": two, "pads": pads }))).expect("valid");
    let rev = &loaded.parts["rev"];
    assert_eq!(rev.kind, PartKind::Button);
    assert_eq!(rev.touch[1].when.as_deref(), Some("while a loop plays"));
    assert_eq!(rev.drawn[1].at.x, 330);
    assert_eq!(rev.drawn[0].images[0].len(), 72 * 53 * 2);
    assert_eq!(loaded.parts["pads"].slot.as_deref(), Some("pad"));

    // Without parts, a screen is as before, and writes none back.
    let mut plain = screen(json!({}));
    plain.as_object_mut().expect("screen").remove("parts");
    let written = serde_json::to_value(checked(plain).expect("valid")).expect("json");
    assert!(written.get("parts").is_none());
}

#[test]
fn malformed_parts_are_refused() {
    let refused = |change: &dyn Fn(&mut Value), problem: &str| {
        let mut value = part();
        change(&mut value);
        let error = checked(screen(json!({ "rev": value }))).expect_err("refused");
        assert!(error.contains(problem), "{problem}: {error}");
    };
    refused(&|p| p["title"] = " ".into(), "parts.rev.title");
    refused(
        &|p| {
            let object = p.as_object_mut().expect("part");
            object.remove("touch");
            object.remove("drawn");
        },
        "a part is touched, drawn or names a slot",
    );
    refused(
        &|p| p["touch"][0]["record"] = "0xd5ede".into(),
        "4-byte aligned",
    );
    refused(
        &|p| p["touch"][0]["handler"] = "0xdd0b10".into(),
        "run-time address",
    );
    refused(
        &|p| p["touch"][0]["handler"] = "0x8dd0b11".into(),
        "run-time address",
    );
    refused(
        &|p| p["touch"][0]["area"]["w"] = 0.into(),
        "a touch area is not empty",
    );
    refused(
        &|p| p["touch"][0]["when"] = "".into(),
        "parts.rev.touch[0].when",
    );
    refused(
        &|p| p["drawn"][0]["images"] = json!([]),
        "a placement names an image",
    );
    refused(
        &|p| p["drawn"][0]["images"][0]["offset"] = "0x808b81".into(),
        "2-byte aligned",
    );
    refused(
        &|p| p["drawn"][0]["images"][0]["height"] = 0.into(),
        "1 byte to 1 MiB",
    );
    refused(
        &|p| p["drawn"][0]["images"][0]["width"] = 10_000.into(),
        "1 byte to 1 MiB",
    );
    refused(&|p| p["slot"] = "jog".into(), "the screen has no slot jog");
    refused(&|p| p["kind"] = "knob".into(), "unknown variant");
    refused(&|p| p["extra"] = 1.into(), "unknown field");
}

#[test]
fn a_parts_touch_areas_and_images_lie_on_its_players_screen() {
    // The common fixture's perform screen (800x480) gains a part.
    let with = |change: &dyn Fn(&mut Value)| {
        let mut fixture = Fixture::new();
        let mut value = part();
        value["touch"][0]["record"] = "0x1000".into();
        value["drawn"][0]["images"][0]["offset"] = "0x2000".into();
        change(&mut value);
        fixture.screens[1]["parts"] = json!({ "rev": value });
        fixture
    };
    assert_eq!(with(&|_| ()).check(), Ok(()));
    let message = with(&|p| p["touch"][0]["area"]["x"] = 733.into()).problem();
    assert!(
        message.contains("part rev: touch[0] is not on the 800x480 screen"),
        "{message}"
    );
    let message = with(&|p| p["drawn"][0]["at"]["y"] = 428.into()).problem();
    assert!(
        message.contains("part rev: drawn[0].images[0] is not on the 800x480 screen"),
        "{message}"
    );
}
