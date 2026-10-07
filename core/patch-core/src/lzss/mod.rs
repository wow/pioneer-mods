//! Decoder for the LZSS variant used by Pioneer XDJ-700 MAIN firmware sections.
//!
//! Semantics follow the device decompressor as documented by the reference implementation (see the
//! README's Acknowledgements):
//!
//! - a 4096-byte ring buffer pre-filled with `0x20`, first write at index `4096 - 18`;
//! - each flag byte describes the next 8 items, least significant bit first;
//! - flag bit `1` is a literal byte, `0` a two-byte match `b1 b2`;
//! - a match copies `(b2 & 0x0F) + 3` bytes (3..=18) from the **absolute** ring position
//!   `b1 | (b2 & 0xF0) << 4`, writing each byte back into the ring as it is produced, so
//!   overlapping matches repeat recent output.
//!
//! Decoding stops when the input is exhausted. The decoder is pure and bounds its output.

use crate::error::LzssError;

mod encode;

pub(crate) use encode::{SECTION_SEED_LEN, encode_section_stream_into};
pub use encode::{encode, encode_section_stream};

pub const WINDOW_SIZE: usize = 4096;
const WINDOW_MASK: usize = WINDOW_SIZE - 1;
pub const MIN_MATCH: usize = 3;
pub const MAX_MATCH: usize = 18;
pub const INITIAL_WRITE_INDEX: usize = WINDOW_SIZE - MAX_MATCH;
pub const RING_FILL: u8 = 0x20;

/// Start of the stock XDJ-700 section stream (`01 00 EE FF`). The device decodes it as data: a
/// literal `0x00` and an 18-byte match of it, i.e. a 19-byte zero prefix.
///
/// The first byte is a flag byte covering 8 items. Only its two low bits belong to the tag
/// (literal, then match); bits 2..=7 describe the data that follows, so a re-encoded section may
/// start with a different first byte.
pub const SECTION_TAG: [u8; 4] = [0x01, 0x00, 0xEE, 0xFF];

/// Decodes `stream`, refusing to produce more than `max_output` bytes.
///
/// # Errors
///
/// [`LzssError::TruncatedMatch`] if the stream ends inside a match token, or
/// [`LzssError::OutputLimitExceeded`] if the output would exceed `max_output`.
pub fn decode(stream: &[u8], max_output: usize) -> Result<Vec<u8>, LzssError> {
    let mut ring = [RING_FILL; WINDOW_SIZE];
    let mut write_index = INITIAL_WRITE_INDEX;
    // Stock sections expand about 2.7x; reserving 3x avoids a mid-decode reallocation.
    let mut output = Vec::with_capacity(stream.len().saturating_mul(3).min(max_output));
    let mut input = stream.iter().copied().enumerate();

    while let Some((_, flags)) = input.next() {
        for bit in 0..8 {
            let Some((offset, first)) = input.next() else {
                return Ok(output);
            };
            if flags >> bit & 1 == 1 {
                if output.len() == max_output {
                    return Err(LzssError::OutputLimitExceeded { limit: max_output });
                }
                output.push(first);
                ring[write_index & WINDOW_MASK] = first;
                write_index += 1;
                continue;
            }

            let Some((_, second)) = input.next() else {
                return Err(LzssError::TruncatedMatch {
                    input_offset: offset,
                });
            };
            let position = usize::from(first) | (usize::from(second & 0xF0) << 4);
            let length = usize::from(second & 0x0F) + MIN_MATCH;
            if output.len() + length > max_output {
                return Err(LzssError::OutputLimitExceeded { limit: max_output });
            }
            for step in 0..length {
                let byte = ring[(position + step) & WINDOW_MASK];
                output.push(byte);
                ring[write_index & WINDOW_MASK] = byte;
                write_index += 1;
            }
        }
    }
    Ok(output)
}
