mod common;

use common::*;
use patch_core::{LayoutViolation, RecordDefect, UpdContainer, UpdError, parse_upd};

fn layout_error(record_index: usize, violation: LayoutViolation) -> Result<UpdContainer, UpdError> {
    Err(UpdError::UnexpectedRecordLayout {
        document_index: 0,
        record_index,
        violation,
    })
}

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

    assert_eq!(
        parse_upd(&container_with_main_line(2, &line)),
        layout_error(2, LayoutViolation::DataNotAscending)
    );
}

#[test]
fn rejects_missing_header_record() {
    let line = record(b'2', 3, 0x0000F0, &[0x22; 4]);

    assert!(matches!(
        parse_upd(&container_with_main_line(0, &line)),
        Err(UpdError::UnexpectedRecordLayout {
            document_index: 0,
            record_index: 0,
            violation: LayoutViolation::HeaderNotFirst,
        })
    ));
}

#[test]
fn rejects_termination_record_before_end() {
    let line = record(b'7', 4, 0, &[]);

    assert_eq!(
        parse_upd(&container_with_main_line(2, &line)),
        layout_error(2, LayoutViolation::NonDataRecordInBody)
    );
}

#[test]
fn rejects_document_without_data_records() {
    let lines = vec![record(b'0', 2, 0, &[]), record(b'7', 4, 0, &[])];
    let bytes = container(&[document("MAIN", &lines)]);

    assert_eq!(
        parse_upd(&bytes),
        layout_error(1, LayoutViolation::NoDataRecords)
    );
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

#[test]
fn rejects_non_termination_record_last() {
    let line = record(b'5', 2, 3, &[]);

    assert_eq!(
        parse_upd(&container_with_main_line(4, &line)),
        layout_error(4, LayoutViolation::TerminationNotLast)
    );
}

#[test]
fn rejects_termination_record_with_data() {
    let line = record(b'7', 4, 0xA000_0000, &[0x01]);

    assert_eq!(
        parse_upd(&container_with_main_line(4, &line)),
        layout_error(4, LayoutViolation::TerminationHasData)
    );
}

#[test]
fn rejects_count_record_between_data_records() {
    let line = record(b'5', 2, 3, &[]);

    assert_eq!(
        parse_upd(&container_with_main_line(2, &line)),
        layout_error(2, LayoutViolation::NonDataRecordInBody)
    );
}

#[test]
fn rejects_second_header_record_in_body() {
    let line = record(b'0', 2, 0, &[]);

    assert_eq!(
        parse_upd(&container_with_main_line(2, &line)),
        layout_error(2, LayoutViolation::NonDataRecordInBody)
    );
}

#[test]
fn rejects_empty_data_record() {
    let line = record(b'2', 3, 0x000004, &[]);

    assert_eq!(
        parse_upd(&container_with_main_line(2, &line)),
        layout_error(2, LayoutViolation::EmptyDataRecord)
    );
}

#[test]
fn rejects_record_shorter_than_address_and_checksum() {
    // S1 needs a 2-byte address plus a checksum byte; this record carries only 2 bytes in total.
    let line = b"S10200FD\r\n".to_vec();

    assert_eq!(
        parse_upd(&container_with_main_line(2, &line)),
        Err(UpdError::MalformedRecord {
            document_index: 0,
            record_index: 2,
            defect: RecordDefect::TooShort,
        })
    );
}
