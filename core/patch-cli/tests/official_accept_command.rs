//! Owner-input test of `patch-cli accept`: the official XDJ-700 v1.15 update in the owner's local
//! emulator. A catalog copy names erased flash above the update's records as a state range (only
//! here: erased flash holds no vendor bytes, and reads as all ones), so the memory path is
//! checked against a known value; a frame's regions are measured. Nothing but the PASS and FAIL
//! lines is printed. To run locally (about two minutes):
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD XDJ700_QEMU=/path/to/qemu-system-sh4 \
//!     cargo test -p patch-cli --test official_accept_command -- --ignored
//! ```
#![cfg(unix)]

mod common;
#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use common::{copy_catalog, write_bytes};
use official_pins::UPD_ENV;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::Command;

fn env_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {name}; see the module docs"))
}

fn write_json(path: &Path, value: &Value) {
    write_bytes(path, &serde_json::to_vec_pretty(value).expect("json"));
}

#[test]
#[ignore = "needs owner-supplied firmware and the owner's emulator; see module docs"]
fn the_runner_reads_memory_and_measures_frames_in_the_emulator() {
    let (update, emulator) = (env_path(UPD_ENV), env_path("XDJ700_QEMU"));
    let root = copy_catalog();
    let player_path = root.path().join("catalog/players/xdj700-v1.15.json");
    let mut player: Value =
        serde_json::from_slice(&std::fs::read(&player_path).expect("read")).expect("player");
    player["state_memory"] = json!([{ "start": "0x00f00000", "end": "0x01000000",
        "evidence": "Erased flash above the update's records (this test only)." }]);
    write_json(&player_path, &player);
    let memory = |address: &str, bytes: u8, equals: u32| {
        let check = json!({ "address": address, "bytes": bytes, "equals": equals });
        json!({ "memory": check })
    };
    let half = |x: u32, y: u32| json!({ "x": x, "y": y, "w": 400, "h": 240 });
    let test = json!({
        "schema_version": 1, "id": "runner", "player": "xdj700-v1.15", "description": "d",
        "steps": [
            { "wait_until": { "check": memory("0x00f00000", 4, 0xffff_ffff),
                              "timeout_ms": 30000 } },
            { "expect": memory("0x00f00010", 2, 0xffff) },
            { "expect": memory("0x00f00021", 1, 0) },
            { "wait": { "ms": 60000 } },
            { "expect": { "brightest": { "region": half(0, 0), "others": [half(400, 240)],
                                         "margin": 1 } } },
            { "expect": { "brightest": { "region": half(400, 240), "others": [half(0, 0)],
                                         "margin": 1 } } }
        ],
        "maintainer": "m"
    });
    let dir = root.path().join("catalog/acceptance");
    std::fs::create_dir_all(&dir).expect("dir");
    write_json(&dir.join("runner.json"), &test);

    let output = Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .args(["accept", "--test", "runner", "--update"])
        .arg(&update)
        .arg("--emulator")
        .arg(&emulator)
        .arg("--root")
        .arg(root.path())
        .output()
        .expect("run patch-cli accept");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    // The erased byte read as 0 fails; two regions cannot each be brighter than the other, so at
    // most one of the last two passes (both fail on a uniform screen).
    assert!(!output.status.success());
    assert!(
        stderr.contains("acceptance test runner failed: "),
        "{stderr}"
    );
    for line in [
        "steps[0] wait_until memory 0x00f00000 (4 bytes) == 4294967295: PASS (read 4294967295",
        "steps[1] expect memory 0x00f00010 (2 bytes) == 65535: PASS (read 65535)",
        "steps[2] expect memory 0x00f00021 (1 byte) == 0: FAIL (read 255)",
    ] {
        assert!(stdout.contains(line), "{line}\n{stdout}");
    }
    let brightness: Vec<&str> = stdout.lines().filter(|l| l.contains("brightest")).collect();
    assert_eq!(brightness.len(), 2, "{stdout}");
    assert!(
        brightness.iter().filter(|l| l.contains(": PASS (")).count() <= 1,
        "{stdout}"
    );
}
