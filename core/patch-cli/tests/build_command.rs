//! `patch-cli build`: refusals that need no firmware. The input path does not exist, or holds a
//! short synthetic file, so each refusal here comes before an official input is read. Building on
//! the official file is in `official_build_command.rs`.

mod common;

use common::{copy_catalog, profile, repo_root, run_with_profile, write_bytes};
use serde_json::{Value, json};
use std::ffi::OsString;
use std::path::Path;
use std::process::Output;

/// Runs `build` for `profile` against the catalog under `root`, with `extra` arguments, on
/// `input` (a missing file when `None`); returns the output and whether the output file was
/// written.
fn run(profile: &Value, root: &Path, extra: &[&str], input: Option<&[u8]>) -> (Output, bool) {
    let dir = tempfile::tempdir().expect("tempdir");
    let input_path = dir.path().join("missing.UPD");
    if let Some(bytes) = input {
        write_bytes(&input_path, bytes);
    }
    let output_path = dir.path().join("out.UPD");
    let mut args: Vec<OsString> = vec![
        "--input".into(),
        input_path.into(),
        "--output".into(),
        output_path.clone().into(),
    ];
    args.extend(extra.iter().map(OsString::from));
    let output = run_with_profile("build", profile, root, dir.path(), args);
    (output, output_path.exists())
}

fn refused(profile: &Value, root: &Path, extra: &[&str]) -> String {
    let (output, written) = run(profile, root, extra, None);
    assert!(!output.status.success());
    assert!(!written);
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_profile_with_nothing_on_is_refused() {
    // The committed features are experimental; a stable profile switches them off.
    let stable = profile(&["beat-loop-1-to-32"], "stable");
    let message = refused(&stable, &repo_root(), &["--no-protected-set"]);
    assert!(message.contains("nothing to build"), "{message}");

    let both = profile(
        &["beat-loop-1-to-32", "beat-loop-16-plays-32"],
        "experimental",
    );
    let message = refused(&both, &repo_root(), &["--no-protected-set"]);
    assert!(message.contains("nothing to build"), "{message}");
}

#[test]
fn a_build_less_settled_than_the_profile_accepts_is_refused() {
    // A stable implementation, but under another reported version than its recipe's own: the
    // build is a new update, experimental at most.
    let root = copy_catalog();
    let feature = root
        .path()
        .join("catalog/features/beat-loop-16-plays-32.json");
    let text = std::fs::read_to_string(&feature).expect("read");
    write_bytes(
        &feature,
        text.replace("\"maturity\": \"experimental\"", "\"maturity\": \"stable\"")
            .as_bytes(),
    );
    let mut stable = profile(&["beat-loop-16-plays-32"], "stable");
    stable["reported_version"] = json!("0.13");

    let message = refused(&stable, root.path(), &["--no-protected-set"]);
    assert!(
        message.contains("the build is experimental")
            && message.contains("the profile accepts stable only"),
        "{message}"
    );
}

#[test]
fn the_release_rules_and_the_protected_set_apply_before_the_input() {
    let mut not_higher = profile(&["beat-loop-1-to-32"], "experimental");
    not_higher["label"] = json!("Ver1.15");
    let message = refused(&not_higher, &repo_root(), &["--no-protected-set"]);
    assert!(message.contains("is not higher"), "{message}");

    let fine = profile(&["beat-loop-1-to-32"], "experimental");
    let message = refused(&fine, &repo_root(), &[]);
    assert!(message.contains("no protected set"), "{message}");

    // With every check passed, the missing input is what refuses it.
    let message = refused(&fine, &repo_root(), &["--no-protected-set"]);
    assert!(message.contains("missing.UPD"), "{message}");
}

#[test]
fn the_build_prints_its_resolution_and_names_itself_in_input_refusals() {
    // A short synthetic input: every check that needs no firmware passes, then the input is
    // refused for its length, before it is read.
    let fine = profile(&["beat-loop-1-to-32"], "experimental");
    let short = [0u8; 1024];
    let (output, written) = run(&fine, &repo_root(), &["--no-protected-set"], Some(&short));
    assert!(!output.status.success());
    assert!(!written);

    let stdout = String::from_utf8_lossy(&output.stdout);
    for line in [
        "label: Ver1.16",
        "reported_version: 0.12",
        "feature beat-loop-1-to-32: on (experimental, recipes/xdj700-v1.15/beat-loop-1-to-32.json)",
        "  evidence: Rehearsed in emulation, both ways; not yet tested on hardware",
        "fragment[0]: recipes/xdj700-v1.15/beat-loop-1-to-32.json",
        "tier: experimental",
    ] {
        assert!(stdout.contains(line), "{line}\n{stdout}");
    }
    // What the build is, and how to restore, only once a file is written.
    assert!(
        !stdout.contains("note:") && !stdout.contains("restore"),
        "{stdout}"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("refusing to build '") && stderr.contains("(1024 bytes, expected"),
        "{stderr}"
    );
}
