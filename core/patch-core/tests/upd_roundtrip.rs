mod common;

use common::*;
use patch_core::upd::{GAP_FILL, MAX_IMAGE_LEN};
use patch_core::{UpdError, parse_upd, sha256_hex, verify_roundtrip};

/// Length of the decimal document-length header (everything before the first model byte).
fn header_len(bytes: &[u8]) -> usize {
    bytes.iter().position(|&b| b == b'S').expect("model byte")
}

#[test]
fn roundtrip_reproduces_two_document_container() {
    let bytes = valid_container();

    let container = verify_roundtrip(&bytes).expect("byte-identical roundtrip");

    assert_eq!(container.to_bytes().expect("serialize"), bytes);
}

#[test]
fn roundtrip_covers_s1_s3_and_s9_records() {
    let s1_doc = document(
        "MAIN",
        &[
            record(b'0', 2, 0, b"hdr"),
            record(b'1', 2, 0xFFF0, &[0xAB; 16]),
            record(b'9', 2, 0x1234, &[]),
        ],
    );
    let s3_doc = document(
        "PANL",
        &[
            record(b'0', 2, 0, &[]),
            record(b'3', 4, 0x8000_0000, &[0x00, 0xFF]),
            record(b'3', 4, 0x8000_0010, &[0x7F]),
            record(b'7', 4, 0x8000_0000, &[]),
        ],
    );
    let bytes = container(&[s1_doc, s3_doc]);

    verify_roundtrip(&bytes).expect("byte-identical roundtrip");
}

#[test]
fn roundtrip_reproduces_seeded_random_containers() {
    let mut state = 0x5EED_2026_u64;
    let mut next = move |bound: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % bound
    };
    for case in 0..200 {
        let mut documents = Vec::new();
        for _ in 0..=next(3) {
            let mut lines = vec![record(b'0', 2, 0, b"seeded")];
            let mut address = next(0x100) as u32;
            for _ in 0..=next(40) {
                let len = 1 + next(32) as usize;
                let data: Vec<u8> = (0..len).map(|_| next(256) as u8).collect();
                lines.push(record(b'2', 3, address, &data));
                address += len as u32 + next(3) as u32 * 16;
            }
            lines.push(record(b'8', 3, next(0x1000) as u32, &[]));
            documents.push(document("MAIN", &lines));
        }
        let bytes = container(&documents);

        let result = verify_roundtrip(&bytes);

        assert!(result.is_ok(), "case {case}: {result:?}");
    }
}

#[test]
fn image_fills_gaps_and_starts_at_first_record() {
    let bytes = valid_container();
    let container = parse_upd(&bytes).expect("valid container");
    let [main, panel] = container.documents() else {
        panic!("expected two documents");
    };

    let main_image = main.image().expect("main image");
    let panel_image = panel.image().expect("panel image");

    assert_eq!(main_image.base(), 0);
    let mut expected = vec![0x11; 4];
    expected.extend_from_slice(&[0x22; 4]);
    expected.extend_from_slice(&[0xFF; 8]);
    expected.extend_from_slice(&[0x33; 2]);
    assert_eq!(main_image.bytes(), expected.as_slice());
    assert_eq!(panel_image.base(), 0x0C0000);
    assert_eq!(panel_image.bytes(), &[0x44; 3]);
}

fn sparse_s3_container(far: u32) -> Vec<u8> {
    let doc = document(
        "MAIN",
        &[
            record(b'0', 2, 0, &[]),
            record(b'3', 4, 0, &[0x01]),
            record(b'3', 4, far, &[0x02]),
            record(b'7', 4, 0, &[]),
        ],
    );
    container(&[doc])
}

#[test]
fn image_cap_is_64_mib() {
    assert_eq!(MAX_IMAGE_LEN, 64 << 20);
}

#[test]
fn image_accepts_span_exactly_at_limit() {
    let far = u32::try_from(MAX_IMAGE_LEN - 1).expect("limit fits u32");
    let parsed = parse_upd(&sparse_s3_container(far)).expect("structurally valid");

    let image = parsed.documents()[0]
        .image()
        .expect("span at the cap is allowed");

    assert_eq!(image.bytes().len() as u64, MAX_IMAGE_LEN);
    assert_eq!(image.bytes()[0], 0x01);
    assert_eq!(image.bytes()[image.bytes().len() - 1], 0x02);
}

#[test]
fn image_refuses_span_above_limit_but_summary_still_reports() {
    let far = u32::try_from(MAX_IMAGE_LEN).expect("limit fits u32");
    let parsed = parse_upd(&sparse_s3_container(far)).expect("structurally valid");

    let error = parsed.documents()[0].image().expect_err("span too large");
    let summary = parsed.summary();

    assert_eq!(
        error,
        UpdError::ImageTooLarge {
            document_index: 0,
            span: MAX_IMAGE_LEN + 1,
        }
    );
    assert_eq!(summary.documents[0].image_span, MAX_IMAGE_LEN + 1);
    assert_eq!(summary.documents[0].image, None);
    assert_eq!(summary.documents[0].data_records, 2);
}

#[test]
fn summary_reports_image_identity() {
    let bytes = valid_container();
    let container = parse_upd(&bytes).expect("valid container");

    let summary = container.summary();

    let main = summary.documents[0].image.as_ref().expect("main image");
    let main_bytes = container.documents()[0].image().expect("image");
    assert_eq!(main.base, "0x00000000");
    assert_eq!(main.len, 0x12);
    assert_eq!(main.sha256, sha256_hex(main_bytes.bytes()));
    let mut expected = vec![0x11; 4];
    expected.extend_from_slice(&[0x22; 4]);
    expected.extend_from_slice(&[0xFF; 8]);
    expected.extend_from_slice(&[0x33; 2]);
    assert_eq!(main.sha256, sha256_hex(&expected));
    let panel = summary.documents[1].image.as_ref().expect("panel image");
    assert_eq!(panel.base, "0x000C0000");
    assert_eq!(panel.sha256, sha256_hex(&[0x44; 3]));
}

#[test]
fn gap_fill_is_erased_flash_value() {
    assert_eq!(GAP_FILL, 0xFF);
}

#[test]
fn verify_serialized_refuses_bytes_of_a_different_container() {
    let bytes = valid_container();
    let container = parse_upd(&bytes).expect("valid container");
    let other = container_with_main_line(2, &record(b'2', 3, 0x000004, &[0x99; 4]));

    assert_eq!(
        container.verify_serialized(&other),
        Err(UpdError::SerializerSelfCheckFailed)
    );
    assert_eq!(
        container.verify_serialized(&bytes[..bytes.len() - 1]),
        Err(UpdError::SerializerOutputUnparseable(Box::new(
            UpdError::LengthHeaderMismatch {
                declared: (bytes.len() - header_len(&bytes)) as u64,
                available: bytes.len() - 1 - header_len(&bytes),
            }
        )))
    );
    assert_eq!(container.verify_serialized(&bytes), Ok(()));
}
