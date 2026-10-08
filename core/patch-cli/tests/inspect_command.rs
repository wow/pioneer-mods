use patch_core::sha256_hex;
use patch_core::upd::crc16_xmodem;
use patch_core::xdj700::{VERSION_STRING_OFFSET, encode_section, section_checksum};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

/// Encodes one S-record line with computed byte count and checksum, CRLF-terminated.
fn srec(record_type: char, address_len: usize, address: u32, data: &[u8]) -> String {
    let mut raw = vec![(address_len + data.len() + 1) as u8];
    raw.extend_from_slice(&address.to_be_bytes()[4 - address_len..]);
    raw.extend_from_slice(data);
    let checksum = !raw.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte));
    raw.push(checksum);
    let hex: String = raw.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("S{record_type}{hex}\r\n")
}

/// Synthetic one-document container (no vendor bytes) with a valid CRC and length header.
fn upd(lines: &[String]) -> Vec<u8> {
    upd_for_model("SYN-100", lines)
}

fn upd_for_model(model: &str, lines: &[String]) -> Vec<u8> {
    upd_for(model, "Ver9.99", lines)
}

fn upd_for(model: &str, version: &str, lines: &[String]) -> Vec<u8> {
    let mut doc = format!("{model:<12}MAIN{version}").into_bytes();
    doc.extend_from_slice(b"\0       0");
    for line in lines {
        doc.extend_from_slice(line.as_bytes());
    }
    let crc = crc16_xmodem(&doc);
    doc.extend_from_slice(&crc.to_le_bytes());
    let mut bytes = format!("{}\r\n", doc.len()).into_bytes();
    bytes.extend_from_slice(&doc);
    bytes
}

/// S0 header, two S2 data records with a gap, S7 termination.
fn synthetic_upd() -> Vec<u8> {
    upd(&[
        srec('0', 2, 0, &[]),
        srec('2', 3, 0x000000, &[0x11, 0x22, 0x33]),
        srec('2', 3, 0x000010, &[0x44, 0x55, 0x66]),
        srec('7', 4, 0, &[]),
    ])
}

/// Two 1-byte S3 records at addresses 0 and `far`, so the image span is `far + 1` bytes.
fn sparse_upd(far: u32) -> Vec<u8> {
    upd(&[
        srec('0', 2, 0, &[]),
        srec('3', 4, 0, &[1]),
        srec('3', 4, far, &[2]),
        srec('7', 4, 0, &[]),
    ])
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
    assert!(
        stdout.contains("roundtrip: byte-identical"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("document[0].image: base=0x00000000 len=19 sha256="),
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
    assert_eq!(json["roundtrip_verified"], true);
    assert_eq!(
        json["container"]["documents"][0]["image"]["status"],
        "reconstructed"
    );
    assert_eq!(json["container"]["documents"][0]["image"]["len"], 19);
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

#[test]
fn inspect_structure_reports_oversized_image_without_failing() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("SPARSE.UPD");
    fs::write(&input, sparse_upd(64 << 20)).expect("write input");

    let output = run_inspect(&input, &["--structure"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("roundtrip: byte-identical"),
        "stdout: {stdout}"
    );
    assert!(
        stdout.contains("document[0].image: not reconstructed (span 67108865 bytes exceeds"),
        "stdout: {stdout}"
    );
}

const XDJ700_STREAM: [u8; 13] = [0xFD, 0x00, 0xEE, 0xFF, 1, 2, 3, 4, 5, 6, 0x03, 7, 8];

/// Size field (u32 LE) followed by the stream: the bytes the section checksum covers.
fn xdj700_section_bytes() -> Vec<u8> {
    let mut section = (XDJ700_STREAM.len() as u32).to_le_bytes().to_vec();
    section.extend_from_slice(&XDJ700_STREAM);
    section
}

/// XDJ-700 MAIN container (`version`) whose image holds a compressed section at 0x40000 that
/// decodes to 19 zeros followed by 1..=8. `corrupt` flips one checksum bit.
fn xdj700_upd(corrupt: bool) -> Vec<u8> {
    xdj700_upd_version("Ver1.15", corrupt)
}

fn xdj700_upd_version(version: &str, corrupt: bool) -> Vec<u8> {
    let mut section = xdj700_section_bytes();
    let mut checksum = section_checksum(&section);
    if corrupt {
        checksum ^= 1;
    }
    section.extend_from_slice(&checksum.to_le_bytes());
    let mut image = vec![0xFF; 0x40000];
    image.extend_from_slice(&section);

    let mut lines = vec![srec('0', 2, 0, &[])];
    for (index, chunk) in image.chunks(32).enumerate() {
        lines.push(srec('2', 3, (index * 32) as u32, chunk));
    }
    lines.push(srec('7', 4, 0, &[]));
    upd_for("XDJ-700", version, &lines)
}

#[test]
fn inspect_structure_decodes_xdj700_application_section() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("XDJ700.UPD");
    fs::write(&input, xdj700_upd(false)).expect("write input");

    let output = run_inspect(&input, &["--structure", "--format", "json"]);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let json: serde_json::Value = serde_json::from_slice(&output.stdout).expect("valid JSON");
    let application = &json["application"];
    assert_eq!(application["status"], "decoded");
    assert_eq!(application["offset"], "0x40000");
    assert_eq!(application["decoded_len"], 27);
    assert_eq!(application["compressed_len"], 13);
    assert_eq!(
        application["checksum"],
        format!("0x{:04X}", section_checksum(&xdj700_section_bytes()))
    );
    let mut decoded = vec![0u8; 19];
    decoded.extend(1..=8);
    assert_eq!(application["decoded_sha256"], sha256_hex(&decoded));
    assert!(
        application["reported_version"].is_null() && application.get("reported_version").is_some(),
        "a 27-byte application holds no version string, reported as null: {application}"
    );
    let text = run_inspect(&input, &["--structure"]);
    assert!(String::from_utf8_lossy(&text.stdout).contains(" reported_version=none"));
}

#[test]
fn inspect_structure_reports_the_application_reported_version() {
    let mut decoded = vec![0u8; 0x800];
    decoded[VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + 5].copy_from_slice(b"0.10\0");
    let mut image = vec![0xFF; 0x40000];
    image.extend(encode_section(&decoded).expect("encode"));
    let mut lines = vec![srec('0', 2, 0, &[])];
    for (index, chunk) in image.chunks(32).enumerate() {
        lines.push(srec('2', 3, (index * 32) as u32, chunk));
    }
    lines.push(srec('7', 4, 0, &[]));
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("XDJ700.UPD");
    fs::write(&input, upd_for("XDJ-700", "Ver1.15", &lines)).expect("write input");

    let json_output = run_inspect(&input, &["--structure", "--format", "json"]);
    let text_output = run_inspect(&input, &["--structure"]);

    let json: serde_json::Value = serde_json::from_slice(&json_output.stdout).expect("valid JSON");
    assert_eq!(json["application"]["reported_version"], "0.10", "{json}");
    let text = String::from_utf8_lossy(&text_output.stdout);
    assert!(text.contains(" reported_version=0.10"), "{text}");
}

#[test]
fn inspect_structure_reports_invalid_application_section_without_failing() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("XDJ700.UPD");
    fs::write(&input, xdj700_upd(true)).expect("write input");

    let output = run_inspect(&input, &["--structure"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(
        stdout.contains("application: invalid (section checksum mismatch"),
        "stdout: {stdout}"
    );
}

#[test]
fn inspect_structure_omits_application_for_other_models() {
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
    assert!(json.get("application").is_none(), "json: {json}");
}

#[test]
fn inspect_structure_reports_unverified_main_version_as_unsupported() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("XDJ700.UPD");
    fs::write(&input, xdj700_upd_version("Ver1.16", false)).expect("write input");

    let output = run_inspect(&input, &["--structure"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(
        stdout.contains("application: unsupported (MAIN version Ver1.16 has no verified"),
        "stdout: {stdout}"
    );
}

#[test]
fn inspect_structure_reports_unreconstructed_main_image() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("XDJ700.UPD");
    let lines = [
        srec('0', 2, 0, &[]),
        srec('3', 4, 0, &[1]),
        srec('3', 4, 64 << 20, &[2]),
        srec('7', 4, 0, &[]),
    ];
    fs::write(&input, upd_for("XDJ-700", "Ver1.15", &lines)).expect("write input");

    let output = run_inspect(&input, &["--structure"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(
        stdout.contains("application: invalid (MAIN image was not reconstructed"),
        "stdout: {stdout}"
    );
}

#[test]
fn inspect_structure_reports_ambiguous_main_documents() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input = tempdir.path().join("XDJ700.UPD");
    let single = xdj700_upd(false);
    let doc_start = single.iter().position(|&b| b == b'X').expect("model byte");
    let doc = &single[doc_start..];
    let mut bytes = format!("{}\r\n{}\r\n", doc.len(), doc.len()).into_bytes();
    bytes.extend_from_slice(doc);
    bytes.extend_from_slice(doc);
    fs::write(&input, bytes).expect("write input");

    let output = run_inspect(&input, &["--structure"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(output.status.success(), "stdout: {stdout}");
    assert!(
        stdout.contains("application: invalid (container has 2 XDJ-700 MAIN documents"),
        "stdout: {stdout}"
    );
}
