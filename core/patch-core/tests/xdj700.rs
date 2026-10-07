mod common;

use common::*;
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, MAX_DECODED_LEN, SECTION_TAG, VERIFIED_MAIN_VERSIONS,
    decode_application, decode_main_image, decode_section, is_xdj700, main_document,
    section_checksum,
};
use patch_core::{LzssError, SectionError, parse_upd};

/// `[u32 LE size][stream][u16 LE additive checksum]`, as stored in the MAIN image.
fn section(stream: &[u8]) -> Vec<u8> {
    let mut bytes = (stream.len() as u32).to_le_bytes().to_vec();
    bytes.extend_from_slice(stream);
    let checksum = section_checksum(&bytes);
    bytes.extend_from_slice(&checksum.to_le_bytes());
    bytes
}

/// Section prefix whose first flag byte also marks the next 6 items as literals (bits 2..=7),
/// then a second flag byte with 2 more literals: decodes to 19 zeros followed by 1..=8.
fn literal_stream() -> Vec<u8> {
    vec![0xFD, 0x00, 0xEE, 0xFF, 1, 2, 3, 4, 5, 6, 0x03, 7, 8]
}

fn expected_decoded() -> Vec<u8> {
    let mut decoded = vec![0; 19];
    decoded.extend(1..=8);
    decoded
}

/// An image with `prefix_len` filler bytes followed by `section_bytes`.
fn image_with(prefix_len: usize, section_bytes: &[u8]) -> Vec<u8> {
    let mut image = vec![0xFF; prefix_len];
    image.extend_from_slice(section_bytes);
    image
}

/// A one-document XDJ-700 MAIN Ver1.15 container whose image starts at `base`.
fn xdj700_container(image: &[u8], base: u32) -> Vec<u8> {
    xdj700_container_version(image, base, "Ver1.15")
}

fn xdj700_container_version(image: &[u8], base: u32, version: &str) -> Vec<u8> {
    let mut lines = vec![record(b'0', 2, 0, b"synthetic")];
    for (index, chunk) in image.chunks(32).enumerate() {
        lines.push(record(b'2', 3, base + (index * 32) as u32, chunk));
    }
    lines.push(record(b'7', 4, 0, &[]));
    let mut descriptor = format!("XDJ-700     MAIN{version}").into_bytes();
    descriptor.extend_from_slice(b"\0       0");
    container(&[document_with_descriptor(&descriptor, &lines)])
}

#[test]
fn decodes_verified_section() {
    let image = image_with(16, &section(&literal_stream()));

    let decoded = decode_section(&image, 16).expect("valid section");

    assert_eq!(decoded.offset(), 16);
    assert_eq!(decoded.compressed_len(), literal_stream().len());
    assert_eq!(decoded.decoded(), expected_decoded().as_slice());
    assert_eq!(decoded.decoded_sha256().len(), 64);
}

#[test]
fn rejects_checksum_mismatch() {
    let mut bytes = section(&literal_stream());
    let last = bytes.len() - 1;
    bytes[last] ^= 0x01;

    assert!(matches!(
        decode_section(&bytes, 0),
        Err(SectionError::ChecksumMismatch { .. })
    ));
}

#[test]
fn rejects_corrupted_stream_byte_via_checksum() {
    let mut bytes = section(&literal_stream());
    bytes[10] ^= 0x40;

    assert!(matches!(
        decode_section(&bytes, 0),
        Err(SectionError::ChecksumMismatch { .. })
    ));
}

#[test]
fn accepts_stock_tag_bytes_exactly() {
    let mut stream = SECTION_TAG.to_vec();
    stream.extend_from_slice(&[0x00, 0x0F].repeat(6));

    let decoded = decode_section(&section(&stream), 0).expect("stock prefix");

    assert_eq!(decoded.decoded().len(), 19 + 6 * 18);
}

#[test]
fn rejects_missing_tag() {
    for stream in [
        vec![0xFF, 1, 2, 3, 4, 5, 6, 7, 8],
        vec![0x03, 0x00, 0xEE, 0xFF],
        vec![0x01, 0x01, 0xEE, 0xFF],
        vec![0x01, 0x00, 0xEE, 0xFE],
        vec![0x01, 0x00],
        vec![0x00, 0x00, 0xEE, 0xFF],
        vec![0xFC, 0x00, 0xEE, 0xFF],
    ] {
        assert_eq!(
            decode_section(&section(&stream), 0),
            Err(SectionError::MissingTag),
            "stream {stream:02X?}"
        );
    }
}

#[test]
fn rejects_size_field_outside_image() {
    assert_eq!(
        decode_section(&[0, 0, 0], 0),
        Err(SectionError::SizeFieldOutOfBounds { offset: 0 })
    );
    assert_eq!(
        decode_section(&[0; 8], usize::MAX - 1),
        Err(SectionError::SizeFieldOutOfBounds {
            offset: usize::MAX - 1
        })
    );
}

#[test]
fn rejects_declared_size_beyond_image() {
    let mut bytes = section(&literal_stream());
    bytes[..4].copy_from_slice(&u32::MAX.to_le_bytes());

    assert_eq!(
        decode_section(&bytes, 0),
        Err(SectionError::DataOutOfBounds {
            offset: 0,
            declared_len: u32::MAX
        })
    );
}

#[test]
fn rejects_missing_checksum_bytes() {
    let mut bytes = section(&literal_stream());
    bytes.truncate(bytes.len() - 1);

    assert_eq!(
        decode_section(&bytes, 0),
        Err(SectionError::ChecksumOutOfBounds { offset: 0 })
    );
}

#[test]
fn propagates_decode_errors() {
    // Item 2 under the prefix flag byte is a match; its second byte is missing.
    let mut stream = SECTION_TAG.to_vec();
    stream.push(0x00);

    assert_eq!(
        decode_section(&section(&stream), 0),
        Err(SectionError::Decode(LzssError::TruncatedMatch {
            input_offset: 4
        }))
    );
}

#[test]
fn decoded_output_is_capped() {
    // Each 2-byte match expands to 18 bytes; enough groups to pass the cap.
    let groups = MAX_DECODED_LEN / (8 * 18) + 1;
    let mut stream = SECTION_TAG.to_vec();
    stream.extend_from_slice(&[0x00, 0x0F].repeat(6));
    for _ in 0..groups {
        stream.push(0x00);
        stream.extend_from_slice(&[0x00, 0x0F].repeat(8));
    }

    assert_eq!(
        decode_section(&section(&stream), 0),
        Err(SectionError::Decode(LzssError::OutputLimitExceeded {
            limit: MAX_DECODED_LEN
        }))
    );
}

#[test]
fn decodes_application_from_xdj700_container() {
    let image = image_with(APPLICATION_SECTION_OFFSET, &section(&literal_stream()));
    let parsed = parse_upd(&xdj700_container(&image, 0)).expect("valid container");

    let decoded = decode_application(&parsed).expect("application section");

    assert!(is_xdj700(&parsed));
    assert_eq!(decoded.offset(), APPLICATION_SECTION_OFFSET);
    assert_eq!(decoded.decoded(), expected_decoded().as_slice());
}

#[test]
fn application_requires_xdj700_main_document() {
    let parsed = parse_upd(&valid_container()).expect("valid container");

    assert!(!is_xdj700(&parsed));
    assert_eq!(
        decode_application(&parsed),
        Err(SectionError::NoMainDocument)
    );
}

#[test]
fn application_requires_image_based_at_zero() {
    let image = image_with(APPLICATION_SECTION_OFFSET, &section(&literal_stream()));
    let parsed = parse_upd(&xdj700_container(&image, 0x100)).expect("valid container");

    assert_eq!(
        decode_application(&parsed),
        Err(SectionError::ImageBase { base: 0x100 })
    );
}

#[test]
fn application_requires_verified_main_version() {
    let image = image_with(APPLICATION_SECTION_OFFSET, &section(&literal_stream()));
    let parsed =
        parse_upd(&xdj700_container_version(&image, 0, "Ver1.16")).expect("valid container");

    assert!(is_xdj700(&parsed));
    assert_eq!(VERIFIED_MAIN_VERSIONS, ["Ver1.15"]);
    assert_eq!(
        decode_application(&parsed),
        Err(SectionError::UnverifiedVersion {
            version: "Ver1.16".to_owned()
        })
    );
}

#[test]
fn section_checksum_is_wrapping_16_bit_sum() {
    assert_eq!(section_checksum(&[]), 0);
    assert_eq!(section_checksum(&[0xFF; 3]), 0x02FD);
    // 0xFF * 0x102 = 0x100FE, which wraps to 0x00FE.
    assert_eq!(section_checksum(&[0xFF; 0x102]), 0x00FE);
}

#[test]
fn refuses_more_than_one_xdj700_main_document() {
    let image = image_with(APPLICATION_SECTION_OFFSET, &section(&literal_stream()));
    let one = xdj700_container(&image, 0);
    let parsed_one = parse_upd(&one).expect("valid container");
    let main_doc = &one[one.iter().position(|&b| b == b'X').expect("model")..];
    let bytes = container(&[main_doc.to_vec(), main_doc.to_vec()]);
    let parsed = parse_upd(&bytes).expect("valid container");

    assert!(main_document(&parsed_one).is_ok());
    assert!(is_xdj700(&parsed));
    assert_eq!(
        main_document(&parsed).map(|_| ()),
        Err(SectionError::AmbiguousMainDocument { count: 2 })
    );
    assert_eq!(
        decode_application(&parsed),
        Err(SectionError::AmbiguousMainDocument { count: 2 })
    );
}

#[test]
fn decode_main_image_uses_a_prebuilt_image() {
    let image = image_with(APPLICATION_SECTION_OFFSET, &section(&literal_stream()));
    let parsed = parse_upd(&xdj700_container(&image, 0)).expect("valid container");
    let main = main_document(&parsed).expect("main");
    let built = main.image().expect("image");

    let decoded = decode_main_image(main, &built).expect("decode");

    assert_eq!(Ok(decoded), decode_application(&parsed));
}

#[test]
fn encode_section_roundtrips_through_decode_section() {
    let mut decoded = vec![0; 19];
    decoded.extend(b"application bytes ".repeat(50));

    let bytes = patch_core::xdj700::encode_section(&decoded).expect("encode");
    let section = decode_section(&bytes, 0).expect("decode");

    assert_eq!(section.decoded(), decoded.as_slice());
    let data_end = bytes.len() - 2;
    assert_eq!(
        u16::from_le_bytes([bytes[data_end], bytes[data_end + 1]]),
        section_checksum(&bytes[..data_end])
    );
    assert_eq!(
        u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize,
        bytes.len() - 6
    );
}

#[test]
fn encode_section_refuses_data_without_seed() {
    assert_eq!(
        patch_core::xdj700::encode_section(b"no zero prefix"),
        Err(SectionError::Encode(LzssError::MissingSectionSeed))
    );
}
