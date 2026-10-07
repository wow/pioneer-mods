//! Motorola S-record line parsing (one CRLF-stripped line at a time).

use crate::error::RecordDefect;
use serde::Serialize;
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum SRecordType {
    S0,
    S1,
    S2,
    S3,
    S5,
    S6,
    S7,
    S8,
    S9,
}

impl SRecordType {
    fn from_digit(digit: u8) -> Option<Self> {
        Some(match digit {
            b'0' => Self::S0,
            b'1' => Self::S1,
            b'2' => Self::S2,
            b'3' => Self::S3,
            b'5' => Self::S5,
            b'6' => Self::S6,
            b'7' => Self::S7,
            b'8' => Self::S8,
            b'9' => Self::S9,
            _ => return None,
        })
    }

    /// Width of the address field in bytes.
    pub fn address_len(self) -> usize {
        match self {
            Self::S0 | Self::S1 | Self::S5 | Self::S9 => 2,
            Self::S2 | Self::S6 | Self::S8 => 3,
            Self::S3 | Self::S7 => 4,
        }
    }

    pub fn is_data(self) -> bool {
        matches!(self, Self::S1 | Self::S2 | Self::S3)
    }

    pub fn is_termination(self) -> bool {
        matches!(self, Self::S7 | Self::S8 | Self::S9)
    }
}

impl fmt::Display for SRecordType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Debug::fmt(self, f)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SRecord {
    record_type: SRecordType,
    address: u32,
    data: Vec<u8>,
}

impl SRecord {
    pub fn record_type(&self) -> SRecordType {
        self.record_type
    }

    pub fn address(&self) -> u32 {
        self.address
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Exclusive end address of this record's data, computed without overflow.
    pub fn end_address(&self) -> u64 {
        u64::from(self.address) + self.data.len() as u64
    }
}

/// Parses one S-record line without its CRLF terminator.
///
/// Fail-closed: only uppercase hex is accepted, the byte count must match the line length, the
/// checksum must be valid, and data must fit in the record type's address space.
pub(crate) fn parse_record(line: &[u8]) -> Result<SRecord, RecordDefect> {
    let [b'S', type_digit, hex @ ..] = line else {
        return Err(RecordDefect::Syntax);
    };
    let record_type = SRecordType::from_digit(*type_digit).ok_or(RecordDefect::UnsupportedType)?;
    if !hex
        .iter()
        .all(|byte| matches!(byte, b'0'..=b'9' | b'A'..=b'F'))
    {
        return Err(RecordDefect::Syntax);
    }
    let mut raw = hex::decode(hex).map_err(|_| RecordDefect::Syntax)?;

    let (&count, rest) = raw.split_first().ok_or(RecordDefect::Syntax)?;
    if usize::from(count) != rest.len() {
        return Err(RecordDefect::ByteCountMismatch);
    }
    let address_len = record_type.address_len();
    if rest.len() < address_len + 1 {
        return Err(RecordDefect::TooShort);
    }
    let checksum_ok = raw.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte)) == 0xFF;
    if !checksum_ok {
        return Err(RecordDefect::ChecksumMismatch);
    }

    let address = rest[..address_len]
        .iter()
        .fold(0u32, |acc, &byte| (acc << 8) | u32::from(byte));
    // Reuse the decoded buffer as the data payload: drop checksum, count and address bytes.
    raw.pop();
    raw.drain(..1 + address_len);
    let record = SRecord {
        record_type,
        address,
        data: raw,
    };

    let address_space = 1u64 << (8 * address_len);
    if record_type.is_data() && record.end_address() > address_space {
        return Err(RecordDefect::AddressOverflow);
    }
    Ok(record)
}
