//! `patch-cli compose`: refusals that need no firmware. Composing on the official file is in
//! `official_compose_command.rs`.

mod common;

use common::{committed_recipe, recipes_dir, write_bytes};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Runs `compose` with `recipes` and `extra`, writing to a fresh output path in `dir`. The input
/// path does not exist: each refusal here must come before the input is read.
fn run_compose(dir: &Path, recipes: &[PathBuf], extra: &[&str]) -> Output {
    run_compose_as(dir, recipes, ["Ver1.16", "0.12"], extra)
}

/// [`run_compose`] under `label` and `reported_version`.
fn run_compose_as(
    dir: &Path,
    recipes: &[PathBuf],
    [label, reported_version]: [&str; 2],
    extra: &[&str],
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    command
        .arg("compose")
        .arg("--input")
        .arg(dir.join("missing.UPD"))
        .args(["--label", label, "--report-version", reported_version])
        .arg("--output")
        .arg(dir.join("out.UPD"))
        .env_remove("XDJ700_PROTECTED_SET")
        .args(extra);
    for recipe in recipes {
        command.arg("--recipe").arg(recipe);
    }
    command.output().expect("run patch-cli compose")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

fn assert_refused(output: &Output, dir: &Path, message: &str) {
    assert!(!output.status.success());
    assert!(stderr(output).contains(message), "{}", stderr(output));
    assert!(!dir.join("out.UPD").exists());
}

fn committed(name: &str) -> PathBuf {
    recipes_dir().join("xdj700-v1.15").join(name)
}

/// The committed recipe `name`, changed by `change` and written to `file` in `dir`.
fn changed(
    dir: &Path,
    name: &str,
    file: &str,
    change: impl FnOnce(&mut serde_json::Value),
) -> PathBuf {
    let raw = std::fs::read(committed(name)).expect("read committed recipe");
    let mut recipe: serde_json::Value = serde_json::from_slice(&raw).expect("json");
    change(&mut recipe);
    let path = dir.join(file);
    write_bytes(&path, &serde_json::to_vec(&recipe).expect("json"));
    path
}

#[test]
fn needs_at_least_two_recipes() {
    let dir = tempfile::tempdir().expect("tempdir");

    let result = run_compose(
        dir.path(),
        &[committed("version-marker-0.10.json")],
        &["--no-protected-set"],
    );

    assert_refused(&result, dir.path(), "compose needs at least two recipes");
}

#[test]
fn refuses_an_invalid_recipe_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let mut invalid = committed_recipe();
    invalid["label"] = json!("Ver1.15");
    let invalid_path = dir.path().join("invalid.json");
    write_bytes(&invalid_path, &serde_json::to_vec(&invalid).expect("json"));

    let result = run_compose(
        dir.path(),
        &[committed("beat-loop-16-plays-32.json"), invalid_path],
        &["--no-protected-set"],
    );

    assert_refused(&result, dir.path(), "refusing to compose recipes");
    // The refusal names the recipe.
    assert!(
        stderr(&result).contains("invalid.json"),
        "{}",
        stderr(&result)
    );
    assert!(stderr(&result).contains("Ver1.15"), "{}", stderr(&result));
}

#[test]
fn refuses_overlapping_recipes_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    // Stage 5's replacement one byte further on: its window overlaps, but it is not a repeat.
    let shifted = changed(
        dir.path(),
        "beat-loop-16-plays-32.json",
        "shifted.json",
        |recipe| {
            recipe["recipe_id"] = json!("shifted");
            let offset = recipe["replacements"][0]["offset"]
                .as_u64()
                .expect("offset");
            recipe["replacements"][0]["offset"] = json!(offset + 1);
        },
    );

    let result = run_compose(
        dir.path(),
        &[committed("beat-loop-16-plays-32.json"), shifted],
        &["--no-protected-set"],
    );

    assert_refused(&result, dir.path(), "overlap");
    // Both files are named.
    assert!(
        stderr(&result).contains("shifted.json"),
        "{}",
        stderr(&result)
    );
    assert!(
        stderr(&result).contains("beat-loop-16-plays-32.json"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn refuses_the_same_recipe_twice_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let recipe = committed("beat-loop-16-plays-32.json");

    let result = run_compose(
        dir.path(),
        &[recipe.clone(), recipe],
        &["--no-protected-set"],
    );

    assert_refused(&result, dir.path(), "give each recipe once");
}

#[test]
fn refuses_an_unpinned_recipe_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let unpinned = changed(
        dir.path(),
        "version-marker-0.10.json",
        "unpinned.json",
        |recipe| {
            recipe.as_object_mut().expect("object").remove("expected");
        },
    );

    let result = run_compose(
        dir.path(),
        &[unpinned, committed("beat-loop-16-plays-32.json")],
        &["--no-protected-set"],
    );

    assert_refused(&result, dir.path(), "must pin expected.application_sha256");
    assert!(
        stderr(&result).contains("unpinned.json"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn refuses_an_invalid_label_or_version_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let recipes = [
        committed("version-marker-0.10.json"),
        committed("beat-loop-16-plays-32.json"),
    ];

    for ([label, version], message) in [
        (
            ["Ver1.15", "0.12"],
            "is not higher than the release's own version",
        ),
        (["Ver1.16", "1.15"], "is not lower than the official"),
    ] {
        let result = run_compose_as(
            dir.path(),
            &recipes,
            [label, version],
            &["--no-protected-set"],
        );

        assert_refused(&result, dir.path(), message);
    }
}

#[test]
fn refuses_a_schema_v1_manifest() {
    let dir = tempfile::tempdir().expect("tempdir");
    let v1_path = dir.path().join("v1.json");
    write_bytes(&v1_path, br#"{"schema_version": 1}"#);

    let result = run_compose(
        dir.path(),
        &[committed("version-marker-0.10.json"), v1_path],
        &["--no-protected-set"],
    );

    assert_refused(
        &result,
        dir.path(),
        "only schema-v2 recipes can be composed",
    );
}

#[test]
fn refuses_to_run_without_a_protected_set_or_an_explicit_skip() {
    let dir = tempfile::tempdir().expect("tempdir");

    let result = run_compose(
        dir.path(),
        &[
            committed("version-marker-0.10.json"),
            committed("beat-loop-16-plays-32.json"),
        ],
        &[],
    );

    assert_refused(&result, dir.path(), "no protected set");
}
