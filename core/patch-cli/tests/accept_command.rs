//! `patch-cli accept`: refusals that need no firmware and no emulator. Each comes before the
//! emulator would start; the emulator path given is a plain file that is never run.

mod common;

use common::{copy_catalog, write_bytes};
use serde_json::json;
use std::path::Path;
use std::process::Command;

fn add_test(root: &Path, id: &str, stick: bool) {
    let test = json!({
        "schema_version": 1, "id": id, "player": "xdj700-v1.15", "description": "d",
        "stick": stick,
        "steps": [{ "expect": { "brightest": {
            "region": { "x": 0, "y": 0, "w": 10, "h": 10 },
            "others": [{ "x": 10, "y": 0, "w": 10, "h": 10 }], "margin": 1 } } }],
        "maintainer": "m"
    });
    let dir = root.join("catalog/acceptance");
    std::fs::create_dir_all(&dir).expect("dir");
    write_bytes(
        &dir.join(format!("{id}.json")),
        &serde_json::to_vec(&test).expect("json"),
    );
}

/// Runs `accept` with `args` against the catalog under `root`; returns stderr.
fn refused(root: &Path, args: &[&str], emulator_env: Option<&Path>) -> String {
    let mut command = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    command.arg("accept").arg("--root").arg(root).args(args);
    command.env_remove("XDJ700_QEMU");
    if let Some(emulator) = emulator_env {
        command.env("XDJ700_QEMU", emulator);
    }
    let output = command.output().expect("run patch-cli accept");
    assert!(!output.status.success());
    assert!(
        output.stdout.is_empty(),
        "{}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn the_test_the_emulator_and_the_stick_are_checked_before_the_update_is_read() {
    let root = copy_catalog();
    add_test(root.path(), "plain", false);
    add_test(root.path(), "needs-stick", true);
    let files = tempfile::tempdir().expect("tempdir");
    let emulator = files.path().join("qemu");
    write_bytes(&emulator, b"not run");
    let missing = files.path().join("missing.UPD");
    let missing = missing.to_str().expect("utf-8");
    let emulator_arg = emulator.to_str().expect("utf-8");

    let message = refused(root.path(), &["--test", "nope", "--update", missing], None);
    assert!(message.contains("refusing to run acceptance test 'nope': it is not in the catalog"));

    let message = refused(root.path(), &["--test", "plain", "--update", missing], None);
    assert!(
        message.contains("name the emulator with --emulator or XDJ700_QEMU"),
        "{message}"
    );
    let gone = files.path().join("gone");
    let message = refused(
        root.path(),
        &["--test", "plain", "--update", missing],
        Some(&gone),
    );
    assert!(
        message.contains("the emulator '") && message.contains("is not a file"),
        "{message}"
    );

    let args = [
        "--test",
        "needs-stick",
        "--update",
        missing,
        "--emulator",
        emulator_arg,
    ];
    let message = refused(root.path(), &args, None);
    assert!(
        message.contains("it needs the owner's USB stick image (--stick)"),
        "{message}"
    );
    let mut with_stick = args.to_vec();
    with_stick.extend(["--stick", missing]);
    let message = refused(root.path(), &with_stick, None);
    assert!(message.contains("the stick image '") && message.contains("is not a file"));

    // With every check passed, the missing update is what refuses it.
    let args = [
        "--test",
        "plain",
        "--update",
        missing,
        "--emulator",
        emulator_arg,
    ];
    let message = refused(root.path(), &args, None);
    assert!(message.contains("failed to read update"), "{message}");
}

#[test]
fn an_update_that_is_not_the_players_is_refused_before_the_emulator_starts() {
    let root = copy_catalog();
    add_test(root.path(), "plain", false);
    let files = tempfile::tempdir().expect("tempdir");
    let emulator = files.path().join("qemu");
    write_bytes(&emulator, b"not run");
    let update = files.path().join("XDJ700.UPD");
    write_bytes(&update, b"not an update");
    let message = refused(
        root.path(),
        &[
            "--test",
            "plain",
            "--update",
            update.to_str().expect("utf-8"),
        ],
        Some(&emulator),
    );
    assert!(message.contains("the update does not parse"), "{message}");
}
