//! XDJ-700 specific layout inside the MAIN document image.
//!
//! The MAIN image holds the compressed application section at [`APPLICATION_SECTION_OFFSET`]:
//!
//! ```text
//! [u32 LE size][size bytes: LZSS stream starting with the section prefix][u16 LE checksum]
//! ```
//!
//! The checksum is the 16-bit sum of the size field and every compressed byte. According to
//! DeckVolve, the boot loader verifies it and falls back to a smaller updater section on mismatch.

use crate::error::SectionError;
use crate::identity::sha256_hex;
use crate::lzss;
use crate::upd::{UpdContainer, UpdDocument};

/// Offset of the compressed application section inside the MAIN image.
pub const APPLICATION_SECTION_OFFSET: usize = 0x40000;

/// MAIN versions whose application-section layout has been verified against an official file.
/// Other versions are refused rather than decoded at a guessed offset.
pub const VERIFIED_MAIN_VERSIONS: &[&str] = &["Ver1.15"];

/// Start of the stock section stream (`01 00 EE FF`). The device decodes it as data: a literal
/// `0x00` and a match that repeats it 18 times, i.e. a 19-byte zero prefix.
///
/// The first byte is a flag byte covering 8 items; only its two low bits belong to the prefix
/// (literal, then match). Bits 2..=7 describe the stream data that follows and may differ from
/// the stock `0x01` in a re-encoded section, so [`decode_section`] checks only the invariant part.
pub const SECTION_TAG: [u8; 4] = [0x01, 0x00, 0xEE, 0xFF];

/// Largest decoded application [`decode_section`] will produce (stock v1.15 is ~17.7 MiB).
pub const MAX_DECODED_LEN: usize = 64 * 1024 * 1024;

const SIZE_FIELD_LEN: usize = 4;
const CHECKSUM_LEN: usize = 2;

/// A verified and decoded compressed section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodedSection {
    offset: usize,
    compressed_len: usize,
    checksum: u16,
    decoded: Vec<u8>,
}

impl DecodedSection {
    /// Offset of the size field inside the image.
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// Value of the size field: tag plus LZSS stream bytes.
    pub fn compressed_len(&self) -> usize {
        self.compressed_len
    }

    /// Stored checksum, verified equal to the computed one.
    pub fn checksum(&self) -> u16 {
        self.checksum
    }

    pub fn decoded(&self) -> &[u8] {
        &self.decoded
    }

    pub fn decoded_sha256(&self) -> String {
        sha256_hex(&self.decoded)
    }
}

/// Verifies and decodes the compressed section whose size field starts at `offset` in `image`.
///
/// # Errors
///
/// A [`SectionError`] for out-of-bounds fields, a checksum mismatch, a missing tag, or an LZSS
/// decode failure (including exceeding [`MAX_DECODED_LEN`]).
pub fn decode_section(image: &[u8], offset: usize) -> Result<DecodedSection, SectionError> {
    let size_bytes = offset
        .checked_add(SIZE_FIELD_LEN)
        .and_then(|end| image.get(offset..end))
        .ok_or(SectionError::SizeFieldOutOfBounds { offset })?;
    let declared_len =
        u32::from_le_bytes([size_bytes[0], size_bytes[1], size_bytes[2], size_bytes[3]]);
    let out_of_bounds = || SectionError::DataOutOfBounds {
        offset,
        declared_len,
    };
    let compressed_len = usize::try_from(declared_len).map_err(|_| out_of_bounds())?;
    let data_start = offset + SIZE_FIELD_LEN;
    let data_end = data_start
        .checked_add(compressed_len)
        .filter(|&end| end <= image.len())
        .ok_or_else(out_of_bounds)?;
    let checksum_bytes = image
        .get(data_end..data_end + CHECKSUM_LEN)
        .ok_or(SectionError::ChecksumOutOfBounds { offset })?;

    let stored = u16::from_le_bytes([checksum_bytes[0], checksum_bytes[1]]);
    let computed = image[offset..data_end]
        .iter()
        .fold(0u16, |sum, &byte| sum.wrapping_add(u16::from(byte)));
    if stored != computed {
        return Err(SectionError::ChecksumMismatch { stored, computed });
    }

    let stream = &image[data_start..data_end];
    let has_prefix = matches!(stream, [flags, 0x00, 0xEE, 0xFF, ..] if flags & 0b11 == 0b01);
    if !has_prefix {
        return Err(SectionError::MissingTag);
    }
    let decoded = lzss::decode(stream, MAX_DECODED_LEN)?;

    Ok(DecodedSection {
        offset,
        compressed_len,
        checksum: stored,
        decoded,
    })
}

/// True when the container looks like an XDJ-700 update (a `MAIN` document from model
/// `XDJ-700`), i.e. when [`decode_application`] applies.
pub fn is_xdj700(container: &UpdContainer) -> bool {
    main_document(container).is_some()
}

/// Reconstructs the MAIN image and decodes its application section.
///
/// # Errors
///
/// [`SectionError::NoMainDocument`] if the container is not an XDJ-700 update,
/// [`SectionError::UnverifiedVersion`] if the MAIN version is not in
/// [`VERIFIED_MAIN_VERSIONS`], an image error, or any [`decode_section`] error.
pub fn decode_application(container: &UpdContainer) -> Result<DecodedSection, SectionError> {
    let main = main_document(container).ok_or(SectionError::NoMainDocument)?;
    let version = main.descriptor().version();
    if !VERIFIED_MAIN_VERSIONS.contains(&version) {
        return Err(SectionError::UnverifiedVersion {
            version: version.to_owned(),
        });
    }
    let image = main.image().map_err(|_| SectionError::ImageUnavailable)?;
    if image.base() != 0 {
        return Err(SectionError::ImageUnavailable);
    }
    decode_section(image.bytes(), APPLICATION_SECTION_OFFSET)
}

fn main_document(container: &UpdContainer) -> Option<&UpdDocument> {
    container.documents().iter().find(|document| {
        document.descriptor().model() == "XDJ-700" && document.descriptor().kind() == "MAIN"
    })
}
