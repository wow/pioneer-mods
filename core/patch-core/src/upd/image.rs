//! Memory image reconstruction from a document's data records.

use super::UpdDocument;
use crate::error::UpdError;

/// Largest image span [`UpdDocument::image`] will allocate (the XDJ-700 MAIN image is ~7 MiB).
pub const MAX_IMAGE_LEN: u64 = 64 * 1024 * 1024;

/// Byte value used for addresses between data records (erased flash).
pub const GAP_FILL: u8 = 0xFF;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DocumentImage {
    base: u64,
    bytes: Vec<u8>,
}

impl DocumentImage {
    /// Address of the first data byte.
    pub fn base(&self) -> u64 {
        self.base
    }

    /// Image bytes from `base` to the end of the last data record, gaps filled with [`GAP_FILL`].
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
}

impl UpdDocument {
    /// Reconstructs the contiguous memory image covered by this document's data records.
    ///
    /// # Errors
    ///
    /// [`UpdError::ImageTooLarge`] if the span exceeds [`MAX_IMAGE_LEN`].
    pub fn image(&self, document_index: usize) -> Result<DocumentImage, UpdError> {
        let data = self.data_records();
        // Parsing guarantees at least one data record, in ascending order.
        let base = data.first().map_or(0, |record| u64::from(record.address()));
        let end = data.last().map_or(base, |record| record.end_address());
        let span = end - base;
        if span > MAX_IMAGE_LEN {
            return Err(UpdError::ImageTooLarge {
                document_index,
                span,
            });
        }

        let mut bytes = vec![GAP_FILL; span as usize];
        for record in data {
            let start = (u64::from(record.address()) - base) as usize;
            bytes[start..start + record.data().len()].copy_from_slice(record.data());
        }
        Ok(DocumentImage { base, bytes })
    }
}
