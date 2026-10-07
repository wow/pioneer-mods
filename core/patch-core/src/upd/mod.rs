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
//! The parser performs no I/O and never modifies its input. Parsed types can only be built by
//! [`parse_upd`], so every value upholds the layout rules it checked.

mod crc;
mod image;
mod srecord;
mod summary;
mod write;

pub use crc::crc16_xmodem;
pub use image::{DocumentImage, GAP_FILL, MAX_IMAGE_LEN};
pub use srecord::{SRecord, SRecordType};
pub use summary::{DocumentSummary, Extent, UpdSummary};
pub use write::verify_roundtrip;

use crate::error::{LayoutViolation, UpdError};
use std::collections::BTreeSet;
use std::ops::Range;

pub const DESCRIPTOR_LEN: usize = 32;
const MODEL_LEN: usize = 12;
const KIND_LEN: usize = 4;
const VERSION_LEN: usize = 7;
const RESERVED_START: usize = MODEL_LEN + KIND_LEN + VERSION_LEN;
pub const RESERVED_LEN: usize = DESCRIPTOR_LEN - RESERVED_START;
const CRC_LEN: usize = 2;
const MAX_LENGTH_DIGITS: usize = 19;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdContainer {
    documents: Vec<UpdDocument>,
}

impl UpdContainer {
    pub fn documents(&self) -> &[UpdDocument] {
        &self.documents
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdDocument {
    offset: usize,
    length: usize,
    descriptor: Descriptor,
    header: SRecord,
    data: Vec<SRecord>,
    termination: SRecord,
    crc16: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Descriptor {
    model: String,
    kind: String,
    version: String,
    reserved: [u8; RESERVED_LEN],
}

impl Descriptor {
    /// Model name with trailing space padding removed, e.g. `XDJ-700`.
    pub fn model(&self) -> &str {
        &self.model
    }

    /// Document kind, e.g. `MAIN` or `PANL`.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Version field, e.g. `Ver1.15`.
    pub fn version(&self) -> &str {
        &self.version
    }

    /// Remaining descriptor bytes, kept verbatim (meaning not yet established).
    pub fn reserved(&self) -> &[u8; RESERVED_LEN] {
        &self.reserved
    }
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

/// Splits the container after its length header.
///
/// Every line that starts with a digit belongs to the header and must be `[1-9][0-9]{0,18}\r\n`;
/// any other digit line is refused as malformed rather than treated as document data.
/// (Document descriptors start with a printable model name, never a digit.)
fn split_documents(bytes: &[u8]) -> Result<Vec<Range<usize>>, UpdError> {
    let mut lengths = Vec::new();
    let mut cursor = 0usize;
    while bytes.get(cursor).is_some_and(u8::is_ascii_digit) {
        let (value, next) = read_length_line(bytes, cursor)?;
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

/// Reads one length line starting with a digit at `start`; returns the value and next offset.
fn read_length_line(bytes: &[u8], start: usize) -> Result<(u64, usize), UpdError> {
    let window = &bytes[start..];
    let digits = window
        .iter()
        .take(MAX_LENGTH_DIGITS + 1)
        .take_while(|byte| byte.is_ascii_digit())
        .count();
    let well_formed = digits <= MAX_LENGTH_DIGITS
        && window.first() != Some(&b'0')
        && window.get(digits..digits + 2) == Some(b"\r\n".as_slice());
    if !well_formed {
        return Err(UpdError::MalformedLengthHeader);
    }
    let value = window[..digits]
        .iter()
        .fold(0u64, |acc, &digit| acc * 10 + u64::from(digit - b'0'));
    Ok((value, start + digits + 2))
}

fn parse_document(index: usize, offset: usize, doc: &[u8]) -> Result<UpdDocument, UpdError> {
    let too_short = || UpdError::DocumentTooShort {
        document_index: index,
        length: doc.len(),
    };
    let (content, trailer) = doc.split_last_chunk::<CRC_LEN>().ok_or_else(too_short)?;
    let (descriptor_bytes, body) = content
        .split_first_chunk::<DESCRIPTOR_LEN>()
        .ok_or_else(too_short)?;

    let stored = u16::from_le_bytes(*trailer);
    let computed = crc16_xmodem(content);
    if stored != computed {
        return Err(UpdError::DocumentCrcMismatch {
            document_index: index,
            stored,
            computed,
        });
    }

    let descriptor = parse_descriptor(index, descriptor_bytes)?;
    let (header, data, termination) = split_layout(index, parse_records(index, body)?)?;

    Ok(UpdDocument {
        offset,
        length: doc.len(),
        descriptor,
        header,
        data,
        termination,
        crc16: stored,
    })
}

fn parse_descriptor(index: usize, bytes: &[u8; DESCRIPTOR_LEN]) -> Result<Descriptor, UpdError> {
    let invalid = |field| UpdError::InvalidDescriptor {
        document_index: index,
        field,
    };
    let (model, rest) = bytes.split_at(MODEL_LEN);
    let (kind, version) = rest[..KIND_LEN + VERSION_LEN].split_at(KIND_LEN);
    let mut reserved = [0u8; RESERVED_LEN];
    reserved.copy_from_slice(&bytes[RESERVED_START..]);

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
        reserved,
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

/// Checks the record layout and splits it into header, data records, and termination.
///
/// Count records (S5/S6) are refused on purpose: the known format never contains them, and an
/// unknown structure must be refused rather than guessed at. They can be allowed once a real
/// file shows where they appear.
fn split_layout(
    index: usize,
    records: Vec<SRecord>,
) -> Result<(SRecord, Vec<SRecord>, SRecord), UpdError> {
    let layout_error = |record_index, violation| UpdError::UnexpectedRecordLayout {
        document_index: index,
        record_index,
        violation,
    };
    let last = records.len() - 1;
    if records[0].record_type() != SRecordType::S0 {
        return Err(layout_error(0, LayoutViolation::HeaderNotFirst));
    }
    if !records[last].record_type().is_termination() {
        return Err(layout_error(last, LayoutViolation::TerminationNotLast));
    }
    if !records[last].data().is_empty() {
        return Err(layout_error(last, LayoutViolation::TerminationHasData));
    }
    if last < 2 {
        return Err(layout_error(last, LayoutViolation::NoDataRecords));
    }

    let mut previous_end = 0u64;
    for (record_index, record) in records.iter().enumerate().take(last).skip(1) {
        if !record.record_type().is_data() {
            return Err(layout_error(
                record_index,
                LayoutViolation::NonDataRecordInBody,
            ));
        }
        if record.data().is_empty() {
            return Err(layout_error(record_index, LayoutViolation::EmptyDataRecord));
        }
        if u64::from(record.address()) < previous_end {
            return Err(layout_error(
                record_index,
                LayoutViolation::DataNotAscending,
            ));
        }
        previous_end = record.end_address();
    }

    let mut records = records.into_iter();
    let (Some(header), Some(termination)) = (records.next(), records.next_back()) else {
        return Err(layout_error(last, LayoutViolation::NoDataRecords));
    };
    Ok((header, records.collect(), termination))
}

impl UpdDocument {
    /// Byte offset of the document inside the container.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Document length in bytes, including descriptor and CRC trailer.
    pub fn length(&self) -> usize {
        self.length
    }

    pub fn descriptor(&self) -> &Descriptor {
        &self.descriptor
    }

    /// The S0 header record.
    pub fn header(&self) -> &SRecord {
        &self.header
    }

    /// Data records (S1/S2/S3), non-empty, in strictly ascending, non-overlapping order.
    pub fn data_records(&self) -> &[SRecord] {
        &self.data
    }

    /// The S7/S8/S9 termination record (carries no data).
    pub fn termination(&self) -> &SRecord {
        &self.termination
    }

    /// All records in file order: header, data records, termination.
    pub fn records(&self) -> impl Iterator<Item = &SRecord> {
        std::iter::once(&self.header)
            .chain(&self.data)
            .chain(std::iter::once(&self.termination))
    }

    /// CRC-16/XMODEM trailer; always equal to the CRC computed during parsing.
    pub fn crc16(&self) -> u16 {
        self.crc16
    }

    /// Contiguous address extents covered by data records, in ascending order.
    pub fn data_extents(&self) -> Vec<Range<u64>> {
        let mut extents: Vec<Range<u64>> = Vec::new();
        for record in &self.data {
            let start = u64::from(record.address());
            match extents.last_mut() {
                Some(extent) if extent.end == start => extent.end = record.end_address(),
                _ => extents.push(start..record.end_address()),
            }
        }
        extents
    }

    /// Distinct data record types used, in a stable order.
    pub fn data_record_types(&self) -> BTreeSet<SRecordType> {
        self.data.iter().map(SRecord::record_type).collect()
    }
}
