use patch_core::LzssError;
use patch_core::lzss::{INITIAL_WRITE_INDEX, RING_FILL, decode};
use patch_core::xdj700::SECTION_TAG;

const LIMIT: usize = 1 << 20;

/// Match token copying `length` bytes from absolute ring `position`.
fn token(position: usize, length: usize) -> [u8; 2] {
    [
        (position & 0xFF) as u8,
        (((position >> 8) as u8) << 4) | (length - 3) as u8,
    ]
}

#[test]
fn decodes_literals_least_significant_flag_bit_first() {
    let stream = [0xFF, 1, 2, 3, 4, 5, 6, 7, 8, 0x03, 9, 10];

    assert_eq!(decode(&stream, LIMIT), Ok((1..=10).collect::<Vec<u8>>()));
}

#[test]
fn match_reads_space_filled_ring_before_any_output() {
    let mut stream = vec![0x00];
    stream.extend_from_slice(&token(0x000, 3));

    // The device pre-fills the ring with ASCII spaces.
    assert_eq!(decode(&stream, LIMIT), Ok(vec![0x20; 3]));
    assert_eq!(RING_FILL, 0x20);
}

#[test]
fn overlapping_match_repeats_bytes_written_during_the_copy() {
    let mut stream = vec![0b0000_0001, b'A'];
    stream.extend_from_slice(&token(INITIAL_WRITE_INDEX, 5));

    assert_eq!(decode(&stream, LIMIT), Ok(b"AAAAAA".to_vec()));
}

#[test]
fn match_uses_high_position_nibble_from_second_byte() {
    // Literals land at ring INITIAL_WRITE_INDEX (0xFEE) onward; 0xFEF holds the second literal.
    let mut stream = vec![0b0000_0011, b'x', b'y'];
    stream.extend_from_slice(&token(INITIAL_WRITE_INDEX + 1, 3));

    // Copies ring[0xFEF] ('y'), then ring[0xFF0] and ring[0xFF1], which this match itself just
    // wrote. Ignoring the high nibble would read ring[0x0EF] (spaces) instead.
    assert_eq!(decode(&stream, LIMIT), Ok(b"xyyyy".to_vec()));
}

#[test]
fn ring_write_index_wraps_to_zero() {
    // 18 literals fill 0xFEE..=0xFFF; the 19th lands at ring index 0.
    let mut stream = vec![
        0xFF, 1, 2, 3, 4, 5, 6, 7, 8, 0xFF, 9, 10, 11, 12, 13, 14, 15, 16,
    ];
    stream.extend_from_slice(&[0b0000_0111, 17, 18, 99]);
    stream.extend_from_slice(&token(0x000, 3));

    let decoded = decode(&stream, LIMIT).expect("decode");

    assert_eq!(&decoded[18..], &[99, 99, 99, 99]);
}

#[test]
fn section_tag_decodes_to_nineteen_zero_bytes() {
    assert_eq!(decode(&SECTION_TAG, LIMIT), Ok(vec![0; 19]));
}

#[test]
fn trailing_flag_byte_without_items_is_accepted() {
    // A full group of 8 literals followed by a flag byte with no items after it.
    let stream = [0xFF, 1, 2, 3, 4, 5, 6, 7, 8, 0xFF];
    assert_eq!(decode(&stream, LIMIT), Ok((1..=8).collect::<Vec<u8>>()));
    // A group that ends early, after one of its 8 items.
    assert_eq!(decode(&[0xFF, 1], LIMIT), Ok(vec![1]));
    assert_eq!(decode(&[], LIMIT), Ok(vec![]));
}

#[test]
fn rejects_match_truncated_after_first_byte() {
    assert_eq!(
        decode(&[0x01, 7, 0x12], LIMIT),
        Err(LzssError::TruncatedMatch { input_offset: 2 })
    );
}

#[test]
fn output_limit_is_inclusive_and_enforced_for_literals_and_matches() {
    let literals = [0xFF, 1, 2, 3, 4, 5, 6, 7, 8];
    assert_eq!(decode(&literals, 8).map(|out| out.len()), Ok(8));
    assert_eq!(
        decode(&literals, 7),
        Err(LzssError::OutputLimitExceeded { limit: 7 })
    );

    let mut matched = vec![0x00];
    matched.extend_from_slice(&token(0, 18));
    assert_eq!(decode(&matched, 18).map(|out| out.len()), Ok(18));
    assert_eq!(
        decode(&matched, 17),
        Err(LzssError::OutputLimitExceeded { limit: 17 })
    );
}
