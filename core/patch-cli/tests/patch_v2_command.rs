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
        "bytes_hex": "312e3939",
        "precondition": {"before": 16, "after": 16, "sha256": "00".repeat(32)},
        "purpose": "an attempt to set the version without reported_version"
    }]);

    let result = run_patch(&missing_input, &recipe, &output, &[]);

    assert_refused(&result, &output, "overlaps protected range 0x0..0x800");
}

#[test]
fn refuses_a_reported_version_not_lower_before_reading_the_input() {
    let (_dir, missing_input, output) = paths();
    let mut recipe = committed_recipe();
    recipe["reported_version"] = json!("1.15");

    let result = run_patch(&missing_input, &recipe, &output, &[]);

    assert_refused(
        &result,
        &output,
        "reported version 1.15 is not lower than the official 1.15",
    );
}

#[test]
fn never_overwrites_an_existing_output() {
    let (_dir, input, output) = paths();
    write_bytes(&input, b"input");
    write_bytes(&output, b"keep me");

    let result = run_patch(&input, &committed_recipe(), &output, &[]);

    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("choose a new output path"),
        "{}",
        stderr(&result)
    );
    assert_eq!(std::fs::read(&output).expect("read output"), b"keep me");
}

#[test]
fn refuses_to_write_over_the_input() {
    let (_dir, input, _output) = paths();
    write_bytes(&input, b"input");

    let result = run_patch(&input, &committed_recipe(), &input, &[]);

    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("same path as input"),
        "{}",
        stderr(&result)
    );
    assert_eq!(std::fs::read(&input).expect("read input"), b"input");
}

#[test]
fn refuses_an_unsupported_schema_version() {
    let (_dir, missing_input, output) = paths();
    let mut recipe = committed_recipe();
    recipe["schema_version"] = json!(3);

    let result = run_patch(&missing_input, &recipe, &output, &[]);

    assert_refused(
        &result,
        &output,
        "unsupported schema_version 3; supported: 1, 2",
    );
}

#[test]
fn refuses_a_recipe_that_is_not_a_small_regular_file() {
    let (dir, missing_input, output) = paths();
    let huge = dir.path().join("huge.json");
    let file = std::fs::File::create(&huge).expect("create");
    file.set_len(1024 * 1024 + 1).expect("grow");

    for (recipe, message) in [
        (huge.as_path(), "larger than any recipe"),
        (dir.path(), "failed to read recipe manifest"),
    ] {
        let result = Command::new(env!("CARGO_BIN_EXE_patch-cli"))
            .arg("patch")
            .arg("--input")
            .arg(&missing_input)
            .arg("--recipe")
            .arg(recipe)
            .arg("--output")
            .arg(&output)
            .output()
            .expect("run patch-cli patch");

        assert_refused(&result, &output, message);
    }
}

#[test]
fn refuses_a_window_past_the_application_before_reading_the_input() {
    let (_dir, missing_input, output) = paths();
    let mut recipe = committed_recipe();
    // The span ends at the application's end (18_601_864 bytes); its window one byte later.
    recipe["replacements"] = json!([{
        "offset": 18_601_862,
        "bytes_hex": "0102",
        "precondition": {"before": 30, "after": 1, "sha256": "00".repeat(32)},
        "purpose": "a window one byte past the application"
    }]);

    let result = run_patch(&missing_input, &recipe, &output, &[]);

    assert_refused(
        &result,
        &output,
        "replacements[0].precondition ends at 0x11bd789, past the application's end at 0x11bd788",
    );
}
