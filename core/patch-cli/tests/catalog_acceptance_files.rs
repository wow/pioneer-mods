//! Acceptance tests in the catalog loader (`catalog/acceptance/<id>.json`), on copies of the
//! committed catalog. Needs no firmware and no emulator.

mod common;

use common::{copy_catalog, write_bytes};
use patch_cli::catalog::load_catalog;
use serde_json::{Value, json};
use std::path::Path;

fn test_file(id: &str) -> Value {
    json!({
        "schema_version": 1, "id": id, "player": "xdj700-v1.15",
        "description": "The first BEAT LOOP pad selects its length", "stick": true,
        "steps": [
            { "wait": { "ms": 45000 } },
            { "press": { "control": "usb" } },
            { "press": { "control": "rotary_push" } },
            { "touch": { "x": 50, "y": 420, "ms": 150 } },
            { "expect": { "brightest": { "region": { "x": 10, "y": 400, "w": 80, "h": 53 },
                                         "others": [{ "x": 95, "y": 400, "w": 80, "h": 53 }],
                                         "margin": 16 } } }
        ],
        "maintainer": "wow"
    })
}

fn write_json(path: &Path, value: &Value) {
    std::fs::create_dir_all(path.parent().expect("parent")).expect("dir");
    write_bytes(path, &serde_json::to_vec_pretty(value).expect("json"));
}

fn refusal(root: &Path) -> String {
    format!("{:#}", load_catalog(root).expect_err("refused"))
}

/// Has the stage-7 feature's implementation name `tests`.
fn name_tests(root: &Path, tests: Value) {
    let path = root.join("catalog/features/beat-loop-1-to-32.json");
    let mut feature: Value =
        serde_json::from_slice(&std::fs::read(&path).expect("read")).expect("feature");
    feature["implementations"]["xdj700-v1.15"][0]["acceptance"] = tests;
    write_json(&path, &feature);
}

#[test]
fn a_test_loads_and_an_implementation_names_it() {
    let root = copy_catalog();
    let dir = root.path().join("catalog/acceptance");
    write_json(
        &dir.join("pads-select-lengths.json"),
        &test_file("pads-select-lengths"),
    );
    name_tests(root.path(), json!(["pads-select-lengths"]));

    let checked = load_catalog(root.path()).expect("loads");
    let catalog = checked.catalog();
    assert_eq!(catalog.acceptance.len(), 1);
    assert!(catalog.acceptance_test("pads-select-lengths").is_some());
    let feature = catalog.feature("beat-loop-1-to-32").expect("feature");
    assert_eq!(
        feature.implementations["xdj700-v1.15"][0].acceptance,
        ["pads-select-lengths"]
    );
    assert_eq!(catalog.players[0].controls.len(), 2);
}

#[test]
fn a_misnamed_unknown_or_unfit_test_is_refused() {
    let root = copy_catalog();
    let dir = root.path().join("catalog/acceptance");
    write_json(&dir.join("pads.json"), &test_file("pads-select-lengths"));
    assert!(refusal(root.path()).contains("must be named pads-select-lengths.json"));
    std::fs::remove_file(dir.join("pads.json")).expect("remove");

    name_tests(root.path(), json!(["missing"]));
    let message = refusal(root.path());
    assert!(
        message
            .contains("feature beat-loop-1-to-32: acceptance test missing is not in the catalog"),
        "{message}"
    );
    name_tests(root.path(), json!([]));

    let mut unfit = test_file("unfit");
    unfit["steps"][1]["press"]["control"] = json!("play");
    write_json(&dir.join("unfit.json"), &unfit);
    let message = refusal(root.path());
    assert!(
        message.contains("control play of player xdj700-v1.15 is not in the catalog"),
        "{message}"
    );
    std::fs::remove_file(dir.join("unfit.json")).expect("remove");

    // The XDJ-700 names no run-time state ranges yet, so no memory check is accepted.
    let mut memory = test_file("memory");
    memory["steps"][4] =
        json!({ "expect": { "memory": { "address": "0x0c000000", "bytes": 4, "equals": 1 } } });
    write_json(&dir.join("memory.json"), &memory);
    let message = refusal(root.path());
    assert!(
        message.contains("reads outside xdj700-v1.15's run-time state ranges"),
        "{message}"
    );
}
