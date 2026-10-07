//! Encoder tests. Golden vectors were produced by DeckVolve's `lzss_pioneer.py` encoder (MIT);
//! matching them byte-for-byte keeps our encoder decision-identical to that reference.

use patch_core::LzssError;
use patch_core::lzss::{decode, encode, encode_section_stream};

fn unhex(text: &str) -> Vec<u8> {
    (0..text.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&text[i..i + 2], 16).expect("hex"))
        .collect()
}

fn seeded(tail: &[u8]) -> Vec<u8> {
    let mut data = vec![0; 19];
    data.extend_from_slice(tail);
    data
}

#[test]
fn plain_encoding_matches_reference_golden_vectors() {
    let cases: [(&[u8], &str); 5] = [
        (b"abcabcabcabc", "07616263eef6"),
        (&[0; 20], "0500eeff00"),
        (
            b"  hello  hello world  hello",
            "7f202068656c6c6feef51f776f726c64f5f4",
        ),
        (
            &[
                0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA,
                0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0xAA, 0x01, 0x02,
            ],
            "19aaeefffef30102",
        ),
        (
            &(0..40).collect::<Vec<u8>>(),
            "ff0001020304050607ff08090a0b0c0d0e0fff1011121314151617ff18191a1b1c1d1e1fff2021222324252627",
        ),
    ];
    for (data, expected) in cases {
        assert_eq!(encode(data), Ok(unhex(expected)), "input {data:02X?}");
    }
}

#[test]
fn section_encoding_matches_reference_golden_vectors() {
    let cases: [(&[u8], &str); 3] = [
        (b"", "0100eeff"),
        (b"abcabcabcabc", "1d00eeff6162630106"),
        (
            b"XY\x00\x00\x00\x00ZZZZZZZZZZ  q",
            "ad00eeff5859fdf15a070620032071",
        ),
    ];
    for (tail, expected) in cases {
        assert_eq!(
            encode_section_stream(&seeded(tail)),
            Ok(unhex(expected)),
            "tail {tail:02X?}"
        );
    }
}

#[test]
fn section_stream_continues_the_tag_flag_byte() {
    // One literal after the seed sets flag bit 2 of the tag's flag byte (0x01 | 0x04).
    let stream = encode_section_stream(&seeded(b"Q")).expect("encode");

    assert_eq!(stream, vec![0x05, 0x00, 0xEE, 0xFF, b'Q']);
}

#[test]
fn encode_then_decode_reproduces_input() {
    let mut state = 0x0DEC_0DE5_u64;
    let mut next = move |bound: u64| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state % bound
    };
    for case in 0..300 {
        let len = next(6000) as usize;
        let alphabet = 1 + next(255);
        let data: Vec<u8> = (0..len).map(|_| next(alphabet) as u8).collect();

        let plain = encode(&data).expect("plain encode");
        let section = encode_section_stream(&seeded(&data)).expect("section encode");

        assert_eq!(
            decode(&plain, usize::MAX),
            Ok(data.clone()),
            "plain case {case}"
        );
        assert_eq!(
            decode(&section, usize::MAX),
            Ok(seeded(&data)),
            "section case {case}"
        );
    }
}

#[test]
fn long_runs_and_window_edges_roundtrip() {
    let mut data = vec![0x20; 5000];
    data.extend((0..=255u8).cycle().take(9000));
    data.extend(vec![0x00; 4097]);

    let stream = encode(&data).expect("encode");

    assert_eq!(decode(&stream, usize::MAX), Ok(data.clone()));
    assert!(stream.len() < data.len() / 4, "runs must compress");
}

#[test]
fn empty_input_encodes_to_empty_stream() {
    assert_eq!(encode(&[]), Ok(vec![]));
}

#[test]
fn section_requires_zero_seed() {
    assert_eq!(
        encode_section_stream(&[0; 18]),
        Err(LzssError::MissingSectionSeed)
    );
    let mut data = vec![0; 19];
    data[18] = 1;
    assert_eq!(
        encode_section_stream(&data),
        Err(LzssError::MissingSectionSeed)
    );
}

/// Inputs chosen to pin specific search decisions; expected streams come from the reference.
#[test]
fn reference_vectors_pin_search_decisions() {
    let cases = [
        // Period-2 run: only the distance-2 probe finds the match (nothing is indexed yet).
        ("6162616261626162616261626162616261626162", "036162eeff"),
        // A match starting in the space-filled history before the data (sources -2/-1).
        ("51202051202051", "0151ecf3"),
        // The longest match is older than the 16 newest candidates with the same 3-byte key,
        // so a smaller candidate cap picks a different (shorter) match.
        (
            "6465666768696a6b6c6d6e6f707172737475767778797a7b7c7d7e7f8081646566c8646566c9646566ca\
             646566cb646566cc646566cd646566ce646566cf646566d0646566d1646566d2646566d3646566d4\
             646566d5646566d6646566d7646566d8646566d9646566da646566db6465666768696a6b6c6d6e6f\
             707172737475767778797a7b7c7d7e7f8081",
            "ff6465666768696a6bff6c6d6e6f70717273ff7475767778797a7bbf7c7d7e7f8081eef0c8aa0c00c9\
             1000ca1400cb1800ccaa1c00cd2000ce2400cf2800d0aa2c00d13000d23400d33800d4aa3c00d54000\
             d64400d74800d82a4c00d95000da5400dbeeff0009",
        ),
    ];
    for (input, expected) in cases {
        let input = unhex(&input.replace(' ', ""));
        assert_eq!(encode(&input), Ok(unhex(&expected.replace(' ', ""))));
    }
}
