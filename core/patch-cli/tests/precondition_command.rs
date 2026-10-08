//! `patch-cli precondition`: refusal paths that need no firmware. Hashing needs the official file;
//! see `official_rebuild_command.rs`.

mod common;

use common::write_bytes;
use serde_json::{Value, json};
use std::path::Path;
use std::process::{Command, Output};

const VERSION_MARKER: &str = "../../recipes/xdj700-v1.15/version-marker-0.10.json";

fn committed_recipe() -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(VERSION_MARKER);
    serde_json::from_slice(&std::fs::read(path).expect("read recipe")).expect("recipe JSON")
}

fn run_precondition(input: &Path, recipe: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("precondition")
        .arg("--input")
        .arg(input)
        .arg("--recipe")
        .arg(recipe)
        .output()
        .expect("run patch-cli precondition")
}

/// Runs `precondition` on `recipe` and an input at `input` (missing unless the test writes it),
/// and checks that it was refused with `message` and printed nothing.
fn assert_refused(recipe: &Value, input: Option<&[u8]>, message: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (input_path, recipe_path) = (dir.path().join("XDJ700.UPD"), dir.path().join("r.json"));
    if let Some(bytes) = input {
        write_bytes(&input_path, bytes);
    }
    write_bytes(
        &recipe_path,
        &serde_json::to_vec(recipe).expect("serialize"),
    );

    let result = run_precondition(&input_path, &recipe_path);

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(stderr.contains(message), "{stderr}");
    assert!(result.stdout.is_empty(), "nothing may be printed");
}

/// A replacement at `offset` with a placeholder hash.
fn draft_replacement(offset: u64, before: u64, after: u64) -> Value {
    json!({
        "offset": offset,
        "bytes_hex": "0102",
        "precondition": {"before": before, "after": after, "sha256": "00".repeat(32)},
        "purpose": "a draft"
    })
}

#[test]
fn refuses_an_invalid_recipe_before_reading_the_input() {
    let mut recipe = committed_recipe();
    recipe["label"] = json!("1.16");

    assert_refused(&recipe, None, "label \"1.16\" is not of the form VerX.YY");
}

#[test]
fn refuses_an_unknown_release_before_reading_the_input() {
    let mut recipe = committed_recipe();
    recipe["target"]["release"] = json!("xdj700-v9.99");

    assert_refused(&recipe, None, "unknown release \"xdj700-v9.99\"");
}

#[test]
fn refuses_windows_outside_the_application_or_in_the_header_before_reading_the_input() {
    let mut recipe = committed_recipe();
    recipe["replacements"] = json!([draft_replacement(18_601_862, 31, 1)]);
    assert_refused(
        &recipe,
        None,
        "replacements[0].precondition ends at 0x11bd789, past the application's end at 0x11bd788",
    );

    recipe["replacements"] = json!([draft_replacement(0x810, 32, 0)]);
    assert_refused(&recipe, None, "overlaps protected range 0x0..0x800");
}

#[test]
fn refuses_a_schema_v1_manifest() {
    let mut recipe = committed_recipe();
    recipe["schema_version"] = json!(1);

    assert_refused(
        &recipe,
        None,
        "schema_version 1 has no preconditions to hash; only schema_version 2 recipes do",
    );
}

#[test]
fn refuses_an_input_of_the_wrong_length_without_reading_it() {
    assert_refused(
        &committed_recipe(),
        Some(b"not the official update"),
        "refusing to hash preconditions on",
    );
    assert_refused(
        &committed_recipe(),
        Some(b"not the official update"),
        "it is not the official update of release xdj700-v1.15 (23 bytes, expected 17371335)",
    );
}

#[test]
fn refuses_an_input_of_the_right_length_that_is_not_the_official_update() {
    let dir = tempfile::tempdir().expect("tempdir");
    let (input, recipe) = (dir.path().join("XDJ700.UPD"), dir.path().join("r.json"));
    // Zeros, as long as the official update.
    let file = std::fs::File::create(&input).expect("create");
    file.set_len(17_371_335).expect("grow");
    write_bytes(
        &recipe,
        &serde_json::to_vec(&committed_recipe()).expect("serialize"),
    );

    let result = run_precondition(&input, &recipe);

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(
        stderr.contains("it is not the official update of release xdj700-v1.15 (SHA-256 ")
            && stderr.contains("); only that exact file is accepted"),
        "{stderr}"
    );
    assert!(result.stdout.is_empty());
}

#[test]
fn refuses_a_recipe_that_is_not_a_regular_file() {
    let dir = tempfile::tempdir().expect("tempdir");

    let result = run_precondition(&dir.path().join("XDJ700.UPD"), dir.path());

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("failed to read recipe manifest"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
}
