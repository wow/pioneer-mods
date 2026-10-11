//! Running an acceptance test (`patch_cli::accept::run_test`) against a scripted emulator: how
//! each step reaches the emulator, and how each check passes or fails. Needs no firmware and no
//! emulator.

use anyhow::{Result, bail};
use patch_cli::accept::{Emulator, Frame, POLL_MS, run_test};
use patch_schema::catalog::{AcceptanceTest, Player};
use serde_json::{Value, json};
use std::collections::BTreeMap;

/// A scripted emulator: memory that changes after a set time, one frame, and a log of what it
/// was asked to do.
#[derive(Default)]
struct Scripted {
    /// Address -> (value before, value after, milliseconds after which it changes).
    memory: BTreeMap<u32, (u32, u32, u32)>,
    frame: Option<Frame>,
    elapsed: u32,
    log: Vec<String>,
    fail_reads: bool,
}

impl Emulator for Scripted {
    fn press(&mut self, code: u8, value: u8, ms: u32) -> Result<()> {
        self.log
            .push(format!("press {code:#04x}:{value:#04x} {ms}"));
        self.elapsed += ms;
        Ok(())
    }

    fn touch(&mut self, x: u32, y: u32, ms: u32) -> Result<()> {
        self.log.push(format!("touch {x},{y} {ms}"));
        self.elapsed += ms;
        Ok(())
    }

    fn wait(&mut self, ms: u32) -> Result<()> {
        self.log.push(format!("wait {ms}"));
        self.elapsed += ms;
        Ok(())
    }

    fn read(&mut self, address: u32, bytes: u8) -> Result<u32> {
        if self.fail_reads {
            bail!("the emulator's monitor closed");
        }
        self.log.push(format!("read {address:#x} {bytes}"));
        let (before, after, at) = self.memory.get(&address).copied().unwrap_or_default();
        Ok(if self.elapsed >= at { after } else { before })
    }

    fn frame(&mut self) -> Result<Frame> {
        self.log.push("frame".to_owned());
        Ok(self.frame.clone().expect("a frame"))
    }
}

fn player() -> Player {
    serde_json::from_value(json!({
        "schema_version": 1, "id": "p-1.0", "model": "P",
        "firmware": { "file": "P.UPD", "upd_sha256": "1".repeat(64),
                      "application_sha256": "2".repeat(64) },
        "screen_class": { "width": 8, "height": 4, "pixels": "rgb565" },
        "budgets": { "compressed_main_growth_bytes": 1 },
        "screens": ["main"],
        "controls": { "usb": { "code": "0x13", "value": "0x04", "description": "USB" } },
        "restore": { "official": "o", "backup": "b", "guide": "docs/p.md" },
        "maintainer": "m"
    }))
    .expect("player")
}

fn test(steps: Value) -> AcceptanceTest {
    serde_json::from_value(json!({
        "schema_version": 1, "id": "t", "player": "p-1.0", "description": "d",
        "steps": steps, "maintainer": "m"
    }))
    .expect("test")
}

/// An 8x4 frame: the left half at `left`, the right half at `right` (grey levels).
fn frame(left: u8, right: u8) -> Frame {
    let pixels = (0..32)
        .map(|i| {
            if i % 8 < 4 {
                (left, left, left)
            } else {
                (right, right, right)
            }
        })
        .collect();
    Frame {
        width: 8,
        height: 4,
        pixels,
    }
}

fn brightest(margin: u8) -> Value {
    json!({ "expect": { "brightest": {
        "region": { "x": 0, "y": 0, "w": 4, "h": 4 },
        "others": [{ "x": 4, "y": 0, "w": 4, "h": 4 }], "margin": margin } } })
}

#[test]
fn steps_reach_the_emulator_in_order() {
    let mut emulator = Scripted::default();
    emulator.memory.insert(0x100, (0, 7, 0));
    let steps = json!([
        { "wait": { "ms": 1000 } },
        { "press": { "control": "usb" } },
        { "touch": { "x": 3, "y": 2, "ms": 150 } },
        { "expect": { "memory": { "address": "0x100", "bytes": 2, "equals": 7 } } }
    ]);
    let outcome = run_test(&test(steps), &player(), &mut emulator).expect("runs");
    assert_eq!(
        emulator.log,
        [
            "wait 1000",
            "press 0x13:0x04 300",
            "touch 3,2 150",
            "read 0x100 2"
        ]
    );
    assert_eq!((outcome.checks, outcome.failed), (1, 0));
    assert_eq!(
        outcome.lines,
        ["steps[3] expect memory 0x00000100 (2 bytes) == 7: PASS (read 7)"]
    );
}

#[test]
fn a_failed_check_is_recorded_and_the_run_goes_on() {
    let mut emulator = Scripted::default();
    emulator.memory.insert(0x100, (3, 3, 0));
    let steps = json!([
        { "expect": { "memory": { "address": "0x100", "bytes": 1, "equals": 4 } } },
        { "wait": { "ms": 10 } },
        { "expect": { "memory": { "address": "0x100", "bytes": 1, "equals": 3 } } }
    ]);
    let outcome = run_test(&test(steps), &player(), &mut emulator).expect("runs");
    assert_eq!((outcome.checks, outcome.failed), (2, 1));
    assert_eq!(
        outcome.lines,
        [
            "steps[0] expect memory 0x00000100 (1 byte) == 4: FAIL (read 3)",
            "steps[2] expect memory 0x00000100 (1 byte) == 3: PASS (read 3)"
        ]
    );
}

#[test]
fn wait_until_polls_until_the_check_holds_or_the_timeout() {
    // The value changes 1200 ms in: read at 0, 500 and 1000 ms, then holds at 1500.
    let mut emulator = Scripted::default();
    emulator.memory.insert(0x100, (0, 1, 1200));
    let poll = json!({ "wait_until": { "check": { "memory": {
        "address": "0x100", "bytes": 4, "equals": 1 } }, "timeout_ms": 5000 } });
    let outcome = run_test(&test(json!([poll])), &player(), &mut emulator).expect("runs");
    assert_eq!(
        outcome.lines,
        [
            "steps[0] wait_until memory 0x00000100 (4 bytes) == 1: PASS (read 1, after 1500 ms \
             of 5000)"
        ]
    );
    assert_eq!(
        emulator
            .log
            .iter()
            .filter(|l| l.starts_with("read"))
            .count(),
        4
    );
    assert!(
        emulator
            .log
            .iter()
            .all(|l| !l.starts_with("wait") || *l == format!("wait {POLL_MS}"))
    );

    // Never holds: polls until the timeout, the last pause cut to what is left.
    let mut emulator = Scripted::default();
    let poll = json!({ "wait_until": { "check": { "memory": {
        "address": "0x100", "bytes": 4, "equals": 1 } }, "timeout_ms": 1200 } });
    let outcome = run_test(&test(json!([poll])), &player(), &mut emulator).expect("runs");
    assert_eq!(outcome.failed, 1);
    assert!(outcome.lines[0].ends_with("FAIL (read 0, after 1200 ms of 1200)"));
    let waits: Vec<&String> = emulator
        .log
        .iter()
        .filter(|l| l.starts_with("wait"))
        .collect();
    assert_eq!(waits, ["wait 500", "wait 500", "wait 200"]);
}

#[test]
fn brightest_compares_regions_of_one_frame_by_the_margin() {
    // Grey 100 is brightness 100; 90 is 90: ten apart.
    let run = |left: u8, right: u8, margin: u8| {
        let mut emulator = Scripted {
            frame: Some(frame(left, right)),
            ..Scripted::default()
        };
        run_test(&test(json!([brightest(margin)])), &player(), &mut emulator).expect("runs")
    };
    let outcome = run(100, 90, 10);
    assert_eq!(outcome.failed, 0);
    assert_eq!(
        outcome.lines,
        [
            "steps[0] expect brightest (0, 0, 4x4) over 1 regions by 10: PASS (brightness 100; \
             others 90)"
        ]
    );
    assert_eq!(run(100, 90, 11).failed, 1);
    assert_eq!(run(90, 100, 1).failed, 1);
    assert_eq!(run(255, 250, 5).failed, 0);
}

#[test]
fn a_frame_of_another_size_or_an_emulator_error_ends_the_run() {
    let mut emulator = Scripted {
        frame: Some(Frame {
            width: 4,
            height: 4,
            pixels: vec![(0, 0, 0); 16],
        }),
        ..Scripted::default()
    };
    let error =
        run_test(&test(json!([brightest(1)])), &player(), &mut emulator).expect_err("refused");
    assert_eq!(
        error.to_string(),
        "the emulator's frame is 4x4, not p-1.0's 8x4 screen"
    );

    let mut emulator = Scripted {
        fail_reads: true,
        ..Scripted::default()
    };
    let steps = json!([{ "expect": { "memory": { "address": "0x0", "bytes": 1, "equals": 0 } } }]);
    let error = run_test(&test(steps), &player(), &mut emulator).expect_err("refused");
    assert_eq!(error.to_string(), "the emulator's monitor closed");

    // A control the player lacks (the catalog check refuses it; the runner does too).
    let steps = json!([{ "press": { "control": "play" } }, brightest(1)]);
    let error = run_test(&test(steps), &player(), &mut Scripted::default()).expect_err("refused");
    assert_eq!(
        error.to_string(),
        "steps[0]: player p-1.0 has no control play"
    );
}
