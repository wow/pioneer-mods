//! What acceptance tests rely on outside the test files: a player's panel controls and run-time
//! state ranges, and the brightness every runner computes.

mod common;

use common::Fixture;
use patch_schema::catalog::{
    Address, CatalogEntry, EntryError, Player, StateRange, brightness, expand_rgb565,
    mean_brightness,
};
use serde_json::{Value, json};

#[test]
fn controls_are_named_and_given_as_hexadecimal_bytes() {
    let player = |controls: Value| {
        let mut value = Fixture::new().player;
        value["controls"] = controls;
        serde_json::from_value::<Player>(value)
            .expect("parses")
            .validate()
    };
    let control =
        |code: &str, value: &str| json!({ "code": code, "value": value, "description": "d" });
    assert_eq!(
        player(json!({ "rotary_push": control("0x11", "0x01") })),
        Ok(())
    );
    for (code, value) in [
        ("0x1", "0x01"),
        ("11", "0x01"),
        ("0x11", "0x100"),
        ("0xzz", "0x01"),
        ("0x+5", "0x01"),
        ("0x11", "0x-1"),
    ] {
        let error = player(json!({ "usb": control(code, value) })).expect_err("refused");
        assert!(
            error.to_string().contains("0x and two hexadecimal digits"),
            "{error}"
        );
    }
    assert!(matches!(
        player(json!({ "USB": control("0x13", "0x04") })),
        Err(EntryError::Name { .. })
    ));
    let mut empty = control("0x13", "0x04");
    empty["description"] = json!(" ");
    assert!(matches!(
        player(json!({ "usb": empty })),
        Err(EntryError::Empty { .. })
    ));
}

#[test]
fn state_ranges_are_well_formed_and_backed_by_evidence() {
    let player = |ranges: Value| {
        let mut value = Fixture::new().player;
        value["state_memory"] = ranges;
        serde_json::from_value::<Player>(value)
            .expect("parses")
            .validate()
    };
    let range = |start: &str, end: &str| json!({ "start": start, "end": end, "evidence": "e" });
    assert_eq!(player(json!([range("0x0", "0x1")])), Ok(()));
    // A malformed address is refused when the file is read.
    for (start, end) in [
        ("10", "0x20"),
        ("0x10", "0x"),
        ("0x+1", "0x20"),
        ("0x123456789", "0x0"),
    ] {
        let mut value = Fixture::new().player;
        value["state_memory"] = json!([range(start, end)]);
        let error = serde_json::from_value::<Player>(value).expect_err("refused");
        assert!(
            error
                .to_string()
                .contains("an address is 0x and one to eight"),
            "{error}"
        );
    }
    // Ranges that overlap or touch are written as one.
    assert_eq!(
        player(json!([range("0x0", "0x10"), range("0x20", "0x30")])),
        Ok(())
    );
    for second in [
        range("0x10", "0x20"),
        range("0x8", "0x18"),
        range("0x0", "0x10"),
    ] {
        let error = player(json!([range("0x0", "0x10"), second])).expect_err("refused");
        assert!(
            error
                .to_string()
                .contains("overlaps or touches another range"),
            "{error}"
        );
    }
    let error = player(json!([range("0x20", "0x30"), range("0x0", "0x20")])).expect_err("refused");
    assert!(error.to_string().contains("state_memory[1]"), "{error}");
    for (start, end) in [("0x10", "0x10"), ("0x20", "0x10")] {
        let error = player(json!([range(start, end)])).expect_err("refused");
        assert!(
            error.to_string().contains("start below end"),
            "{start} {end}: {error}"
        );
    }
    let mut blank = range("0x0", "0x1");
    blank["evidence"] = json!("");
    assert!(matches!(
        player(json!([blank])),
        Err(EntryError::Empty { .. })
    ));
}

#[test]
fn brightness_is_rec_601_luma_of_8_bit_channels() {
    assert_eq!(brightness(0, 0, 0), 0);
    assert_eq!(brightness(255, 255, 255), 255);
    assert_eq!(brightness(255, 0, 0), 76);
    assert_eq!(brightness(0, 255, 0), 149);
    assert_eq!(brightness(0, 0, 255), 29);
    assert_eq!(brightness(10, 20, 30), 18);

    assert_eq!(expand_rgb565(0xffff), (255, 255, 255));
    assert_eq!(expand_rgb565(0x0000), (0, 0, 0));
    assert_eq!(expand_rgb565(0xf800), (255, 0, 0));
    assert_eq!(expand_rgb565(0x07e0), (0, 255, 0));
    assert_eq!(expand_rgb565(0x001f), (0, 0, 255));
    // 0b10000 in red, 0b100000 in green, 0b10000 in blue: the high bits replicated.
    assert_eq!(expand_rgb565(0x8410), (132, 130, 132));

    assert_eq!(mean_brightness([]), 0);
    assert_eq!(mean_brightness([(255, 255, 255), (0, 0, 0)]), 127);
    assert_eq!(mean_brightness([(255, 255, 255); 3]), 255);
}

#[test]
fn a_state_range_holds_the_bytes_a_check_reads() {
    let range = StateRange {
        start: Address(0x10),
        end: Address(0x16),
        evidence: "e".to_owned(),
    };
    assert!(range.holds(0x10, 4));
    assert!(range.holds(0x14, 2));
    assert!(range.holds(0x15, 1));
    assert!(!range.holds(0x14, 4), "two bytes past the end");
    assert!(!range.holds(0x16, 1), "the end is exclusive");
    assert!(!range.holds(0x0f, 1), "below the start");
    assert!(!range.holds(u32::MAX, 4));
}

#[test]
fn two_controls_may_not_send_the_same_event() {
    let mut value = Fixture::new().player;
    value["controls"]["usb_copy"] = value["controls"]["usb"].clone();
    let error = serde_json::from_value::<Player>(value)
        .expect("parses")
        .validate()
        .expect_err("refused");
    assert_eq!(
        error.to_string(),
        "controls.usb_copy: the same code and value as usb: a test pressing one would press both"
    );
}
