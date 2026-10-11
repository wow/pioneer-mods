//! Acceptance tests as catalog entries: each file checked on its own, then against its player and
//! the implementations that name it, on the synthetic catalog of `common`.

mod common;

use common::{Fixture, acceptance_test};
use patch_schema::catalog::{
    AcceptanceTest, CatalogEntry, Check, EntryError, Region, Step, parse_address,
};
use serde_json::{Value, json};

fn validated(value: Value) -> Result<(), EntryError> {
    serde_json::from_value::<AcceptanceTest>(value)
        .expect("parses")
        .validate()
}

fn refused(value: Value) -> String {
    validated(value).expect_err("refused").to_string()
}

/// The fixture's test with step `index` replaced by `step`.
fn with_step(index: usize, step: Value) -> Value {
    let mut test = acceptance_test("t");
    test["steps"][index] = step;
    test
}

#[test]
fn a_test_parses_with_its_defaults() {
    let test: AcceptanceTest = serde_json::from_value(acceptance_test("t")).expect("parses");
    assert_eq!(test.validate(), Ok(()));
    assert!(!test.stick);
    assert_eq!(
        test.steps[1],
        Step::Press {
            control: "usb".to_owned(),
            ms: 300
        }
    );
    assert_eq!(
        test.steps[2],
        Step::Touch {
            x: 40,
            y: 420,
            ms: 300
        }
    );
    let Step::Expect(check) = &test.steps[5] else {
        panic!("an expect step");
    };
    assert_eq!(check.regions().len(), 3);
    assert_eq!(test.steps[4], {
        let check = Check::Memory {
            address: "0x0c000014".to_owned(),
            bytes: 2,
            equals: 32,
        };
        assert_eq!(check.address(), Some(0x0c00_0014));
        Step::Expect(check)
    });
}

#[test]
fn unknown_steps_checks_and_fields_are_refused() {
    for step in [
        json!({ "turn": { "control": "jog" } }),
        json!({ "wait": { "ms": 1, "s": 1 } }),
        json!({ "expect": { "pixels": {} } }),
        json!({ "wait": { "ms": 1 }, "press": { "control": "usb" } }),
    ] {
        let test = with_step(3, step.clone());
        assert!(
            serde_json::from_value::<AcceptanceTest>(test).is_err(),
            "{step}"
        );
    }
}

#[test]
fn steps_are_bounded_and_a_test_checks_something() {
    let message = refused(with_step(3, json!({ "wait": { "ms": 0 } })));
    assert!(message.contains("wait: 1 ms to 10 minutes"), "{message}");
    let message = refused(with_step(3, json!({ "wait": { "ms": 600_001 } })));
    assert!(message.contains("wait: 1 ms to 10 minutes"), "{message}");
    let message = refused(with_step(3, json!({ "wait": { "ms": 0 } })));
    assert!(message.contains("wait: 1 ms to 10 minutes"), "{message}");
    let message = refused(with_step(
        1,
        json!({ "press": { "control": "usb", "ms": 0 } }),
    ));
    assert!(message.contains("press: held 1 ms to 10 s"), "{message}");
    let message = refused(with_step(1, json!({ "press": { "control": "USB" } })));
    assert!(message.contains("is not a name"), "{message}");
    let message = refused(with_step(
        2,
        json!({ "touch": { "x": 1, "y": 1, "ms": 10_001 } }),
    ));
    assert!(message.contains("touch: held 1 ms to 10 s"), "{message}");

    let mut test = acceptance_test("t");
    test["steps"][0]["wait_until"]["timeout_ms"] = json!(0);
    assert!(refused(test).contains("a timeout of 1 ms to 10 minutes"));

    let mut test = acceptance_test("t");
    test["steps"] = json!([{ "wait": { "ms": 1 } }]);
    assert!(refused(test).contains("at least one step checks something"));
    let mut test = acceptance_test("t");
    test["steps"] = json!([]);
    assert!(matches!(validated(test), Err(EntryError::Empty { .. })));

    // Seven waits of ten minutes: more than an hour in all.
    let mut test = acceptance_test("t");
    let steps = test["steps"].as_array_mut().expect("steps");
    steps.extend(std::iter::repeat_n(json!({ "wait": { "ms": 600_000 } }), 6));
    assert!(refused(test).contains("waits, holds and timeouts of at most one hour"));

    // Holds count too: 400 presses of ten seconds.
    let mut test = acceptance_test("t");
    let press = json!({ "press": { "control": "usb", "ms": 10_000 } });
    let steps = test["steps"].as_array_mut().expect("steps");
    steps.extend(std::iter::repeat_n(press, 400));
    assert!(refused(test).contains("waits, holds and timeouts of at most one hour"));
    let mut test = acceptance_test("t");
    let touch = json!({ "touch": { "x": 1, "y": 1, "ms": 10_000 } });
    let steps = test["steps"].as_array_mut().expect("steps");
    steps.extend(std::iter::repeat_n(touch, 400));
    assert!(refused(test).contains("waits, holds and timeouts of at most one hour"));

    // At most 64 memory checks, counting polls.
    let mut test = acceptance_test("t");
    let check = test["steps"][4].clone();
    let steps = test["steps"].as_array_mut().expect("steps");
    steps.extend(std::iter::repeat_n(check, 63));
    assert!(refused(test).contains("at most 64 memory checks"));
    let mut test = acceptance_test("t");
    let check = test["steps"][4].clone();
    let steps = test["steps"].as_array_mut().expect("steps");
    steps.extend(std::iter::repeat_n(check, 62));
    assert_eq!(validated(test), Ok(()));

    // Timeouts count as waits: seven polls of up to ten minutes.
    let mut test = acceptance_test("t");
    test["steps"][0]["wait_until"]["timeout_ms"] = json!(600_000);
    let poll = test["steps"][0].clone();
    let steps = test["steps"].as_array_mut().expect("steps");
    steps.extend(std::iter::repeat_n(poll, 6));
    assert!(refused(test).contains("waits, holds and timeouts of at most one hour"));

    let mut test = acceptance_test("t");
    let wait = json!({ "wait": { "ms": 1 } });
    let steps = test["steps"].as_array_mut().expect("steps");
    steps.extend(std::iter::repeat_n(wait, 1000));
    assert!(refused(test).contains("at most 1000 steps"));
}

#[test]
fn memory_checks_name_an_aligned_address_and_a_value_that_fits() {
    let memory = |address: &str, bytes: u8, equals: u32| {
        with_step(
            4,
            json!({ "expect": { "memory": {
                "address": address, "bytes": bytes, "equals": equals } } }),
        )
    };
    assert_eq!(validated(memory("0x0", 1, 255)), Ok(()));
    assert_eq!(validated(memory("0xffffffff", 1, 0)), Ok(()));
    assert_eq!(validated(memory("0x0c000014", 4, u32::MAX)), Ok(()));
    for address in [
        "0c000014",
        "0x",
        "0x123456789",
        "0x000000010",
        "0xg0",
        "0X10",
        "+0x10",
    ] {
        let message = refused(memory(address, 4, 0));
        assert!(
            message.contains("address: 0x and one to eight"),
            "{address}: {message}"
        );
    }
    assert!(refused(memory("0x0c000014", 3, 0)).contains("bytes: 1, 2 or 4"));
    assert!(refused(memory("0x0c000016", 4, 0)).contains("address: aligned to bytes"));
    assert!(refused(memory("0x0c000015", 2, 0)).contains("address: aligned to bytes"));
    assert!(refused(memory("0x0c000014", 1, 256)).contains("equals: a value that fits"));
    assert!(refused(memory("0x0c000014", 2, 65_536)).contains("equals: a value that fits"));
    assert_eq!(parse_address("0xABCdef01"), Some(0xabcd_ef01));
}

#[test]
fn brightness_checks_compare_with_one_to_sixteen_regions_by_a_margin() {
    let region = json!({ "x": 0, "y": 0, "w": 1, "h": 1 });
    let brightest = |others: Value, margin: u8| {
        with_step(
            5,
            json!({ "expect": { "brightest": {
                "region": region.clone(), "others": others, "margin": margin } } }),
        )
    };
    assert_eq!(validated(brightest(json!([region.clone()]), 1)), Ok(()));
    assert!(refused(brightest(json!([]), 1)).contains("others: one to 16 regions"));
    let many = Value::Array(vec![region.clone(); 17]);
    assert!(refused(brightest(many, 1)).contains("others: one to 16 regions"));
    assert!(refused(brightest(json!([region.clone()]), 0)).contains("margin: at least 1"));

    let fits = |x, y, w, h| Region { x, y, w, h }.fits(800, 480);
    assert!(fits(0, 0, 800, 480));
    assert!(fits(799, 479, 1, 1));
    assert!(!fits(0, 0, 0, 1));
    assert!(!fits(0, 0, 801, 1));
    assert!(!fits(1, 0, 800, 1));
    assert!(!fits(0, 480, 1, 1));
    assert!(!fits(u32::MAX, 0, 2, 1));
}

#[test]
fn a_test_must_fit_its_player() {
    let refused = |change: fn(&mut Value)| {
        let mut fixture = Fixture::new();
        change(&mut fixture.acceptance[0]);
        fixture.problem()
    };

    assert_eq!(
        refused(|t| t["player"] = json!("q-1.0")),
        "acceptance test pads: player q-1.0 is not in the catalog"
    );
    assert_eq!(
        refused(|t| t["steps"][1]["press"]["control"] = json!("play")),
        "acceptance test pads: control play of player p-1.0 is not in the catalog"
    );
    assert_eq!(
        refused(|t| t["steps"][2]["touch"]["x"] = json!(800)),
        "acceptance test pads: steps[2]: the touch is not on p-1.0's 800x480 screen"
    );
    let message = refused(|t| t["steps"][2]["touch"]["y"] = json!(480));
    assert!(message.contains("the touch is not on"), "{message}");
    let message = refused(|t| t["steps"][5]["expect"]["brightest"]["others"][1]["x"] = json!(721));
    assert_eq!(
        message,
        "acceptance test pads: steps[5]: a region is not on p-1.0's 800x480 screen"
    );

    // Memory checks read only the player's run-time state ranges.
    let outside = "acceptance test pads: steps[4]: the memory check reads outside p-1.0's \
                   run-time state ranges (state_memory); a test never reads the loaded \
                   application";
    assert_eq!(
        refused(|t| t["steps"][4]["expect"]["memory"]["address"] = json!("0x0c001000")),
        outside
    );
    // The last two bytes of the range fit; four bytes from there would not.
    let mut fixture = Fixture::new();
    fixture.acceptance[0]["steps"][4]["expect"]["memory"]["address"] = json!("0x0c000ffe");
    assert_eq!(fixture.check(), Ok(()));
    fixture.acceptance[0]["steps"][4]["expect"]["memory"]["bytes"] = json!(4);
    fixture.acceptance[0]["steps"][4]["expect"]["memory"]["address"] = json!("0x0c000ffc");
    assert_eq!(fixture.check(), Ok(()));
    fixture.acceptance[0]["steps"][4]["expect"]["memory"]["address"] = json!("0x0bfffffc");
    assert!(fixture.problem().contains("reads outside"));
    let message =
        refused(|t| t["steps"][0]["wait_until"]["check"]["memory"]["address"] = json!("0x0"));
    assert!(message.starts_with("acceptance test pads: steps[0]: the memory check reads outside"));
    // A player with no ranges allows no memory check.
    let mut fixture = Fixture::new();
    fixture.player["state_memory"] = json!([]);
    assert!(
        fixture
            .problem()
            .contains("steps[0]: the memory check reads outside")
    );

    let mut fixture = Fixture::new();
    fixture.acceptance.push(acceptance_test("pads"));
    assert_eq!(fixture.problem(), "acceptance test pads is defined twice");

    let mut fixture = Fixture::new();
    fixture.acceptance[0]["steps"][0]["wait_until"]["timeout_ms"] = json!(0);
    let message = fixture.problem();
    assert!(
        message.starts_with("acceptance test pads: steps[0]"),
        "{message}"
    );
}

#[test]
fn an_implementation_names_tests_for_its_own_player() {
    let mut fixture = Fixture::new();
    fixture.features[0]["implementations"]["p-1.0"][0]["acceptance"] = json!(["missing"]);
    assert_eq!(
        fixture.problem(),
        "feature plain: acceptance test missing is not in the catalog"
    );

    let mut fixture = Fixture::new();
    fixture.skins[0]["implementations"]["p-1.0"][0]["acceptance"] = json!(["missing"]);
    assert_eq!(
        fixture.problem(),
        "skin dark-pads: acceptance test missing is not in the catalog"
    );

    // The test moved to another player, with its own screen.
    let fixture = Fixture::new();
    let mut catalog = fixture.catalog();
    let mut other = fixture.player.clone();
    other["id"] = json!("q-1.0");
    other["screens"] = json!(["main"]);
    catalog
        .players
        .push(serde_json::from_value(other).expect("player"));
    let mut screen = catalog.screens[0].clone();
    screen.player = "q-1.0".to_owned();
    catalog.screens.push(screen);
    catalog.acceptance[0].player = "q-1.0".to_owned();
    let message = catalog
        .check(&fixture.parsed_recipes())
        .expect_err("refused")
        .to_string();
    assert_eq!(
        message,
        "feature plain: its implementation for p-1.0 names acceptance test pads, which is for \
         q-1.0"
    );

    let mut fixture = Fixture::new();
    let tests = json!(["pads", "pads"]);
    fixture.features[0]["implementations"]["p-1.0"][0]["acceptance"] = tests;
    assert!(fixture.problem().contains("lists \"pads\" twice"));
    let mut fixture = Fixture::new();
    fixture.features[0]["implementations"]["p-1.0"][0]["acceptance"] = json!(["Pads"]);
    assert!(fixture.problem().contains("is not an id"));
}
