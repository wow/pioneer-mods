// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

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
