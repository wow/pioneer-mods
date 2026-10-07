//! Synthetic `.UPD` builders shared by the parser test crates (no vendor bytes).
// Each test crate uses a different subset of these helpers.
#![allow(dead_code)]

use patch_core::upd::crc16_xmodem;
/// Formats one S-record line (with CRLF) using a computed checksum.
pub fn record(record_type: u8, address_len: usize, address: u32, data: &[u8]) -> Vec<u8> {
    let mut raw = vec![(address_len + data.len() + 1) as u8];
    raw.extend_from_slice(&address.to_be_bytes()[4 - address_len..]);
    raw.extend_from_slice(data);
    let checksum = !raw.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte));
    raw.push(checksum);
    let hex: String = raw.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("S{}{hex}\r\n", record_type as char).into_bytes()
}

pub fn descriptor(kind: &str, version: &str) -> Vec<u8> {
    let mut bytes = format!("{:<12}{kind}{version}", "SYN-100").into_bytes();
    bytes.extend_from_slice(b"\0       0");
    assert_eq!(bytes.len(), 32);
    bytes
}

/// Builds a document from raw record lines and appends a valid CRC trailer.
pub fn document(kind: &str, lines: &[Vec<u8>]) -> Vec<u8> {
    let mut doc = descriptor(kind, "Ver9.99");
    for line in lines {
        doc.extend_from_slice(line);
    }
    let crc = crc16_xmodem(&doc);
    doc.extend_from_slice(&crc.to_le_bytes());
    doc
}

pub fn container(documents: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = Vec::new();
    for doc in documents {
        bytes.extend_from_slice(format!("{}\r\n", doc.len()).as_bytes());
    }
    for doc in documents {
        bytes.extend_from_slice(doc);
    }
    bytes
}

pub fn main_lines() -> Vec<Vec<u8>> {
    vec![
        record(b'0', 2, 0, b"synthetic"),
        record(b'2', 3, 0x000000, &[0x11; 4]),
        record(b'2', 3, 0x000004, &[0x22; 4]),
        record(b'2', 3, 0x000010, &[0x33; 2]),
        record(b'7', 4, 0xA000_0000, &[]),
    ]
}

pub fn panel_lines() -> Vec<Vec<u8>> {
    vec![
        record(b'0', 2, 0, &[]),
        record(b'2', 3, 0x0C0000, &[0x44; 3]),
        record(b'8', 3, 0, &[]),
    ]
}

pub fn valid_container() -> Vec<u8> {
    container(&[
        document("MAIN", &main_lines()),
        document("PANL", &panel_lines()),
    ])
}

/// Replaces one line of the MAIN document and rebuilds a container with valid CRCs.
pub fn container_with_main_line(index: usize, line: &[u8]) -> Vec<u8> {
    let mut lines = main_lines();
    lines[index] = line.to_vec();
    container(&[document("MAIN", &lines), document("PANL", &panel_lines())])
}

/// Builds a document from an arbitrary 32-byte descriptor and appends a valid CRC trailer.
pub fn document_with_descriptor(descriptor: &[u8], lines: &[Vec<u8>]) -> Vec<u8> {
    let mut doc = descriptor.to_vec();
    for line in lines {
        doc.extend_from_slice(line);
    }
    let crc = crc16_xmodem(&doc);
    doc.extend_from_slice(&crc.to_le_bytes());
    doc
}
