mod common;

use common::*;
use patch_core::{RecordDefect, UpdError, parse_upd};

#[test]
fn rejects_record_checksum_mismatch_even_with_valid_document_crc() {
    let mut line = record(b'2', 3, 0x000004, &[0x22; 4]);
    let checksum_pos = line.len() - 3;
    line[checksum_pos] = if line[checksum_pos] == b'0' {
        b'1'
    } else {
        b'0'
    };

    assert_eq!(
        parse_upd(&container_with_main_line(2, &line)),
        Err(UpdError::MalformedRecord {
            document_index: 0,
            record_index: 2,
            defect: RecordDefect::ChecksumMismatch,
        })
    );
}

#[test]
fn rejects_lowercase_hex() {
    let line = String::from_utf8(record(b'2', 3, 0x000004, &[0xAB; 4]))
        .expect("ascii")
        .to_ascii_lowercase()
        .replacen('s', "S", 1);

    assert!(matches!(
        parse_upd(&container_with_main_line(2, line.as_bytes())),
        Err(UpdError::MalformedRecord {
            defect: RecordDefect::Syntax,
            ..
        })
    ));
}

#[test]
fn rejects_byte_count_mismatch() {
    let mut line = record(b'2', 3, 0x000004, &[0x22; 4]);
    line[3] = b'9'; // count 0x08 -> 0x09

    assert!(matches!(
        parse_upd(&container_with_main_line(2, &line)),
        Err(UpdError::MalformedRecord {
            defect: RecordDefect::ByteCountMismatch,
            ..
        })
    ));
}

#[test]
fn rejects_unsupported_record_type() {
    let line = record(b'4', 2, 0, &[0x00]);

    assert!(matches!(
        parse_upd(&container_with_main_line(2, &line)),
        Err(UpdError::MalformedRecord {
            defect: RecordDefect::UnsupportedType,
            ..
        })
    ));
}

#[test]
fn rejects_bare_lf_line_ending() {
    let mut line = record(b'2', 3, 0x000004, &[0x22; 4]);
    line.remove(line.len() - 2);

    assert!(matches!(
        parse_upd(&container_with_main_line(2, &line)),
        Err(UpdError::MalformedRecord {
            defect: RecordDefect::Syntax,
            ..
        })
    ));
}

#[test]
fn rejects_overlapping_data_records() {
    let line = record(b'2', 3, 0x000002, &[0x22; 4]);

    assert!(matches!(
        parse_upd(&container_with_main_line(2, &line)),
        Err(UpdError::UnexpectedRecordLayout {
            record_index: 2,
            ..
        })
    ));
}

#[test]
fn rejects_missing_header_record() {
    let line = record(b'2', 3, 0x0000F0, &[0x22; 4]);

    assert!(matches!(
        parse_upd(&container_with_main_line(0, &line)),
        Err(UpdError::UnexpectedRecordLayout {
            record_index: 0,
            ..
        })
    ));
}

#[test]
fn rejects_termination_record_before_end() {
    let line = record(b'7', 4, 0, &[]);

    assert!(matches!(
        parse_upd(&container_with_main_line(2, &line)),
        Err(UpdError::UnexpectedRecordLayout {
            record_index: 2,
            ..
        })
    ));
}

#[test]
fn rejects_document_without_data_records() {
    let lines = vec![record(b'0', 2, 0, &[]), record(b'7', 4, 0, &[])];
    let bytes = container(&[document("MAIN", &lines)]);

    assert!(matches!(
        parse_upd(&bytes),
        Err(UpdError::UnexpectedRecordLayout { .. })
    ));
}

#[test]
fn rejects_data_beyond_address_space() {
    let line = record(b'1', 2, 0xFFFE, &[0x22; 4]);

    assert!(matches!(
        parse_upd(&container_with_main_line(3, &line)),
        Err(UpdError::MalformedRecord {
            defect: RecordDefect::AddressOverflow,
            ..
        })
    ));
}
