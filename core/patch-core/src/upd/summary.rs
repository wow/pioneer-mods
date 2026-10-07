//! Serializable structure report for `patch-cli inspect --structure`.

use super::{SRecordType, UpdContainer, UpdDocument};
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Extent {
    pub start: u64,
    pub end: u64,
}

impl UpdContainer {
    pub fn summary(&self) -> UpdSummary {
        UpdSummary {
            documents: self
                .documents()
                .iter()
                .enumerate()
                .map(|(index, document)| document.summary(index))
                .collect(),
        }
    }
}

impl UpdDocument {
    fn summary(&self, index: usize) -> DocumentSummary {
        let descriptor = self.descriptor();
        let data = self.data_records();
        DocumentSummary {
            index,
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
        }
    }
}
