//! Read-only parser for Pioneer `.UPD` update containers (XDJ-700 generation).
//!
//! Layout, as modelled by the MIT-licensed DeckVolve xdj-700-mods patcher
//! (<https://github.com/DeckVolve/xdj-700-mods>):
//!
//! ```text
//! "<len0>\r\n<len1>\r\n...<document0><document1>...
//! document = descriptor[32] + S-record lines (each CRLF-terminated) + CRC-16/XMODEM (LE u16)
//! descriptor = model[12] + kind[4] + version[7] + reserved[9]
//! ```
//!
//! The parser performs no I/O and never modifies its input.

mod crc;
mod srecord;
mod summary;

pub use crc::crc16_xmodem;
pub use srecord::{SRecord, SRecordType};
pub use summary::{DocumentSummary, Extent, UpdSummary};

use crate::error::{LayoutViolation, UpdError};
use std::collections::BTreeSet;
use std::ops::Range;

pub const DESCRIPTOR_LEN: usize = 32;
const MODEL_LEN: usize = 12;
const KIND_LEN: usize = 4;
const VERSION_LEN: usize = 7;
const CRC_LEN: usize = 2;
const MAX_LENGTH_DIGITS: usize = 19;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdContainer {
    pub documents: Vec<UpdDocument>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdDocument {
    /// Byte offset of the document inside the container.
    pub offset: usize,
    /// Document length in bytes, including descriptor and CRC trailer.
    pub length: usize,
    pub descriptor: Descriptor,
    /// Header record (S0), data records, and the termination record, in file order.
    pub records: Vec<SRecord>,
    /// Stored (and verified) CRC-16/XMODEM trailer.
    pub crc16: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    /// Model name with trailing space padding removed, e.g. `XDJ-700`.
    pub model: String,
    /// Document kind, e.g. `MAIN` or `PANL`.
    pub kind: String,
    /// Version field, e.g. `Ver1.15`.
    pub version: String,
    /// Remaining descriptor bytes, kept verbatim (meaning not yet established).
    pub reserved: [u8; DESCRIPTOR_LEN - MODEL_LEN - KIND_LEN - VERSION_LEN],
}

/// Parses and validates a complete `.UPD` container.
///
/// # Errors
///
/// Returns the first structural violation found: length header, document CRC, descriptor,
/// S-record syntax/checksum, or record layout (S0 first, ascending non-overlapping data,
/// single termination record last).
pub fn parse_upd(bytes: &[u8]) -> Result<UpdContainer, UpdError> {
    let documents = split_documents(bytes)?
        .into_iter()
        .enumerate()
        .map(|(index, range)| parse_document(index, range.start, &bytes[range]))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(UpdContainer { documents })
}

fn split_documents(bytes: &[u8]) -> Result<Vec<Range<usize>>, UpdError> {
    let mut lengths = Vec::new();
    let mut cursor = 0usize;
    while let Some((value, next)) = read_length_line(bytes, cursor) {
        lengths.push(value);
        cursor = next;
    }
    if lengths.is_empty() {
        return Err(UpdError::MissingLengthHeader);
    }

    let declared = lengths
        .iter()
        .try_fold(0u64, |sum, &length| sum.checked_add(length))
        .ok_or(UpdError::MalformedLengthHeader)?;
    let available = bytes.len() - cursor;
    if declared != available as u64 {
        return Err(UpdError::LengthHeaderMismatch {
            declared,
            available,
        });
    }

    let mut ranges = Vec::with_capacity(lengths.len());
    for length in lengths {
        // Each length fits: their sum equals `available`, which is a usize.
        let end = cursor + length as usize;
        ranges.push(cursor..end);
        cursor = end;
    }
    Ok(ranges)
}

/// Reads one `[1-9][0-9]*\r\n` line at `start`, returning the value and the next offset.
fn read_length_line(bytes: &[u8], start: usize) -> Option<(u64, usize)> {
    let window = bytes.get(start..)?;
    let digits = window
        .iter()
        .take(MAX_LENGTH_DIGITS + 1)
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    if digits == 0 || digits > MAX_LENGTH_DIGITS || window[0] == b'0' {
        return None;
    }
    if window.get(digits..digits + 2) != Some(b"\r\n".as_slice()) {
        return None;
    }
    let text = std::str::from_utf8(&window[..digits]).ok()?;
    Some((text.parse().ok()?, start + digits + 2))
}

fn parse_document(index: usize, offset: usize, doc: &[u8]) -> Result<UpdDocument, UpdError> {
    if doc.len() < DESCRIPTOR_LEN + CRC_LEN {
        return Err(UpdError::DocumentTooShort {
            document_index: index,
            length: doc.len(),
        });
    }
    let (content, trailer) = doc.split_at(doc.len() - CRC_LEN);
    let stored = u16::from_le_bytes([trailer[0], trailer[1]]);
    let computed = crc16_xmodem(content);
    if stored != computed {
        return Err(UpdError::DocumentCrcMismatch {
            document_index: index,
            stored,
            computed,
        });
    }

    let (descriptor_bytes, body) = content.split_at(DESCRIPTOR_LEN);
    let descriptor = parse_descriptor(index, descriptor_bytes)?;
    let records = parse_records(index, body)?;
    validate_layout(index, &records)?;

    Ok(UpdDocument {
        offset,
        length: doc.len(),
        descriptor,
        records,
        crc16: stored,
    })
}

fn parse_descriptor(index: usize, bytes: &[u8]) -> Result<Descriptor, UpdError> {
    let invalid = |field| UpdError::InvalidDescriptor {
        document_index: index,
        field,
    };
    let (model, rest) = bytes.split_at(MODEL_LEN);
    let (kind, rest) = rest.split_at(KIND_LEN);
    let (version, reserved) = rest.split_at(VERSION_LEN);

    let model = printable_ascii(model).ok_or_else(|| invalid("model"))?;
    let model = model.trim_end_matches(' ');
    if model.is_empty() {
        return Err(invalid("model"));
    }
    if !kind.iter().all(|byte| byte.is_ascii_alphanumeric()) {
        return Err(invalid("kind"));
    }
    let version = printable_ascii(version).ok_or_else(|| invalid("version"))?;

    Ok(Descriptor {
        model: model.to_owned(),
        kind: String::from_utf8_lossy(kind).into_owned(),
        version: version.to_owned(),
        reserved: reserved.try_into().map_err(|_| invalid("reserved"))?,
    })
}

fn printable_ascii(bytes: &[u8]) -> Option<&str> {
    if bytes.iter().all(|byte| (0x20..=0x7E).contains(byte)) {
        std::str::from_utf8(bytes).ok()
    } else {
        None
    }
}

fn parse_records(index: usize, body: &[u8]) -> Result<Vec<SRecord>, UpdError> {
    if !body.ends_with(b"\r\n") {
        return Err(UpdError::MissingFinalCrlf {
            document_index: index,
        });
    }
    // Strip only the final LF so every line, including the last, still ends with CR.
    body[..body.len() - 1]
        .split(|&byte| byte == b'\n')
        .enumerate()
        .map(|(record_index, line)| {
            let line = line.strip_suffix(b"\r").unwrap_or(b"");
            srecord::parse_record(line).map_err(|defect| UpdError::MalformedRecord {
                document_index: index,
                record_index,
                defect,
            })
        })
        .collect()
}

fn validate_layout(index: usize, records: &[SRecord]) -> Result<(), UpdError> {
    let layout_error = |record_index, violation| UpdError::UnexpectedRecordLayout {
        document_index: index,
        record_index,
        violation,
    };
    let last = records.len() - 1;
    if records[0].record_type != SRecordType::S0 {
        return Err(layout_error(0, LayoutViolation::HeaderNotFirst));
    }
    if !records[last].record_type.is_termination() {
        return Err(layout_error(last, LayoutViolation::TerminationNotLast));
    }
    if !records[last].data.is_empty() {
        return Err(layout_error(last, LayoutViolation::TerminationHasData));
    }
    if last < 2 {
        return Err(layout_error(last, LayoutViolation::NoDataRecords));
    }

    let mut previous_end = 0u64;
    for (record_index, record) in records.iter().enumerate().take(last).skip(1) {
        if !record.record_type.is_data() {
            return Err(layout_error(
                record_index,
                LayoutViolation::NonDataRecordInBody,
            ));
        }
        if record.data.is_empty() {
            return Err(layout_error(record_index, LayoutViolation::EmptyDataRecord));
        }
        if u64::from(record.address) < previous_end {
            return Err(layout_error(
                record_index,
                LayoutViolation::DataNotAscending,
            ));
        }
        previous_end = record.end_address();
    }
    Ok(())
}

impl UpdDocument {
    /// Data records only (excludes header and termination records).
    pub fn data_records(&self) -> &[SRecord] {
        let last = self.records.len().saturating_sub(1);
        self.records.get(1..last).unwrap_or(&[])
    }

    /// Contiguous address extents covered by data records, in ascending order.
    pub fn data_extents(&self) -> Vec<Range<u64>> {
        let mut extents: Vec<Range<u64>> = Vec::new();
        for record in self.data_records() {
            let start = u64::from(record.address);
            match extents.last_mut() {
                Some(extent) if extent.end == start => extent.end = record.end_address(),
                _ => extents.push(start..record.end_address()),
            }
        }
        extents
    }

    /// Distinct data record types used, in a stable order.
    pub fn data_record_types(&self) -> BTreeSet<SRecordType> {
        self.data_records()
            .iter()
            .map(|record| record.record_type)
            .collect()
    }
}
