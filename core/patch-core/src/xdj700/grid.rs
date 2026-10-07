//! The MAIN S-record grid: [`RECORD_DATA_LEN`]-byte S2 records over a document's extents, the
//! last extent ending at the image end. Shared by the rebuild (which writes records) and its
//! verification (which only compares them).

use crate::upd::{SRecord, SRecordType};
use std::ops::Range;

/// Data bytes per MAIN S2 record, as in the official update.
pub const RECORD_DATA_LEN: usize = 32;
/// S2 records carry 24-bit addresses.
pub(crate) const S2_ADDRESS_SPACE: usize = 1 << 24;

/// The image ranges of the grid records for an image of `image_len` bytes (based at 0), or
/// `None` if `extents` do not fit it.
fn cuts(image_len: usize, extents: &[Range<u64>]) -> Option<impl Iterator<Item = Range<usize>>> {
    let mut bounds = Vec::with_capacity(extents.len());
    for (position, extent) in extents.iter().enumerate() {
        let start = usize::try_from(extent.start).ok()?;
        let end = if position + 1 == extents.len() {
            image_len
        } else {
            usize::try_from(extent.end).ok()?
        };
        // Extents ascend, so `end > image_len` before the last extent is already implied by the
        // last one starting inside the image; it is kept so no slice below can go out of bounds.
        if start >= end || end > image_len || end > S2_ADDRESS_SPACE {
            return None;
        }
        bounds.push(start..end);
    }
    Some(bounds.into_iter().flat_map(|extent| {
        let end = extent.end;
        extent
            .step_by(RECORD_DATA_LEN)
            .map(move |address| address..(address + RECORD_DATA_LEN).min(end))
    }))
}

/// The grid records for `image`, or `None` if `extents` do not fit it.
pub(crate) fn grid_records(image: &[u8], extents: &[Range<u64>]) -> Option<Vec<SRecord>> {
    let records = cuts(image.len(), extents)?
        .map(|cut| SRecord::data_record(SRecordType::S2, cut.start as u32, image[cut].to_vec()))
        .collect();
    Some(records)
}

/// Whether `records` are exactly the grid records for `image`, without building them.
pub(crate) fn follows_grid(image: &[u8], extents: &[Range<u64>], records: &[SRecord]) -> bool {
    let Some(mut cuts) = cuts(image.len(), extents) else {
        return false;
    };
    let mut records = records.iter();
    let all_match = cuts.all(|cut| {
        records.next().is_some_and(|record| {
            record.record_type() == SRecordType::S2
                && record.address() as usize == cut.start
                && record.data() == &image[cut]
        })
    });
    // A record past the last cut is impossible for a parsed image (it would extend the image and
    // so the last cut); the check keeps the comparison exact on its own.
    all_match && records.next().is_none()
}
