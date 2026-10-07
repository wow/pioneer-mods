//! Output-file safety (`patch_cli::output`), exercised directly so the write-time refusals are
//! covered without the official firmware.

use patch_cli::output::{
    Overwrite, ensure_safe_output_path, is_no_clobber_unsupported, read_back,
    write_output_atomically,
};
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
fn writes_a_new_file_without_leaving_temporaries() {
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");

    write_output_atomically(&output, b"firmware", Overwrite::Never).expect("write");

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

    read_back(&path, b"firmware").expect("match");
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

#[test]
fn read_back_compares_files_larger_than_one_chunk() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("big.UPD");
    let mut expected: Vec<u8> = (0..(3 << 20) + 17).map(|i| (i % 251) as u8).collect();
    fs::write(&path, &expected).expect("seed");

    read_back(&path, &expected).expect("match");
    let last = expected.len() - 1;
    expected[last] ^= 1;
    let error = read_back(&path, &expected).expect_err("last byte differs");
    assert!(error.to_string().contains("does not match"), "{error:#}");
}

#[test]
fn refuses_a_missing_output_directory_up_front() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("XDJ700.UPD");
    fs::write(&input, b"input").expect("seed");
    let output = dir.path().join("typo").join("XDJ700.UPD");

    let error = ensure_safe_output_path(&input, &output, Overwrite::Never).expect_err("missing");

    assert!(error.to_string().contains("does not exist"), "{error:#}");
}

#[cfg(unix)]
#[test]
fn refuses_a_dangling_symlink_up_front_unless_forced() {
    let dir = tempfile::tempdir().expect("tempdir");
    let input = dir.path().join("XDJ700.UPD");
    fs::write(&input, b"input").expect("seed");
    let output = dir.path().join("out.UPD");
    std::os::unix::fs::symlink(dir.path().join("nowhere.UPD"), &output).expect("symlink");

    let error = ensure_safe_output_path(&input, &output, Overwrite::Never).expect_err("dangling");

    assert!(
        error.to_string().contains("choose a new output path"),
        "{error:#}"
    );
    assert!(ensure_safe_output_path(&input, &output, Overwrite::Forced).is_ok());
}

#[cfg(unix)]
#[test]
fn a_failed_directory_sync_keeps_the_verified_file_even_when_forced() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let sub = dir.path().join("out");
    fs::create_dir(&sub).expect("subdir");
    let output = sub.join("XDJ700.UPD");
    fs::write(&output, b"old").expect("seed");
    // Write and search, but no read: creating, reading back and renaming work; opening the
    // directory for its sync does not.
    fs::set_permissions(&sub, fs::Permissions::from_mode(0o300)).expect("chmod");
    if fs::File::open(&sub).is_ok() {
        fs::set_permissions(&sub, fs::Permissions::from_mode(0o700)).expect("restore");
        eprintln!("skipped: permissions are not enforced for this user");
        return;
    }

    let result = write_output_atomically(&output, b"new", Overwrite::Forced);

    fs::set_permissions(&sub, fs::Permissions::from_mode(0o700)).expect("restore");
    let error = result.expect_err("directory sync must fail");
    assert!(
        error.to_string().contains("durability is not confirmed"),
        "{error:#}"
    );
    assert_eq!(
        fs::read(&output).expect("read"),
        b"new",
        "verified file kept"
    );
}

#[cfg(unix)]
#[test]
fn recognises_file_systems_without_a_no_clobber_rename() {
    use std::io::{Error, ErrorKind};
    for code in [libc::ENOTSUP, libc::EOPNOTSUPP, libc::EPERM] {
        assert!(
            is_no_clobber_unsupported(&Error::from_raw_os_error(code)),
            "{code}"
        );
    }
    assert!(is_no_clobber_unsupported(&Error::from(
        ErrorKind::Unsupported
    )));
    assert!(!is_no_clobber_unsupported(&Error::from_raw_os_error(
        libc::ENOENT
    )));
    assert!(!is_no_clobber_unsupported(&Error::from(
        ErrorKind::AlreadyExists
    )));
}
