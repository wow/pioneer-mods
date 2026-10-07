mod common;

use common::*;
use patch_core::upd::{SRecordType, crc16_xmodem};
use patch_core::{UpdContainer, UpdError, parse_upd};

#[test]
fn crc16_xmodem_matches_known_check_value() {
    assert_eq!(crc16_xmodem(b"123456789"), 0x31C3);
    assert_eq!(crc16_xmodem(b""), 0x0000);
}

#[test]
fn parses_two_document_container() {
    let bytes = valid_container();

    let parsed = parse_upd(&bytes).expect("valid container");

    let [main, panel] = parsed.documents() else {
        panic!("expected two documents");
    };
    assert_eq!(main.descriptor().model(), "SYN-100");
    assert_eq!(main.descriptor().kind(), "MAIN");
    assert_eq!(main.descriptor().version(), "Ver9.99");
    assert_eq!(main.descriptor().reserved(), b"\0       0");
    assert_eq!(main.header().data(), b"synthetic");
    assert_eq!(main.data_records().len(), 3);
    assert_eq!(main.records().count(), 5);
    assert_eq!(main.data_extents(), vec![0..8, 0x10..0x12]);
    assert_eq!(main.termination().record_type(), SRecordType::S7);
    assert_eq!(main.termination().address(), 0xA000_0000);
    assert_eq!(
        main.crc16(),
        crc16_xmodem(&bytes[main.offset()..main.offset() + main.length() - 2])
    );
    assert!(main.offset() < panel.offset());

    assert_eq!(panel.descriptor().kind(), "PANL");
    assert_eq!(panel.data_extents(), vec![0x0C0000..0x0C0003]);
    assert_eq!(panel.offset() + panel.length(), bytes.len());
}

#[test]
fn summary_reports_structure_deterministically() {
    let bytes = valid_container();
    let parsed = parse_upd(&bytes).expect("valid container");

    let summary = parsed.summary().expect("summary");

    let again = parse_upd(&bytes).expect("reparse").summary();
    assert_eq!(Ok(summary.clone()), again);
    let main = &summary.documents[0];
    assert_eq!(main.data_records, 3);
    assert_eq!(main.data_bytes, 10);
    assert_eq!(main.data_record_types, vec![SRecordType::S2]);
    assert_eq!(main.termination_type, SRecordType::S7);
    assert_eq!(main.entry_address, "0xA0000000");
    assert_eq!(main.header_text, "synthetic");
    assert_eq!(main.reserved_hex, "002020202020202030");
    assert_eq!(main.extents.len(), 2);
    assert_eq!(summary.documents[1].termination_type, SRecordType::S8);
}

#[test]
fn rejects_missing_length_header() {
    let bytes = document("MAIN", &main_lines());

    assert_eq!(parse_upd(&bytes), Err(UpdError::MissingLengthHeader));
}

#[test]
fn rejects_zero_padded_length_line_as_malformed() {
    let main = document("MAIN", &main_lines());
    let panel = document("PANL", &panel_lines());
    let mut bytes = format!("{}\r\n0{}\r\n", main.len(), panel.len()).into_bytes();
    bytes.extend_from_slice(&main);
    bytes.extend_from_slice(&panel);

    assert_eq!(parse_upd(&bytes), Err(UpdError::MalformedLengthHeader));
}

#[test]
fn rejects_zero_length_line_as_malformed() {
    let mut bytes = b"0\r\n".to_vec();
    bytes.extend_from_slice(&valid_container());

    assert_eq!(parse_upd(&bytes), Err(UpdError::MalformedLengthHeader));
}

#[test]
fn rejects_twenty_digit_length_line_as_malformed() {
    let mut bytes = b"12345678901234567890\r\n".to_vec();
    bytes.extend_from_slice(&document("MAIN", &main_lines()));

    assert_eq!(parse_upd(&bytes), Err(UpdError::MalformedLengthHeader));
}

#[test]
fn rejects_length_line_without_cr_as_malformed() {
    let main = document("MAIN", &main_lines());
    let mut bytes = format!("{}\n", main.len()).into_bytes();
    bytes.extend_from_slice(&main);

    assert_eq!(parse_upd(&bytes), Err(UpdError::MalformedLengthHeader));
}

#[test]
fn rejects_truncated_container() {
    let mut bytes = valid_container();
    bytes.pop();

    assert!(matches!(
        parse_upd(&bytes),
        Err(UpdError::LengthHeaderMismatch { .. })
    ));
}

#[test]
fn rejects_trailing_bytes() {
    let mut bytes = valid_container();
    bytes.push(0);

    assert!(matches!(
        parse_upd(&bytes),
        Err(UpdError::LengthHeaderMismatch { .. })
    ));
}

#[test]
fn rejects_document_crc_mismatch() {
    let mut bytes = valid_container();
    let header_len = bytes.iter().position(|&b| b == b'S').expect("model byte");
    // Flip a byte inside the MAIN descriptor model field.
    bytes[header_len + 1] ^= 0x01;

    assert!(matches!(
        parse_upd(&bytes),
        Err(UpdError::DocumentCrcMismatch {
            document_index: 0,
            ..
        })
    ));
}

#[test]
fn rejects_missing_final_crlf() {
    let mut doc = descriptor("MAIN", "Ver9.99");
    for line in main_lines() {
        doc.extend_from_slice(&line);
    }
    doc.truncate(doc.len() - 2);
    let crc = crc16_xmodem(&doc);
    doc.extend_from_slice(&crc.to_le_bytes());

    assert_eq!(
        parse_upd(&container(&[doc])),
        Err(UpdError::MissingFinalCrlf { document_index: 0 })
    );
}

#[test]
fn rejects_non_printable_descriptor_model() {
    let mut doc = descriptor("MAIN", "Ver9.99");
    doc[0] = 0x01;
    for line in main_lines() {
        doc.extend_from_slice(&line);
    }
    let crc = crc16_xmodem(&doc);
    doc.extend_from_slice(&crc.to_le_bytes());

    assert_eq!(
        parse_upd(&container(&[doc])),
        Err(UpdError::InvalidDescriptor {
            document_index: 0,
            field: "model"
        })
    );
}

#[test]
fn rejects_document_too_short() {
    let bytes = container(&[vec![b'X'; 10]]);

    assert!(matches!(
        parse_upd(&bytes),
        Err(UpdError::DocumentTooShort { length: 10, .. })
    ));
}

#[test]
fn rejects_every_truncation_and_single_byte_corruption() {
    let bytes = valid_container();
    for length in 0..bytes.len() {
        assert!(
            parse_upd(&bytes[..length]).is_err(),
            "truncated to {length}"
        );
    }

    // Deterministic xorshift so the test needs no extra dependency.
    let mut state = 0x2026_1007_u64;
    let mut next = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state
    };
    for _ in 0..5_000 {
        let mut mutated = bytes.clone();
        let index = (next() % bytes.len() as u64) as usize;
        let flip = (next() % 255 + 1) as u8;
        mutated[index] ^= flip;
        // CRC-16 detects every burst of up to 16 bits, and the length header covers the rest,
        // so any single-byte corruption must be refused.
        assert!(
            parse_upd(&mutated).is_err(),
            "byte {index} ^= {flip:#04x} accepted"
        );
    }
}

fn descriptor_error(field: &'static str) -> Result<UpdContainer, UpdError> {
    Err(UpdError::InvalidDescriptor {
        document_index: 0,
        field,
    })
}

fn container_with_descriptor(descriptor: &[u8]) -> Vec<u8> {
    container(&[document_with_descriptor(descriptor, &main_lines())])
}

#[test]
fn rejects_all_space_descriptor_model() {
    let bytes = container_with_descriptor(b"            MAINVer9.99\0       0");

    assert_eq!(parse_upd(&bytes), descriptor_error("model"));
}

#[test]
fn rejects_non_alphanumeric_descriptor_kind() {
    let bytes = container_with_descriptor(b"SYN-100     MA-NVer9.99\0       0");

    assert_eq!(parse_upd(&bytes), descriptor_error("kind"));
}

#[test]
fn rejects_control_byte_in_descriptor_version() {
    let bytes = container_with_descriptor(b"SYN-100     MAINVer9.9\x07\0       0");

    assert_eq!(parse_upd(&bytes), descriptor_error("version"));
}

#[test]
fn rejects_length_header_sum_overflow() {
    let mut bytes = b"9999999999999999999\r\n9999999999999999999\r\n".to_vec();
    bytes.extend_from_slice(&document("MAIN", &main_lines()));

    assert_eq!(parse_upd(&bytes), Err(UpdError::MalformedLengthHeader));
}
