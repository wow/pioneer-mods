//! Serializable structure report for `patch-cli inspect --structure`.

use super::{MAX_TOTAL_IMAGE_LEN, SRecordType, UpdContainer, UpdDocument};
use crate::error::UpdError;
use crate::identity::sha256_hex;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UpdSummary {
    pub documents: Vec<DocumentSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DocumentSummary {
    pub index: usize,
    pub offset: usize,
    pub length: usize,
    pub model: String,
    pub kind: String,
    pub version: String,
    /// Reserved descriptor bytes as lowercase hex.
    pub reserved_hex: String,
    /// Verified CRC-16/XMODEM trailer, formatted `0xNNNN`.
    pub crc16: String,
    /// S0 header data with non-printable bytes escaped.
    pub header_text: String,
    pub data_record_types: Vec<SRecordType>,
    pub data_records: usize,
    pub data_bytes: u64,
    pub extents: Vec<Extent>,
    pub termination_type: SRecordType,
    /// Entry/start address from the termination record, formatted `0xNNNNNNNN`.
    pub entry_address: String,
    /// Bytes from the first data byte to the end of the last data record.
    pub image_span: u64,
    /// Reconstructed image identity, or why it was not reconstructed.
    pub image: ImageReport,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Extent {
    pub start: u64,
    pub end: u64,
}

/// Image reconstruction outcome for one document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum ImageReport {
    /// Identity of the reconstructed memory image (gaps filled with `0xFF`).
    Reconstructed {
        /// Start address, formatted `0xNNNNNNNN`.
        base: String,
        len: u64,
        sha256: String,
    },
    /// The span exceeds the per-document cap [`super::MAX_IMAGE_LEN`].
    SpanExceedsCap,
    /// Earlier documents used up the container-wide budget [`MAX_TOTAL_IMAGE_LEN`].
    BudgetExhausted,
}

impl UpdContainer {
    /// Summarizes every document, reconstructing and hashing images within the container-wide
    /// budget [`MAX_TOTAL_IMAGE_LEN`]. Images over the per-document cap or the remaining budget are
    /// reported with the reason, so they never hide the rest of the structure report.
    ///
    /// # Errors
    ///
    /// Any image error other than the per-document cap (none exist today); such errors are
    /// propagated rather than reported as a skipped image.
    pub fn summary(&self) -> Result<UpdSummary, UpdError> {
        let mut budget = MAX_TOTAL_IMAGE_LEN;
        let documents = self
            .documents()
            .iter()
            .map(|document| document.summary(&mut budget))
            .collect::<Result<_, _>>()?;
        Ok(UpdSummary { documents })
    }
}

impl UpdDocument {
    fn summary(&self, budget: &mut u64) -> Result<DocumentSummary, UpdError> {
        let descriptor = self.descriptor();
        let data = self.data_records();
        let span = self.image_span();
        let image_span = span.end - span.start;
        let image = self.image_report(image_span, budget)?;
        Ok(DocumentSummary {
            index: self.index(),
            offset: self.offset(),
            length: self.length(),
            model: descriptor.model().to_owned(),
            kind: descriptor.kind().to_owned(),
            version: descriptor.version().to_owned(),
            reserved_hex: hex::encode(descriptor.reserved()),
            crc16: format!("0x{:04X}", self.crc16()),
            header_text: self.header().data().escape_ascii().to_string(),
            data_record_types: self.data_record_types().into_iter().collect(),
            data_records: data.len(),
            data_bytes: data.iter().map(|record| record.data().len() as u64).sum(),
            extents: self
                .data_extents()
                .into_iter()
                .map(|range| Extent {
                    start: range.start,
                    end: range.end,
                })
                .collect(),
            termination_type: self.termination().record_type(),
            entry_address: format!("0x{:08X}", self.termination().address()),
            image_span,
            image,
        })
    }

    /// The budget is checked before allocating; the per-document cap is enforced by `image()`.
    fn image_report(&self, image_span: u64, budget: &mut u64) -> Result<ImageReport, UpdError> {
        if image_span > *budget {
            return Ok(ImageReport::BudgetExhausted);
        }
        match self.image() {
            Ok(image) => {
                *budget -= image_span;
                Ok(ImageReport::Reconstructed {
                    base: format!("0x{:08X}", image.base()),
                    len: image.bytes().len() as u64,
                    sha256: sha256_hex(image.bytes()),
                })
            }
            Err(UpdError::ImageTooLarge { .. }) => Ok(ImageReport::SpanExceedsCap),
            Err(error) => Err(error),
        }
    }
}
