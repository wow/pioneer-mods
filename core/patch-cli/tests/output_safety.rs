//! Output-file safety (`patch_cli::output`), exercised directly so the write-time refusals are
//! covered without the official firmware.

use patch_cli::output::{Overwrite, ensure_safe_output_path, read_back, write_output_atomically};
use patch_core::sha256_hex;
use std::fs;
use std::path::Path;

fn entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("list dir")
        .map(|entry| {
            entry
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .collect();
    names.sort();
    names
}

#[test]
fn writes_a_new_file_and_returns_its_hash_without_leaving_temporaries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");

    let sha256 = write_output_atomically(&output, b"firmware", Overwrite::Never).expect("write");

    assert_eq!(sha256, sha256_hex(b"firmware"));
    assert_eq!(fs::read(&output).expect("read"), b"firmware");
    assert_eq!(entries(dir.path()), ["XDJ700.UPD"]);
}

#[test]
fn never_refuses_an_existing_file_at_write_time_and_keeps_it() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");
    fs::write(&output, b"keep me").expect("seed");

    let error =
        write_output_atomically(&output, b"firmware", Overwrite::Never).expect_err("must refuse");

    assert!(
        error.to_string().contains("choose a new output path"),
        "{error:#}"
    );
    assert_eq!(fs::read(&output).expect("read"), b"keep me");
    assert_eq!(
        entries(dir.path()),
        ["XDJ700.UPD"],
        "no temporary file left"
    );
}

#[test]
fn refuse_unless_forced_names_the_force_flag_and_forced_overwrites() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");
    fs::write(&output, b"old").expect("seed");

    let error = write_output_atomically(&output, b"new", Overwrite::RefuseUnlessForced)
        .expect_err("must refuse");
    assert!(
        error.to_string().contains("pass --force to overwrite"),
        "{error:#}"
    );
    assert_eq!(fs::read(&output).expect("read"), b"old");

    write_output_atomically(&output, b"new", Overwrite::Forced).expect("forced write");
    assert_eq!(fs::read(&output).expect("read"), b"new");
    assert_eq!(entries(dir.path()), ["XDJ700.UPD"]);
}

#[cfg(unix)]
#[test]
fn never_refuses_a_dangling_symlink_and_does_not_create_its_target() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");
    let target = dir.path().join("elsewhere.UPD");
    std::os::unix::fs::symlink(&target, &output).expect("symlink");

    let error =
        write_output_atomically(&output, b"firmware", Overwrite::Never).expect_err("must refuse");

    assert!(
        error.to_string().contains("choose a new output path"),
        "{error:#}"
    );
    assert!(!target.exists(), "symlink target must not be created");
    assert!(fs::symlink_metadata(&output).expect("link").is_symlink());
}

#[test]
fn read_back_accepts_matching_bytes_and_refuses_others() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("XDJ700.UPD");
    fs::write(&path, b"firmware").expect("seed");

    assert_eq!(
        read_back(&path, b"firmware").expect("match"),
        sha256_hex(b"firmware")
    );
    let error = read_back(&path, b"firmwarf").expect_err("mismatch");
    assert!(
        error
            .to_string()
            .contains("does not match the verified bytes"),
        "{error:#}"
    );
    let error = read_back(&path, b"firmware!").expect_err("length mismatch");
    assert!(error.to_string().contains("do not use it"), "{error:#}");
}

#[test]
fn read_back_refuses_a_non_regular_file() {
    let dir = tempfile::tempdir().expect("tempdir");

    let error = read_back(dir.path(), b"").expect_err("directory");

    assert!(
        error.to_string().contains("failed to read back"),
        "{error:#}"
    );
}

#[test]
fn output_path_may_not_resolve_to_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("XDJ700.UPD");
    fs::write(&input, b"input").expect("seed");
    let alias = dir.path().join(".").join("XDJ700.UPD");

    for overwrite in [Overwrite::Never, Overwrite::Forced] {
        let error = ensure_safe_output_path(&input, &alias, overwrite).expect_err("alias");
        assert!(
            error.to_string().contains("same path as input"),
            "{error:#}"
        );
    }
    assert!(ensure_safe_output_path(&input, &dir.path().join("new.UPD"), Overwrite::Never).is_ok());
}
