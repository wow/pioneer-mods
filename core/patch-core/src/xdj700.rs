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

use crate::error::{LzssError, SectionError};
use crate::identity::sha256_hex;
use crate::lzss;
use crate::upd::{DocumentImage, UpdContainer, UpdDocument};

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
    let computed = section_checksum(&image[offset..data_end]);
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

/// Encodes `decoded` into complete section bytes (`[u32 LE size][stream][u16 LE checksum]`).
///
/// Self-checked before returning: the bytes are verified and decoded with [`decode_section`],
/// and must reproduce `decoded` exactly.
///
/// # Errors
///
/// [`SectionError::Encode`] if the input cannot be encoded (for example a missing 19-byte zero
/// prefix), [`SectionError::SectionTooLarge`] if the stream does not fit the `u32` size field, or
/// [`SectionError::EncodeSelfCheckFailed`] if the result does not decode back to `decoded`.
pub fn encode_section(decoded: &[u8]) -> Result<Vec<u8>, SectionError> {
    if decoded.len() > MAX_DECODED_LEN {
        return Err(SectionError::Encode(LzssError::InputTooLarge {
            len: decoded.len(),
        }));
    }
    let stream = lzss::encode_section_stream(decoded).map_err(SectionError::Encode)?;
    let size = u32::try_from(stream.len())
        .map_err(|_| SectionError::SectionTooLarge { len: stream.len() })?;
    let mut bytes = Vec::with_capacity(SIZE_FIELD_LEN + stream.len() + CHECKSUM_LEN);
    bytes.extend_from_slice(&size.to_le_bytes());
    bytes.extend_from_slice(&stream);
    let checksum = section_checksum(&bytes);
    bytes.extend_from_slice(&checksum.to_le_bytes());

    match decode_section(&bytes, 0) {
        Ok(section) if section.decoded() == decoded => Ok(bytes),
        _ => Err(SectionError::EncodeSelfCheckFailed),
    }
}

/// The section checksum: 16-bit wrapping sum of `size_field_and_stream` (the `u32` size field
/// followed by the compressed stream).
pub fn section_checksum(size_field_and_stream: &[u8]) -> u16 {
    size_field_and_stream
        .iter()
        .fold(0u16, |sum, &byte| sum.wrapping_add(u16::from(byte)))
}

/// True when the container has at least one `MAIN` document from model `XDJ-700`, i.e. when an
/// application report applies (an ambiguous container is then reported, not ignored).
pub fn is_xdj700(container: &UpdContainer) -> bool {
    container.documents().iter().any(is_xdj700_main)
}

/// The container's single XDJ-700 `MAIN` document.
///
/// # Errors
///
/// [`SectionError::NoMainDocument`] if there is none, or
/// [`SectionError::AmbiguousMainDocument`] if there is more than one: the device would use only
/// one of them, so a report about "the" application could describe the wrong bytes.
pub fn main_document(container: &UpdContainer) -> Result<&UpdDocument, SectionError> {
    let mut mains = container
        .documents()
        .iter()
        .filter(|doc| is_xdj700_main(doc));
    let first = mains.next().ok_or(SectionError::NoMainDocument)?;
    let others = mains.count();
    if others > 0 {
        return Err(SectionError::AmbiguousMainDocument { count: others + 1 });
    }
    Ok(first)
}

/// Refuses MAIN versions whose application layout has not been verified.
///
/// # Errors
///
/// [`SectionError::UnverifiedVersion`] if the version is not in [`VERIFIED_MAIN_VERSIONS`].
pub fn verify_main_version(main: &UpdDocument) -> Result<(), SectionError> {
    let version = main.descriptor().version();
    if VERIFIED_MAIN_VERSIONS.contains(&version) {
        Ok(())
    } else {
        Err(SectionError::UnverifiedVersion {
            version: version.to_owned(),
        })
    }
}

/// Decodes the application section from an already reconstructed MAIN image.
///
/// `image` must be `main.image()`; this lets callers that already built the image (for example
/// during a budgeted summary) avoid building it again.
///
/// # Errors
///
/// [`SectionError::UnverifiedVersion`], [`SectionError::ImageBase`] if the image does not start
/// at address 0, or any [`decode_section`] error.
pub fn decode_main_image(
    main: &UpdDocument,
    image: &DocumentImage,
) -> Result<DecodedSection, SectionError> {
    verify_main_version(main)?;
    if image.base() != 0 {
        return Err(SectionError::ImageBase { base: image.base() });
    }
    decode_section(image.bytes(), APPLICATION_SECTION_OFFSET)
}

/// Finds the single MAIN document, reconstructs its image, and decodes the application section.
///
/// # Errors
///
/// Any [`main_document`] or [`decode_main_image`] error, or [`SectionError::Image`] with the
/// underlying image error.
pub fn decode_application(container: &UpdContainer) -> Result<DecodedSection, SectionError> {
    let main = main_document(container)?;
    verify_main_version(main)?;
    let image = main.image()?;
    decode_main_image(main, &image)
}

fn is_xdj700_main(document: &UpdDocument) -> bool {
    document.descriptor().model() == "XDJ-700" && document.descriptor().kind() == "MAIN"
}
