//! Deterministic greedy encoder for the XDJ-700 LZSS variant.
//!
//! The match search reproduces DeckVolve's `lzss_pioneer.py` encoder decision for decision, so
//! both produce byte-identical streams for the same input:
//!
//! - candidates are indexed by their next 3 bytes, in insertion order; the ring's initial spaces
//!   are pre-indexed as logical sources `-4096..=-3`, and sources `-2` and `-1` (which straddle
//!   the start of the data) are indexed once, the first time they are fully known;
//! - at each position the newest 4096 candidates for the current 3-byte key are tried, newest
//!   first, keeping the first strictly longer match and stopping at the maximum length;
//! - distances 1 and 2 are then tried, since overlapping runs have no non-overlapping key;
//! - a match of at least 3 bytes is emitted as an absolute ring position, otherwise a literal.
//!
//! This is not claimed to be the stock packer's heuristic; stock sections compress differently.

use super::{INITIAL_WRITE_INDEX, MAX_MATCH, MIN_MATCH, RING_FILL, WINDOW_SIZE};
use crate::error::LzssError;
use std::collections::HashMap;

const WINDOW_MASK: usize = WINDOW_SIZE - 1;
const MAX_CANDIDATES: usize = 4096;
/// Stock section tag, consumed as stream data: a literal `0x00` and an 18-byte match of it.
const SECTION_TAG: [u8; 4] = [0x01, 0x00, 0xEE, 0xFF];
/// Decoded bytes produced by [`SECTION_TAG`].
const SECTION_SEED_LEN: usize = 19;

/// Encodes `data` as a standalone stream (empty ring history).
///
/// # Errors
///
/// [`LzssError::InputTooLarge`] above `i32::MAX` bytes, or [`LzssError::EncoderSelfCheck`] if a
/// chosen match would not reproduce the input (an internal defect).
pub fn encode(data: &[u8]) -> Result<Vec<u8>, LzssError> {
    let mut state = State::new();
    let mut stream = Vec::new();
    state.encode(data, 0, &mut stream, None, 0)?;
    Ok(stream)
}

/// Encodes a firmware section: the tag followed by the stream for `data[19..]`, continuing the
/// tag's flag byte, exactly as the device decoder will consume it.
///
/// # Errors
///
/// [`LzssError::MissingSectionSeed`] unless `data` starts with the 19 zero bytes the tag decodes
/// to, plus the errors of [`encode`].
pub fn encode_section_stream(data: &[u8]) -> Result<Vec<u8>, LzssError> {
    if data.len() < SECTION_SEED_LEN || data[..SECTION_SEED_LEN].iter().any(|&b| b != 0) {
        return Err(LzssError::MissingSectionSeed);
    }
    let mut state = State::new();
    // Replay the tag: one literal, then an 18-byte match of the ring position just written.
    state.write(SECTION_TAG[1]);
    let position = usize::from(SECTION_TAG[2]) | (usize::from(SECTION_TAG[3] >> 4) << 8);
    for step in 0..usize::from(SECTION_TAG[3] & 0x0F) + MIN_MATCH {
        let byte = state.ring[(position + step) & WINDOW_MASK];
        state.write(byte);
    }

    let mut stream = SECTION_TAG.to_vec();
    state.encode(data, SECTION_SEED_LEN, &mut stream, Some(0), 2)?;
    Ok(stream)
}

struct State {
    ring: [u8; WINDOW_SIZE],
    write_index: usize,
}

impl State {
    fn new() -> Self {
        Self {
            ring: [RING_FILL; WINDOW_SIZE],
            write_index: INITIAL_WRITE_INDEX,
        }
    }

    fn write(&mut self, byte: u8) {
        self.ring[self.write_index & WINDOW_MASK] = byte;
        self.write_index += 1;
    }

    /// Encodes `data[start..]` into `stream`, continuing flag byte `flag_offset` at `bit`.
    fn encode(
        &mut self,
        data: &[u8],
        start: usize,
        stream: &mut Vec<u8>,
        mut flag_offset: Option<usize>,
        mut bit: u32,
    ) -> Result<(), LzssError> {
        if i32::try_from(data.len()).is_err() {
            return Err(LzssError::InputTooLarge { len: data.len() });
        }
        let mut index = CandidateIndex::new(data);
        let mut current = start;
        while current < data.len() {
            let flags_at = *flag_offset.get_or_insert_with(|| {
                stream.push(0);
                bit = 0;
                stream.len() - 1
            });

            let (length, source) = index.best_match(current as i64);
            if length >= MIN_MATCH {
                let position = (INITIAL_WRITE_INDEX as i64 + source).rem_euclid(WINDOW_SIZE as i64);
                let position = position as usize;
                stream.push((position & 0xFF) as u8);
                stream.push((((position >> 8) as u8) << 4) | (length - MIN_MATCH) as u8);
                for step in 0..length {
                    let byte = self.ring[(position + step) & WINDOW_MASK];
                    if byte != data[current + step] {
                        return Err(LzssError::EncoderSelfCheck { position: current });
                    }
                    self.write(byte);
                }
                current += length;
            } else {
                stream[flags_at] |= 1 << bit;
                stream.push(data[current]);
                self.write(data[current]);
                current += 1;
            }

            bit += 1;
            if bit == 8 {
                flag_offset = None;
            }
        }
        Ok(())
    }
}

/// 3-byte key → logical source positions, in insertion order.
struct CandidateIndex<'a> {
    data: &'a [u8],
    buckets: HashMap<[u8; 3], Vec<i32>>,
    indexed_until: usize,
    history_indexed: [bool; 2],
}

impl<'a> CandidateIndex<'a> {
    fn new(data: &'a [u8]) -> Self {
        let mut buckets = HashMap::new();
        buckets.insert([RING_FILL; 3], (-(WINDOW_SIZE as i32)..-2).collect());
        Self {
            data,
            buckets,
            indexed_until: 0,
            history_indexed: [false; 2],
        }
    }

    /// The byte a match from `source` produces at `offset`, as the device's ring copy would.
    fn match_byte(&self, current: i64, source: i64, offset: i64) -> u8 {
        let logical = source + offset;
        if logical < current {
            if logical < 0 {
                RING_FILL
            } else {
                self.data[logical as usize]
            }
        } else {
            let distance = current - source;
            self.data[(current + offset - distance) as usize]
        }
    }

    fn match_length(&self, current: i64, source: i64) -> usize {
        let distance = current - source;
        if !(1..=WINDOW_SIZE as i64).contains(&distance) {
            return 0;
        }
        let limit = MAX_MATCH.min(self.data.len() - current as usize);
        (0..limit)
            .take_while(|&length| {
                self.match_byte(current, source, length as i64)
                    == self.data[current as usize + length]
            })
            .count()
    }

    fn index_up_to(&mut self, limit: i64) {
        while (self.indexed_until as i64) + 2 < limit {
            let source = self.indexed_until;
            let key = [
                self.data[source],
                self.data[source + 1],
                self.data[source + 2],
            ];
            self.buckets.entry(key).or_default().push(source as i32);
            self.indexed_until += 1;
        }
        for (slot, source) in [(0, -2_i64), (1, -1_i64)] {
            if self.history_indexed[slot] || source + 2 >= limit {
                continue;
            }
            let key = [
                self.match_byte(limit, source, 0),
                self.match_byte(limit, source, 1),
                self.match_byte(limit, source, 2),
            ];
            self.buckets.entry(key).or_default().push(source as i32);
            self.history_indexed[slot] = true;
        }
    }

    fn best_match(&mut self, current: i64) -> (usize, i64) {
        self.index_up_to(current);
        let position = current as usize;
        if position + MIN_MATCH > self.data.len() {
            return (0, 0);
        }
        let key = [
            self.data[position],
            self.data[position + 1],
            self.data[position + 2],
        ];
        let lower_bound = current - WINDOW_SIZE as i64;
        let mut best = (0, 0);
        if let Some(candidates) = self.buckets.get(&key) {
            for &source in candidates.iter().rev().take(MAX_CANDIDATES) {
                let source = i64::from(source);
                if source < lower_bound || source >= current {
                    continue;
                }
                let length = self.match_length(current, source);
                if length > best.0 {
                    best = (length, source);
                    if length == MAX_MATCH {
                        break;
                    }
                }
            }
        }
        for distance in [1, 2] {
            let source = current - distance;
            if source < lower_bound {
                continue;
            }
            let length = self.match_length(current, source);
            if length > best.0 {
                best = (length, source);
            }
        }
        best
    }
}
