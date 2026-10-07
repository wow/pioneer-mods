use patch_core::{PatchCoreError, identify_bytes, identify_firmware, read_firmware};
use std::fs::File;
use std::io::Write;

#[test]
fn identifies_file_size_and_sha256() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let mut file = File::create(&input_path).expect("create temp file");
    file.write_all(b"abc").expect("write temp file");
    file.flush().expect("flush temp file");

    let identity = identify_firmware(&input_path).expect("identify firmware");

    assert_eq!(identity.file_name, "XDJ700.UPD");
    assert_eq!(identity.size_bytes, 3);
    assert_eq!(
        identity.sha256_hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn identifies_in_memory_bytes() {
    let identity = identify_bytes("XDJ700.UPD".to_owned(), b"abc");

    assert_eq!(identity.file_name, "XDJ700.UPD");
    assert_eq!(identity.size_bytes, 3);
    assert_eq!(
        identity.sha256_hex,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
}

#[test]
fn read_firmware_returns_identity_and_same_bytes() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    std::fs::write(&input_path, b"abc").expect("write input");

    let (identity, bytes) = read_firmware(&input_path).expect("read firmware");

    assert_eq!(bytes, b"abc");
    assert_eq!(identity, identify_firmware(&input_path).expect("identify"));
}

#[test]
fn read_firmware_refuses_directory() {
    let tempdir = tempfile::tempdir().expect("create tempdir");

    let error = read_firmware(tempdir.path()).expect_err("directory must be refused");

    assert!(matches!(error, PatchCoreError::InputNotAFile { .. }));
}

#[cfg(unix)]
#[test]
fn read_firmware_refuses_fifo_without_blocking() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let fifo = tempdir.path().join("pipe.UPD");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("run mkfifo");
    assert!(status.success());

    // Opening a FIFO with no writer blocks; the refusal must happen before open().
    let error = read_firmware(&fifo).expect_err("FIFO must be refused");

    assert!(matches!(error, PatchCoreError::InputNotAFile { .. }));
}
