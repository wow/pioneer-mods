//! Helpers for the protected-set CLI tests: a temporary directory without an input, so a command
//! that gets past the set stops at the input, and runs of `patch` and `precondition` with the
//! protected-set variable controlled.

use super::{recipes_dir, write_bytes};
use patch_cli::recipe::PROTECTED_SET_ENV;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The committed beat-loop recipe: its precondition window is run-time `0x080D6234..=0x080D66F4`.
pub fn beat_loop() -> PathBuf {
    recipes_dir().join("xdj700-v1.15/beat-loop-16-plays-32.json")
}

pub const OVERLAPPING: &str = "release xdj700-v1.15\nstart end bytes\n080d6300 080d63ff 256\n";
pub const CLEAR: &str = "release xdj700-v1.15\n08000800 080d6233\n080d66f5 080d7000\n";

pub struct Run {
    _dir: tempfile::TempDir,
    pub input: PathBuf,
    pub output: PathBuf,
    pub set: PathBuf,
}

/// A temporary directory with no input (so a command that gets past the set stops at the input),
/// an output path, and `set` written as the protected set.
pub fn setup(set: &[u8]) -> Run {
    let dir = tempfile::tempdir().expect("tempdir");
    let run = Run {
        input: dir.path().join("XDJ700.UPD"),
        output: dir.path().join("out.UPD"),
        set: dir.path().join("set.tsv"),
        _dir: dir,
    };
    write_bytes(&run.set, set);
    run
}

/// `patch` with `extra` arguments; the protected-set variable is `env`, or unset.
pub fn patch_with(
    run: &Run,
    recipe: &Path,
    extra: &[&std::ffi::OsStr],
    env: Option<&Path>,
) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    command
        .arg("patch")
        .arg("--input")
        .arg(&run.input)
        .arg("--recipe")
        .arg(recipe)
        .arg("--output")
        .arg(&run.output)
        .args(extra)
        .env_remove(PROTECTED_SET_ENV);
    if let Some(path) = env {
        command.env(PROTECTED_SET_ENV, path);
    }
    command.output().expect("run patch-cli patch")
}

/// `patch --protected-set <set>`.
pub fn patch(run: &Run, recipe: &Path, set: &Path) -> Output {
    patch_with(
        run,
        recipe,
        &["--protected-set".as_ref(), set.as_os_str()],
        None,
    )
}

pub fn precondition_with(run: &Run, recipe: &Path, extra: &[&str], env: Option<&Path>) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    command
        .arg("precondition")
        .arg("--input")
        .arg(&run.input)
        .arg("--recipe")
        .arg(recipe)
        .arg("--committed-recipes")
        .arg(recipes_dir())
        .args(extra)
        .env_remove(PROTECTED_SET_ENV);
    if let Some(path) = env {
        command.env(PROTECTED_SET_ENV, path);
    }
    command.output().expect("run patch-cli precondition")
}

pub fn precondition(run: &Run, recipe: &Path) -> Output {
    let set = run.set.to_str().expect("UTF-8 path");
    precondition_with(run, recipe, &["--protected-set", set], None)
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Refused with `message`, nothing printed and no output written.
pub fn assert_refused(result: &Output, run: &Run, message: &str) {
    assert!(!result.status.success());
    assert!(stderr(result).contains(message), "{}", stderr(result));
    assert!(result.stdout.is_empty(), "nothing may be printed");
    assert!(!run.output.exists(), "no output may be written");
}
