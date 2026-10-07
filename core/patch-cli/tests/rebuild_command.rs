//! `patch-cli rebuild` refusal paths. A successful rebuild needs the official file, so it is
//! covered by the owner-input test in `official_rebuild_command.rs`.

mod common;

use common::write_bytes;
use std::path::Path;
use std::process::{Command, Output};

fn run_rebuild(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("rebuild")
        .args(args)
        .output()
        .expect("run patch-cli rebuild")
}

fn rebuild(input: &Path, output: &Path, label: &str) -> Output {
    run_rebuild(&[
        "--input",
        input.to_str().expect("utf-8 path"),
        "--application",
        "stock",
        "--label",
        label,
        "--output",
        output.to_str().expect("utf-8 path"),
    ])
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn refuses_an_input_other_than_the_official_update_without_writing() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("XDJ700.UPD");
    let output = dir.path().join("rebuilt.UPD");
    write_bytes(&input, b"not the official update");

    let result = rebuild(&input, &output, "Ver1.15");

    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("is not the official XDJ-700 v1.15 update"),
        "{}",
        stderr(&result)
    );
    assert!(!output.exists(), "no output may be written");
}

#[test]
fn refuses_a_malformed_label_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let missing_input = dir.path().join("missing.UPD");
    let output = dir.path().join("rebuilt.UPD");

    let result = rebuild(&missing_input, &output, "1.22");

    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("version label \"1.22\" is not of the form VerX.YY"),
        "{}",
        stderr(&result)
    );
    assert!(!output.exists());
}

#[test]
fn never_overwrites_an_existing_output() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("XDJ700.UPD");
    let output = dir.path().join("rebuilt.UPD");
    write_bytes(&input, b"input");
    write_bytes(&output, b"keep me");

    let result = rebuild(&input, &output, "Ver1.15");

    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("refusing to overwrite existing output file")
            && stderr(&result).contains("choose a new output path"),
        "{}",
        stderr(&result)
    );
    assert_eq!(std::fs::read(&output).expect("read output"), b"keep me");
}

#[test]
fn refuses_to_write_over_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("XDJ700.UPD");
    write_bytes(&input, b"input");

    let result = rebuild(&input, &input, "Ver1.15");

    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("same path as input"),
        "{}",
        stderr(&result)
    );
    assert_eq!(std::fs::read(&input).expect("read input"), b"input");
}

#[test]
fn refuses_a_non_regular_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("rebuilt.UPD");

    let result = rebuild(dir.path(), &output, "Ver1.15");

    assert!(!result.status.success());
    assert!(!output.exists());
}

#[test]
fn requires_an_explicit_label_and_application() {
    let without_label = run_rebuild(&[
        "--input",
        "in.UPD",
        "--application",
        "stock",
        "--output",
        "out.UPD",
    ]);
    let without_application = run_rebuild(&[
        "--input", "in.UPD", "--label", "Ver1.15", "--output", "out.UPD",
    ]);
    let unknown_application = run_rebuild(&[
        "--input",
        "in.UPD",
        "--application",
        "custom.bin",
        "--label",
        "Ver1.15",
        "--output",
        "out.UPD",
    ]);

    assert!(stderr(&without_label).contains("--label <LABEL>"));
    assert!(stderr(&without_application).contains("--application <APPLICATION>"));
    assert!(stderr(&unknown_application).contains("invalid value 'custom.bin'"));
    for result in [without_label, without_application, unknown_application] {
        assert_eq!(result.status.code(), Some(2), "clap usage error");
    }
}
