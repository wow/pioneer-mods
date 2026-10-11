//! Synthetic updates for the acceptance runner's tests: S-record documents with our own records,
//! a MAIN document and a PANL one. No vendor bytes.

use patch_core::upd::crc16_xmodem;

/// One S-record line (with CRLF), its checksum computed.
pub fn record(record_type: u8, address_len: usize, address: u32, data: &[u8]) -> Vec<u8> {
    let mut raw = vec![u8::try_from(address_len + data.len() + 1).expect("short")];
    raw.extend_from_slice(&address.to_be_bytes()[4 - address_len..]);
    raw.extend_from_slice(data);
    raw.push(!raw.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte)));
    let hex: String = raw.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("S{}{hex}\r\n", record_type as char).into_bytes()
}

pub fn document(model: &str, kind: &str, lines: &[Vec<u8>]) -> Vec<u8> {
    let mut doc = format!("{model:<12}{kind}Ver1.15").into_bytes();
    doc.extend_from_slice(b"\0       0");
    assert_eq!(doc.len(), 32);
    for line in lines {
        doc.extend_from_slice(line);
    }
    let crc = crc16_xmodem(&doc);
    doc.extend_from_slice(&crc.to_le_bytes());
    doc
}

/// A synthetic update whose MAIN document holds `records` (address, data).
pub fn update(model: &str, records: &[(u32, &[u8])]) -> Vec<u8> {
    let mut lines = vec![record(b'0', 2, 0, b"synthetic")];
    lines.extend(records.iter().map(|(at, data)| record(b'2', 3, *at, data)));
    lines.push(record(b'8', 3, 0, &[]));
    let panel = vec![
        record(b'0', 2, 0, &[]),
        record(b'2', 3, 0x0C_0000, &[0x44; 3]),
        record(b'8', 3, 0, &[]),
    ];
    let documents = [
        document(model, "MAIN", &lines),
        document(model, "PANL", &panel),
    ];
    let mut bytes = Vec::new();
    for doc in &documents {
        bytes.extend_from_slice(format!("{}\r\n", doc.len()).as_bytes());
    }
    for doc in &documents {
        bytes.extend_from_slice(doc);
    }
    bytes
}
