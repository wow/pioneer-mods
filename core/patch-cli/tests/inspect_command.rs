use patch_core::upd::crc16_xmodem;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// Minimal synthetic one-document container: S0 header, two S2 data records with a gap, S7.
fn synthetic_upd() -> Vec<u8> {
    let mut doc = b"SYN-100     MAINVer9.99\0       0".to_vec();
    for line in [
        "S0030000FC",
        "S20700000011223392",
        "S207000010445566E9",
        "S70500000000FA",
    ] {
        doc.extend_from_slice(line.as_bytes());
        doc.extend_from_slice(b"\r\n");
    }
    let crc = crc16_xmodem(&doc);
    doc.extend_from_slice(&crc.to_le_bytes());

    let mut bytes = format!("{}\r\n", doc.len()).into_bytes();
    bytes.extend_from_slice(&doc);
    bytes
}

fn run_inspect(input: &Path, extra_args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("inspect")
        .arg("--input")
        .arg(input)
        .args(extra_args)
        .output()
        .expect("run patch-cli inspect")
}

#[test]
fn inspect_structure_reports_documents_as_text() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("SYN100.UPD");
    fs::write(&input, synthetic_upd()).expect("write input");

    let output = run_inspect(&input, &["--structure"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(stdout.contains("documents: 1"), "stdout: {stdout}");
    assert!(
        stdout.contains(r#"kind=MAIN model="SYN-100" version="Ver9.99""#),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("data=2 types=S2 bytes=6 termination=S7"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("0x000000..0x000003 0x000010..0x000013"),
        "stdout: {stdout}"
    );
}

#[test]
fn inspect_structure_reports_json_with_identity_fields() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("SYN100.UPD");
    fs::write(&input, synthetic_upd()).expect("write input");

    let output = run_inspect(&input, &["--structure", "--format", "json"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(json["file_name"], "SYN100.UPD");
    assert_eq!(json["container"]["documents"][0]["kind"], "MAIN");
    assert_eq!(json["container"]["documents"][0]["data_records"], 2);
}

#[test]
fn inspect_without_structure_keeps_identity_only_output() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("blob.bin");
    fs::write(&input, b"abc").expect("write input");

    let output = run_inspect(&input, &["--format", "json"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    assert_eq!(json["size_bytes"], 3);
    assert!(json.get("container").is_none(), "json: {json}");
}

#[test]
fn inspect_structure_rejects_corrupted_container() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("SYN100.UPD");
    let mut bytes = synthetic_upd();
    let last = bytes.len() - 1;
    bytes[last] ^= 0xFF;
    fs::write(&input, &bytes).expect("write input");

    let output = run_inspect(&input, &["--structure"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("is not a valid .UPD container"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("CRC-16 mismatch"), "stderr: {stderr}");
    assert!(output.stdout.is_empty(), "no partial report on refusal");
}

#[test]
fn inspect_structure_refuses_directory_input() {
    let tempdir = tempfile::tempdir().expect("create tempdir");

    let output = run_inspect(tempdir.path(), &["--structure"]);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("does not point to a regular file"),
        "stderr: {stderr}"
    );
}
