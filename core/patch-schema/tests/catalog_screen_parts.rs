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
        &|p| p["drawn"][0]["images"][0]["height"] = 0.into(),
        "an image has pixels",
    );
    refused(&|p| p["slot"] = "jog".into(), "the screen has no slot jog");
    refused(&|p| p["kind"] = "knob".into(), "unknown variant");
    refused(&|p| p["extra"] = 1.into(), "unknown field");

    // Each zone, placement and image once.
    let again = |p: &mut Value, list: &str| {
        let first = p[list][0].clone();
        p[list].as_array_mut().expect("list").push(first);
    };
    refused(
        &|p| again(p, "touch"),
        "another zone of the part has this area and `when`",
    );
    refused(
        &|p| again(p, "drawn"),
        "another placement of the part is at this position",
    );
    refused(
        &|p| {
            let image = p["drawn"][0]["images"][0].clone();
            p["drawn"][0]["images"]
                .as_array_mut()
                .expect("images")
                .push(image);
        },
        "the placement names this image already",
    );

    // Tables: 1, 2 or 4 bytes an entry, at least one, each value fitting.
    refused(
        &with_table(|t| {
            t["entry_bytes"] = 3.into();
            t["stride"] = 4.into();
        }),
        "an entry is 1, 2 or 4 bytes",
    );
    refused(
        &with_table(|t| t["stride"] = 1.into()),
        "`stride` is at least that",
    );
    refused(
        &with_table(|t| t["values"] = json!([])),
        "a table has an entry",
    );
    refused(
        &with_table(|t| t["values"] = json!([65_536])),
        "a value fits in `entry_bytes`",
    );
    refused(
        &with_table(|t| t["what"] = "".into()),
        "parts.rev.tables[0].what",
    );
    let mut tabled = part();
    with_table(|_| ())(&mut tabled);
    let loaded = checked(screen(json!({ "rev": tabled }))).expect("valid");
    let entries: Vec<_> = loaded.parts["rev"].tables[0].entries().collect();
    assert_eq!(entries, [0xd6250..0xd6252, 0xd6252..0xd6254]);
}

/// A two-entry table of 16-bit edges, changed by `change`, given to a part.
fn with_table(change: fn(&mut Value)) -> impl Fn(&mut Value) {
    move |p: &mut Value| {
        p["tables"] = json!([{ "offset": "0xd6250", "entry_bytes": 2, "stride": 2,
                               "values": [83, 166], "what": "Edges." }]);
        change(&mut p["tables"][0]);
    }
}

/// Part `rev` of the fixture's perform screen.
fn rev(fixture: &mut Fixture) -> &mut Value {
    &mut fixture.screens[1]["parts"]["rev"]
}

/// The common fixture with a touch layout of 20-byte records and part `rev` on its perform
/// screen, changed by `change`.
fn fixture(change: &dyn Fn(&mut Fixture)) -> Fixture {
    let mut fixture = Fixture::new();
    fixture.player["touch_layout"] = json!({ "record_bytes": 20, "record_alignment": 4,
                                             "handler_alignment": 2, "evidence": "Measured." });
    let mut value = part();
    value["touch"][0]["record"] = "0x1000".into();
    value["drawn"][0]["images"][0]["offset"] = "0x2000".into();
    fixture.screens[1]["parts"] = json!({ "rev": value });
    change(&mut fixture);
    fixture
}

fn problem(change: &dyn Fn(&mut Fixture), expected: &str) {
    let message = fixture(change).problem();
    assert!(message.contains(expected), "{expected}: {message}");
}

#[test]
fn a_part_holds_to_its_players_screen_pixels_and_touch_layout() {
    assert_eq!(fixture(&|_| ()).check(), Ok(()));
    problem(
        &|f| rev(f)["touch"][0]["area"]["x"] = 733.into(),
        "part rev: touch[0] is not on the 800x480 screen",
    );
    problem(
        &|f| rev(f)["drawn"][0]["at"]["y"] = 428.into(),
        "part rev: drawn[0].images[0] is not on the 800x480 screen",
    );
    problem(
        &|f| rev(f)["touch"][0]["record"] = "0x1002".into(),
        "touch[0].record is not 4-byte aligned",
    );
    problem(
        &|f| rev(f)["touch"][0]["handler"] = "0x8dd0b11".into(),
        "touch[0].handler is not 2-byte aligned",
    );
    problem(
        &|f| rev(f)["drawn"][0]["images"][0]["offset"] = "0x2001".into(),
        "is not 2-byte aligned",
    );
    problem(
        &|f| rev(f)["drawn"][0]["images"][0]["width"] = 10_000.into(),
        "is over 1 MiB",
    );
    problem(
        &|f| {
            f.player
                .as_object_mut()
                .expect("player")
                .remove("touch_layout");
        },
        "part rev: player p-1.0 does not describe its touch layout",
    );
    // A player's touch layout has a size and power-of-two alignments.
    let mut player = fixture(&|_| ());
    player.player["touch_layout"]["record_alignment"] = 3.into();
    assert!(player.problem().contains("alignments are powers of two"));
}

#[test]
fn a_players_touch_records_are_described_once() {
    let second = |record: &str, area_x: u32, screen: usize| {
        let record = record.to_owned();
        move |f: &mut Fixture| {
            let mut other = part();
            other["touch"][0]["record"] = record.clone().into();
            other["touch"][0]["area"]["x"] = area_x.into();
            other["drawn"][0]["images"][0]["offset"] = "0x3000".into();
            f.screens[screen]["parts"]["sync"] = other;
        }
    };
    // On one screen, records 20 bytes apart or more; never one record twice or overlapping.
    assert_eq!(fixture(&second("0x1014", 100, 1)).check(), Ok(()));
    assert_eq!(fixture(&second("0x0fec", 100, 1)).check(), Ok(()));
    for record in ["0x1000", "0x1010", "0x0ff4"] {
        problem(&second(record, 100, 1), "the touch records of perform/");
    }
    // One record described alike by two parts of one screen is still twice.
    problem(&second("0x1000", 18, 1), "the touch records of perform/");
    // With records 1-byte aligned, an overlap of one byte.
    problem(
        &|f| {
            f.player["touch_layout"]["record_alignment"] = 1.into();
            second("0x1013", 100, 1)(f);
        },
        "the touch records of perform/",
    );
    // Across screens sharing the layout: one record, described alike, or none overlapping.
    assert_eq!(fixture(&second("0x1000", 18, 0)).check(), Ok(()));
    problem(
        &second("0x1000", 19, 0),
        "overlap and are described differently",
    );
    problem(
        &second("0x1010", 18, 0),
        "overlap and are described differently",
    );
    // A bad part name is refused as a name, before any overlap is reported.
    let mut named = fixture(&second("0x1000", 100, 1));
    let parts = named.screens[1]["parts"].as_object_mut().expect("parts");
    let sync = parts.remove("sync").expect("sync");
    named.screens[1]["parts"]["sy\u{202e}nc"] = sync;
    let message = named.problem();
    assert!(message.contains("parts"), "{message}");
    assert!(!message.contains("overlap"), "{message}");
}
