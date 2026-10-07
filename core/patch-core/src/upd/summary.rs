//! Serializable structure report for `patch-cli inspect --structure`.

use super::{MAX_IMAGE_LEN, SRecordType, UpdContainer, UpdDocument};
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
    /// Reconstructed image identity; `None` when `image_span` exceeds [`MAX_IMAGE_LEN`].
    pub image: Option<ImageSummary>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Extent {
    pub start: u64,
    pub end: u64,
}

/// Identity of a reconstructed memory image (gaps filled with `0xFF`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ImageSummary {
    /// Start address, formatted `0xNNNNNNNN`.
    pub base: String,
    pub len: u64,
    pub sha256: String,
}

impl UpdContainer {
    /// Summarizes every document. Images larger than [`MAX_IMAGE_LEN`] are reported by span only,
    /// so oversized documents never hide the rest of the structure report.
    pub fn summary(&self) -> UpdSummary {
        UpdSummary {
            documents: self.documents().iter().map(UpdDocument::summary).collect(),
        }
    }
}

impl UpdDocument {
    fn summary(&self) -> DocumentSummary {
        let descriptor = self.descriptor();
        let data = self.data_records();
        let span = self.image_span();
        let image_span = span.end - span.start;
        let image = (image_span <= MAX_IMAGE_LEN)
            .then(|| self.image().ok())
            .flatten()
            .map(|image| ImageSummary {
                base: format!("0x{:08X}", image.base()),
                len: image.bytes().len() as u64,
                sha256: sha256_hex(image.bytes()),
            });
        DocumentSummary {
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
        }
    }
}
