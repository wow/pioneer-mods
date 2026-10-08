//! `patch-cli patch` with schema-v2 recipes: refusal paths that need no firmware. A successful
//! application needs the official file; see `official_rebuild_command.rs`.

mod common;

use common::write_bytes;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

const VERSION_MARKER: &str = "../../recipes/xdj700-v1.15/version-marker-0.10.json";

fn committed_recipe() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(VERSION_MARKER);
    serde_json::from_slice(&std::fs::read(path).expect("read recipe")).expect("recipe JSON")
}

fn run_patch(input: &Path, recipe: &Value, output: &Path, extra: &[&str]) -> Output {
    let dir = output.parent().expect("output directory");
    let recipe_path = dir.join("recipe.json");
    write_bytes(
        &recipe_path,
        &serde_json::to_vec(recipe).expect("serialize"),
    );
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("patch")
        .arg("--input")
        .arg(input)
        .arg("--recipe")
        .arg(&recipe_path)
        .arg("--output")
        .arg(output)
        .args(extra)
        .output()
        .expect("run patch-cli patch")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn paths() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("XDJ700.UPD");
    let output = dir.path().join("out.UPD");
    (dir, input, output)
}

fn assert_refused(result: &Output, output: &Path, message: &str) {
    assert!(!result.status.success());
    assert!(stderr(result).contains(message), "{}", stderr(result));
    assert!(!output.exists(), "no output may be written");
}

#[test]
fn refuses_an_invalid_recipe_before_reading_the_input() {
    let (_dir, missing_input, output) = paths();
    let mut recipe = committed_recipe();
    recipe["label"] = json!("1.16");

    let result = run_patch(&missing_input, &recipe, &output, &[]);

    assert_refused(
        &result,
        &output,
        "label \"1.16\" is not of the form VerX.YY",
    );
}

#[test]
fn refuses_force_with_a_v2_recipe() {
    let (_dir, missing_input, output) = paths();

    let result = run_patch(&missing_input, &committed_recipe(), &output, &["--force"]);

    assert_refused(
        &result,
        &output,
        "--force is not accepted with a schema-v2 recipe",
    );
}

#[test]
fn refuses_an_unknown_release() {
    let (_dir, missing_input, output) = paths();
    let mut recipe = committed_recipe();
    recipe["target"]["release"] = json!("xdj700-v9.99");

    let result = run_patch(&missing_input, &recipe, &output, &[]);

    assert_refused(&result, &output, "unknown release \"xdj700-v9.99\"");
}

#[test]
fn refuses_an_input_of_the_wrong_length_without_reading_it() {
    let (_dir, input, output) = paths();
    write_bytes(&input, b"not the official update");

    let result = run_patch(&input, &committed_recipe(), &output, &[]);

    assert_refused(
        &result,
        &output,
        "it is not the official update of release xdj700-v1.15 (23 bytes, expected 17371335)",
    );
}

#[test]
fn refuses_a_label_the_updater_would_skip_before_reading_the_input() {
    let (_dir, missing_input, output) = paths();
    let mut recipe = committed_recipe();
    recipe["label"] = json!("Ver1.15");

    let result = run_patch(&missing_input, &recipe, &output, &[]);

    assert_refused(
        &result,
        &output,
        "label Ver1.15 is not higher than the release's own version 1.15",
    );
}

#[test]
fn refuses_a_replacement_in_the_protected_header_before_reading_the_input() {
    let (_dir, missing_input, output) = paths();
    let mut recipe = committed_recipe();
    recipe["replacements"] = json!([{
        "offset": 0x740,
        "original_sha256": "00".repeat(32),
        "bytes_hex": "312e3939",
        "purpose": "an attempt to set the version without reported_version"
    }]);

    let result = run_patch(&missing_input, &recipe, &output, &[]);

    assert_refused(&result, &output, "overlaps protected range 0..2048");
}
