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
    let mut command = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    command
        .arg("compose")
        .arg("--input")
        .arg(dir.join("missing.UPD"))
        .args(["--label", "Ver1.16", "--report-version", "0.12"])
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
    assert!(stderr(&result).contains("Ver1.15"), "{}", stderr(&result));
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
