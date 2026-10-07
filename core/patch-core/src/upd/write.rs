//! Canonical `.UPD` serializer with a runtime self-check.

use super::{DESCRIPTOR_LEN, Descriptor, MODEL_LEN, UpdContainer, UpdDocument, crc16_xmodem};
use crate::error::UpdError;

/// Serialized bytes that passed [`UpdContainer::verify_serialized`].
struct SelfChecked(Vec<u8>);

impl UpdContainer {
    /// Serializes the container and verifies the result before returning it.
    ///
    /// The output is re-parsed and must yield a container equal to `self`; any serializer defect
    /// is refused here, before a caller can write the bytes anywhere.
    ///
    /// # Errors
    ///
    /// [`UpdError::SerializerSelfCheckFailed`] if the output does not re-parse to `self`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, UpdError> {
        let SelfChecked(bytes) = self.self_checked(self.encode())?;
        Ok(bytes)
    }

    /// Checks that `bytes` parse to a container equal to `self`.
    ///
    /// # Errors
    ///
    /// [`UpdError::SerializerSelfCheckFailed`] if `bytes` do not parse, or parse to a different
    /// container.
    pub fn verify_serialized(&self, bytes: &[u8]) -> Result<(), UpdError> {
        match super::parse_upd(bytes) {
            Ok(reparsed) if reparsed == *self => Ok(()),
            _ => Err(UpdError::SerializerSelfCheckFailed),
        }
    }

    /// The only constructor of [`SelfChecked`], so `to_bytes` cannot skip the check.
    fn self_checked(&self, bytes: Vec<u8>) -> Result<SelfChecked, UpdError> {
        self.verify_serialized(&bytes)?;
        Ok(SelfChecked(bytes))
    }

    fn encode(&self) -> Vec<u8> {
        let documents: Vec<Vec<u8>> = self.documents.iter().map(UpdDocument::encode).collect();
        let total: usize = documents.iter().map(Vec::len).sum();
        let mut out = Vec::with_capacity(total + 24 * documents.len());
        for document in &documents {
            out.extend_from_slice(format!("{}\r\n", document.len()).as_bytes());
        }
        for document in &documents {
            out.extend_from_slice(document);
        }
        out
    }
}

impl UpdDocument {
    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.length);
        self.descriptor.encode(&mut out);
        for record in self.records() {
            record.write_line(&mut out);
        }
        let crc = crc16_xmodem(&out);
        out.extend_from_slice(&crc.to_le_bytes());
        out
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
/// Any parse error, [`UpdError::SerializerSelfCheckFailed`], or
/// [`UpdError::RoundTripMismatch`] with the first differing byte offset.
pub fn verify_roundtrip(input: &[u8]) -> Result<UpdContainer, UpdError> {
    let container = super::parse_upd(input)?;
    let output = container.to_bytes()?;
    if output != input {
        let offset = output
            .iter()
            .zip(input)
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| output.len().min(input.len()));
        return Err(UpdError::RoundTripMismatch { offset });
    }
    Ok(container)
}
