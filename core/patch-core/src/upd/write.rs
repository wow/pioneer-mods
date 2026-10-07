//! Canonical `.UPD` serializer with a runtime self-check.

use super::{
    CRC_LEN, DESCRIPTOR_LEN, Descriptor, MODEL_LEN, SRecord, UpdContainer, UpdDocument,
    crc16_xmodem,
};
use crate::error::UpdError;

impl UpdContainer {
    /// Serializes the container and verifies the result before returning it.
    ///
    /// The output is re-parsed and must yield a container equal to `self`; any serializer defect
    /// is refused here, before a caller can write the bytes anywhere. `encode` is private, so
    /// this is the only public way to obtain serialized bytes.
    ///
    /// # Errors
    ///
    /// [`UpdError::SerializerOutputUnparseable`] or [`UpdError::SerializerSelfCheckFailed`] if
    /// the output does not re-parse to `self`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, UpdError> {
        let bytes = self.encode();
        self.verify_serialized(&bytes)?;
        Ok(bytes)
    }

    /// Checks that `bytes` parse to a container equal to `self`.
    ///
    /// # Errors
    ///
    /// [`UpdError::SerializerOutputUnparseable`] (carrying the parse error) if `bytes` do not
    /// parse, or [`UpdError::SerializerSelfCheckFailed`] if they parse to a different container.
    pub fn verify_serialized(&self, bytes: &[u8]) -> Result<(), UpdError> {
        let reparsed = super::parse_upd(bytes)
            .map_err(|error| UpdError::SerializerOutputUnparseable(Box::new(error)))?;
        if reparsed != *self {
            return Err(UpdError::SerializerSelfCheckFailed);
        }
        Ok(())
    }

    /// Proves that serializing `self` reproduces `input` byte-for-byte.
    ///
    /// Byte equality with the input that `self` was parsed from is the strongest possible check,
    /// so the output is only re-parsed when it differs, to diagnose the serializer defect.
    ///
    /// # Errors
    ///
    /// A serializer self-check error, or [`UpdError::RoundTripMismatch`] with the first differing
    /// byte offset. Both indicate a defect in this library, not in the input.
    pub fn verify_reproduces(&self, input: &[u8]) -> Result<(), UpdError> {
        let output = self.encode();
        if output == input {
            return Ok(());
        }
        self.verify_serialized(&output)?;
        let offset = output
            .iter()
            .zip(input)
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| output.len().min(input.len()));
        Err(UpdError::RoundTripMismatch { offset })
    }

    /// Encodes into a single buffer sized up front: the length header needs every document's
    /// encoded length before any document is written.
    fn encode(&self) -> Vec<u8> {
        let lengths: Vec<usize> = self
            .documents
            .iter()
            .map(UpdDocument::encoded_len)
            .collect();
        let header: String = lengths
            .iter()
            .map(|length| format!("{length}\r\n"))
            .collect();
        let mut out = Vec::with_capacity(header.len() + lengths.iter().sum::<usize>());
        out.extend_from_slice(header.as_bytes());
        for document in &self.documents {
            document.encode_into(&mut out);
        }
        out
    }
}

impl UpdDocument {
    fn encoded_len(&self) -> usize {
        DESCRIPTOR_LEN + self.records().map(SRecord::line_len).sum::<usize>() + CRC_LEN
    }

    fn encode_into(&self, out: &mut Vec<u8>) {
        let start = out.len();
        self.descriptor.encode(out);
        for record in self.records() {
            record.write_line(out);
        }
        let crc = crc16_xmodem(&out[start..]);
        out.extend_from_slice(&crc.to_le_bytes());
    }
}

impl Descriptor {
    fn encode(&self, out: &mut Vec<u8>) {
        let start = out.len();
        out.extend_from_slice(self.model.as_bytes());
        out.resize(start + MODEL_LEN, b' ');
        out.extend_from_slice(self.kind.as_bytes());
        out.extend_from_slice(self.version.as_bytes());
        out.extend_from_slice(&self.reserved);
        debug_assert_eq!(out.len() - start, DESCRIPTOR_LEN);
    }
}

/// Parses `input` and proves that serializing the result reproduces `input` byte-for-byte.
///
/// This is the Phase 1 no-op gate. The parser only accepts canonical files, so for any input it
/// accepts the byte comparison is expected to pass; it is kept as an explicit, independent guard
/// against serializer regressions.
///
/// # Errors
///
/// Any parse error (the input is invalid), or an error from
/// [`UpdContainer::verify_reproduces`] (a defect in this library).
pub fn verify_roundtrip(input: &[u8]) -> Result<UpdContainer, UpdError> {
    let container = super::parse_upd(input)?;
    container.verify_reproduces(input)?;
    Ok(container)
}
