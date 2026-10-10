// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

pub mod protected_set;

use serde_json::{Value, json};
use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The repository root, holding `catalog/` and `recipes/`.
pub fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A copy of the committed catalog, the recipes it names and the docs (its players' flashing
/// guides), under a temporary root.
pub fn copy_catalog() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    for dir in ["catalog", "recipes", "docs"] {
        copy_tree(&repo_root().join(dir), &root.path().join(dir));
    }
    root
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

/// A profile for the committed XDJ-700 v1.15 player choosing `features` under `Ver1.16` and
/// `0.12` (stage 7's own), accepting `maturity`; tests change other fields in place.
pub fn profile(features: &[&str], maturity: &str) -> Value {
    json!({
        "schema_version": 1, "player": "xdj700-v1.15", "features": features,
        "label": "Ver1.16", "reported_version": "0.12", "maturity": maturity
    })
}

/// Writes `profile` to `dir/profile.json` and runs `patch-cli <command> --profile <it> --root
/// <root>`, then `extra`, with no protected set named by the environment.
pub fn run_with_profile<I, S>(
    command: &str,
    profile: &Value,
    root: &Path,
    dir: &Path,
    extra: I,
) -> Output
where
    I: IntoIterator<Item = S>,
    S: AsRef<OsStr>,
{
    let path = dir.join("profile.json");
    write_bytes(&path, &serde_json::to_vec(profile).expect("json"));
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg(command)
        .arg("--profile")
        .arg(&path)
        .arg("--root")
        .arg(root)
        .args(extra)
        .env_remove("XDJ700_PROTECTED_SET")
        .output()
        .unwrap_or_else(|error| panic!("run patch-cli {command}: {error}"))
}

/// The repository's recipe directory.
pub fn recipes_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recipes")
}

/// The committed version-marker recipe's path.
pub fn version_marker_path() -> PathBuf {
    recipes_dir().join("xdj700-v1.15/version-marker-0.10.json")
}

/// The committed version-marker recipe, as JSON to modify.
pub fn committed_recipe() -> serde_json::Value {
    let raw = std::fs::read(version_marker_path()).expect("read recipe");
    serde_json::from_slice(&raw).expect("recipe JSON")
}

pub fn write_bytes(path: &Path, bytes: &[u8]) {
    let mut file = File::create(path).expect("create file");
    file.write_all(bytes).expect("write file");
    file.flush().expect("flush file");
}

pub fn run_patch_command_with_args_in_dir(
    input: &Path,
    recipe: &Path,
    output: &Path,
    extra_args: &[&str],
    current_dir: Option<&Path>,
) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    cmd.arg("patch")
        .arg("--input")
        .arg(input)
        .arg("--recipe")
        .arg(recipe)
        .arg("--output")
        .arg(output);
    if let Some(current_dir) = current_dir {
        cmd.current_dir(current_dir);
    }
    cmd.args(extra_args);
    cmd.output().expect("run patch-cli patch")
}
