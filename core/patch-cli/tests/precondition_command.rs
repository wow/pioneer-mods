//! `patch-cli precondition`: refusal paths that need no firmware. Hashing needs the official file;
//! see `official_rebuild_command.rs`.

mod common;

use common::{committed_recipe, recipes_dir, write_bytes};
use serde_json::{Value, json};
use std::path::Path;
use std::process::{Command, Output};

fn run_precondition(input: &Path, recipe: &Path, committed: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("precondition")
        .arg("--input")
        .arg(input)
        .arg("--recipe")
        .arg(recipe)
        .arg("--committed-recipes")
        .arg(committed)
        // These tests are about other refusals: skip the protected set explicitly.
        .arg("--no-protected-set")
        .env_remove("XDJ700_PROTECTED_SET")
        .output()
        .expect("run patch-cli precondition")
}

/// Runs `precondition` on `recipe`, an input with `input` bytes (missing if `None`) and the
/// committed recipes in `committed`, and checks that it was refused with `message` and printed
/// nothing.
fn assert_refused_with(recipe: &Value, input: Option<&[u8]>, committed: &Path, message: &str) {
    let dir = tempfile::tempdir().expect("tempdir");
    let (input_path, recipe_path) = (dir.path().join("XDJ700.UPD"), dir.path().join("r.json"));
    if let Some(bytes) = input {
        write_bytes(&input_path, bytes);
    }
    write_bytes(
        &recipe_path,
        &serde_json::to_vec(recipe).expect("serialize"),
    );

    let result = run_precondition(&input_path, &recipe_path, committed);

    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(!result.status.success());
    assert!(stderr.contains(message), "{stderr}");
    assert!(result.stdout.is_empty(), "nothing may be printed");
}

/// [`assert_refused_with`] the repository's committed recipes.
fn assert_refused(recipe: &Value, input: Option<&[u8]>, message: &str) {
    assert_refused_with(recipe, input, &recipes_dir(), message);
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

/// A committed recipe directory holding one recipe, `published`, with a window `0x8e0..0x902`.
fn published_dir() -> tempfile::TempDir {
    let committed = tempfile::tempdir().expect("tempdir");
    let mut published = committed_recipe();
    published["recipe_id"] = json!("published");
    published["replacements"] = json!([draft_replacement(0x900, 32, 0)]);
    write_bytes(
        &committed.path().join("published.json"),
        &serde_json::to_vec(&published).expect("serialize"),
    );
    committed
}

#[test]
fn refuses_a_window_overlapping_a_committed_one_before_reading_the_input() {
    let committed = published_dir();
    let mut recipe = committed_recipe();
    recipe["recipe_id"] = json!("draft");

    // Shifted by one byte, and identical: both refused.
    recipe["replacements"] = json!([draft_replacement(0x901, 32, 0)]);
    assert_refused_with(
        &recipe,
        None,
        committed.path(),
        "published replacements[0] and draft replacements[0]: windows 0x8e0..0x902 and \
         0x8e1..0x903 overlap; windows of recipes for the same release must be disjoint",
    );
    recipe["replacements"] = json!([draft_replacement(0x900, 32, 0)]);
    assert_refused_with(
        &recipe,
        None,
        committed.path(),
        "draft replacements[0] and published replacements[0]: windows 0x8e0..0x902 and \
         0x8e0..0x902 overlap",
    );

    // Disjoint: the command goes on to the (missing) input.
    recipe["replacements"] = json!([draft_replacement(0x902, 0, 32)]);
    assert_refused_with(
        &recipe,
        None,
        committed.path(),
        "failed to read input update",
    );
}

#[test]
fn skips_the_drafts_own_file_among_the_committed_recipes() {
    let committed = published_dir();
    let mut recipe = committed_recipe();
    recipe["recipe_id"] = json!("draft");
    recipe["replacements"] = json!([draft_replacement(0x1000, 32, 0)]);
    write_bytes(
        &committed.path().join("draft.json"),
        &serde_json::to_vec(&recipe).expect("serialize"),
    );
    // The same file, spelled differently from the directory's listing of it.
    let draft = committed.path().join("sub/../draft.json");
    std::fs::create_dir(committed.path().join("sub")).expect("mkdir");
    let missing_input = committed.path().join("XDJ700.UPD");

    let result = run_precondition(&missing_input, &draft, committed.path());

    // Compared with `published` only, not with itself: the command reaches the input.
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(stderr.contains("failed to read input update"), "{stderr}");
    assert!(result.stdout.is_empty());
}

#[test]
fn refuses_a_committed_directory_without_another_recipe_for_the_release() {
    let empty = tempfile::tempdir().expect("tempdir");
    assert_refused_with(
        &committed_recipe(),
        None,
        empty.path(),
        "it holds no other recipe for release xdj700-v1.15; pass the repository's recipes \
         directory",
    );

    // A directory holding only a recipe for another release.
    let other = tempfile::tempdir().expect("tempdir");
    let mut elsewhere = committed_recipe();
    elsewhere["target"]["release"] = json!("another-release");
    write_bytes(
        &other.path().join("elsewhere.json"),
        &serde_json::to_vec(&elsewhere).expect("serialize"),
    );
    assert_refused_with(
        &committed_recipe(),
        None,
        other.path(),
        "it holds no other recipe for release xdj700-v1.15",
    );

    // A directory holding only the draft itself.
    let only_draft = tempfile::tempdir().expect("tempdir");
    let draft = only_draft.path().join("draft.json");
    write_bytes(
        &draft,
        &serde_json::to_vec(&committed_recipe()).expect("serialize"),
    );
    let result = run_precondition(
        &only_draft.path().join("XDJ700.UPD"),
        &draft,
        only_draft.path(),
    );
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("it holds no other recipe for release"),
        "{stderr}"
    );
    assert!(result.stdout.is_empty());
}

#[cfg(unix)]
#[test]
fn refuses_a_symbolic_link_among_the_committed_recipes() {
    let committed = published_dir();
    let elsewhere = tempfile::tempdir().expect("tempdir");
    let target = elsewhere.path().join("outside.json");
    write_bytes(&target, b"{}");
    std::os::unix::fs::symlink(&target, committed.path().join("link.json")).expect("symlink");

    assert_refused_with(
        &committed_recipe(),
        None,
        committed.path(),
        "input path does not point to a regular file",
    );
}

#[test]
fn refuses_schema_versions_without_preconditions() {
    let mut recipe = committed_recipe();
    recipe["schema_version"] = json!(1);
    assert_refused(
        &recipe,
        None,
        "schema_version 1 manifests have no preconditions to hash; only schema_version 2 \
         recipes do",
    );

    // The same message as `patch`.
    recipe["schema_version"] = json!(3);
    assert_refused(
        &recipe,
        None,
        "unsupported schema_version 3; supported: 1, 2",
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

    let result = run_precondition(&input, &recipe, &recipes_dir());

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

    let result = run_precondition(&dir.path().join("XDJ700.UPD"), dir.path(), &recipes_dir());

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("failed to read recipe manifest"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(result.stdout.is_empty());
}

#[test]
fn requires_the_committed_recipes() {
    let result = Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .args([
            "precondition",
            "--input",
            "XDJ700.UPD",
            "--recipe",
            "r.json",
        ])
        .output()
        .expect("run patch-cli precondition");

    assert!(!result.status.success());
    assert!(
        String::from_utf8_lossy(&result.stderr).contains("--committed-recipes"),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}
