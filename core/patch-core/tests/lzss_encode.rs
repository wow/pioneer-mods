//! Encoder tests. Golden vectors were produced by the reference encoder (README, Acknowledgements);
//! matching them byte-for-byte keeps our encoder decision-identical to that reference.

use patch_core::lzss::{decode, encode, encode_section_stream};
use patch_core::{LzssError, sha256_hex};

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

#[test]
fn reference_vectors_pin_history_insertion_and_probe_order() {
    // `-2` is indexed before `-1`; swapping them changes which history source wins.
    assert_eq!(encode(&unhex("20202000202020")), Ok(unhex("02ebf000edf0")));
    // Distance 1 is probed before distance 2; the first strictly longer match wins.
    assert_eq!(
        encode(&unhex("4141204141414141")),
        Ok(unhex("034141edf0f2f0"))
    );
}

/// A 40-byte block repeated at distance exactly 4096 (the window edge).
fn window_edge_input() -> Vec<u8> {
    let block: Vec<u8> = (0..40u32).map(|i| ((i * 37 + 11) & 0xFF) as u8).collect();
    let mut data = block.clone();
    data.extend((0..4096 - 40u32).map(|i| ((i * i * 7 + i * 13 + 5) & 0xFF) as u8));
    data.extend(block);
    data
}

/// 6000 bytes with the same 3-byte key every 4 bytes (dense candidate lists).
fn dense_key_input() -> Vec<u8> {
    (0..1500u32)
        .flat_map(|i| [b'a', b'b', b'c', b"xyz"[((i * i + 3 * i) % 3) as usize]])
        .collect()
}

/// A long match that is older than 1300 newer same-key candidates (each matching 3 bytes), all
/// inside the window, so any candidate cap of 1300 or less changes the chosen match.
fn cap_binding_input() -> Vec<u8> {
    let block: Vec<u8> = b"abc".iter().copied().chain(0xA0..0xA0 + 18).collect();
    let mut data = block.clone();
    data.extend(b"abc".repeat(1300));
    // Separator: stops a period-3 match running into the block, forcing a fresh search there.
    data.extend([0x01, 0x02]);
    data.extend(block);
    data
}

/// Starts with three spaces, so history sources `-2`/`-1` share its key's bucket; the repeat at
/// distance 4096 is reachable only by scanning past those (then out-of-window) history entries.
fn history_break_input() -> Vec<u8> {
    let unique: Vec<u8> = (0xC0..0xC0 + 17).collect();
    let filler = (0..4096 - 20u32).map(|i| match ((i * i * 7 + i * 13 + 5) & 0xFF) as u8 {
        0x20 => 0x21,
        byte => byte,
    });
    let mut data = b"   ".to_vec();
    data.extend(&unique);
    data.extend(filler);
    data.extend(b"   ");
    data.extend(unique);
    data
}

#[test]
fn reference_vectors_pin_window_edge_and_dense_candidates() {
    // Expected streams come from the reference encoder; pinned by length and SHA-256.
    let cases = [
        (
            window_edge_input(),
            "ed105fe9b812be12cd79c4427bf6c412cf5765f6a57561a8037452a5b1eabbf3",
            790,
            "38b0fde92e00cec219876106cb81985852650397eb057785fc9e71cf65411c99",
        ),
        (
            dense_key_input(),
            "966c4de7a220a1cc54d72a16b622f3a5ef68cb36c19a214c181090930f2d0703",
            718,
            "4b71deefd511641f8aadd7db558ad3963318206c53bb836aec517d541e435891",
        ),
        (
            cap_binding_input(),
            "bcb603883319b4a5123ab0c4a94953d23517385ee49d8f2b4adfa2959d046ca9",
            494,
            "23bfe931d726984700b912249ce8619fe150498528197b39f1185188720dd0e1",
        ),
        (
            history_break_input(),
            "b533763870124e069f72b47aa570cb2d7490dc9d5c3eb84fa029d3d76ad9a272",
            767,
            "1925eec9d335a40e0e0da106a977c1d1df5d74f5178e419e38f4124d76202a03",
        ),
    ];
    for (input, input_sha, stream_len, stream_sha) in cases {
        assert_eq!(sha256_hex(&input), input_sha, "fixture input drifted");
        let stream = encode(&input).expect("encode");
        assert_eq!(stream.len(), stream_len);
        assert_eq!(sha256_hex(&stream), stream_sha);
    }
}
