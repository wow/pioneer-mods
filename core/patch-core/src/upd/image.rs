//! Memory image reconstruction from a document's data records.

use super::UpdDocument;
use crate::error::UpdError;
use std::ops::Range;

/// Largest image span [`UpdDocument::image`] will allocate (the XDJ-700 MAIN image is ~7 MiB).
pub const MAX_IMAGE_LEN: u64 = 64 * 1024 * 1024;

/// Total image bytes [`crate::upd::UpdContainer::summary`] reconstructs across all documents.
///
/// Bounds the work a small crafted file can cause (many sparse documents) independently of the
/// per-document cap; the official XDJ-700 v1.15 images total about 7.3 MiB.
pub const MAX_TOTAL_IMAGE_LEN: u64 = 2 * MAX_IMAGE_LEN;

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
    /// Address range from the first data byte to the end of the last data record.
    ///
    /// Computed without allocating; parsing guarantees at least one data record, in ascending
    /// order, so `start <= end`.
    pub fn image_span(&self) -> Range<u64> {
        let data = self.data_records();
        let start = data.first().map_or(0, |record| u64::from(record.address()));
        let end = data.last().map_or(start, |record| record.end_address());
        start..end
    }

    /// Reconstructs the contiguous memory image covered by this document's data records.
    ///
    /// # Errors
    ///
    /// [`UpdError::ImageTooLarge`] if the span exceeds [`MAX_IMAGE_LEN`].
    pub fn image(&self) -> Result<DocumentImage, UpdError> {
        let Range { start: base, end } = self.image_span();
        let span = end - base;
        if span > MAX_IMAGE_LEN {
            return Err(UpdError::ImageTooLarge {
                document_index: self.index(),
                span,
            });
        }

        let mut bytes = vec![GAP_FILL; span as usize];
        for record in self.data_records() {
            let start = (u64::from(record.address()) - base) as usize;
            bytes[start..start + record.data().len()].copy_from_slice(record.data());
        }
        Ok(DocumentImage { base, bytes })
    }
}
