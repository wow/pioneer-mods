//! XDJ-700 specific layout inside the MAIN document image.
//!
//! The MAIN image holds the compressed application section at [`APPLICATION_SECTION_OFFSET`]:
//!
//! ```text
//! [u32 LE size][size bytes: LZSS stream starting with the section prefix][u16 LE checksum]
//! ```
//!
//! The checksum is the 16-bit sum of the size field and every compressed byte. The boot loader
//! verifies it and, on a mismatch, runs the fallback updater at [`FALLBACK_SECTION_OFFSET`]
//! instead (static analysis of the v1.15 loader).

mod app_version;
mod grid;
mod label;
mod rebuild;

pub use app_version::{
    VERSION_STRING_OFFSET, reported_version, validate_reported_version, with_reported_version,
};
pub use grid::RECORD_DATA_LEN;
pub use label::{OFFICIAL_V115_LABEL, is_label_higher, validate_version_label};
pub use rebuild::{
    MAX_MAIN_GROWTH, OFFICIAL_V115, RebuiltUpdate, StockRelease, rebuild_with_application,
    rebuild_with_stock_application, verify_rebuild,
};

use crate::error::SectionError;
use crate::identity::sha256_hex;
use crate::lzss;
use crate::upd::{DocumentImage, UpdContainer, UpdDocument};

/// Offset of the compressed application section inside the MAIN image.
pub const APPLICATION_SECTION_OFFSET: usize = 0x40000;

/// Offset of the loader's fallback updater section inside the MAIN image (v1.15). It has the
/// application section's format. Static analysis of the loader shows it decompresses and runs
/// this section instead of the application when the application section's checksum does not
/// match, for example after an update interrupted while the application was being written (not
/// yet observed on hardware). It lies in the loader region that every rebuild keeps
/// byte-identical.
pub const FALLBACK_SECTION_OFFSET: usize = 0x10000;

/// MAIN versions whose application-section layout has been verified against an official file.
/// An image with another label is decoded only if its loader region matches
/// [`VERIFIED_LOADER_SHA256`]; otherwise it is refused rather than decoded at a guessed offset.
pub const VERIFIED_MAIN_VERSIONS: &[&str] = &[OFFICIAL_V115_LABEL];

/// SHA-256 of the loader region `[0, APPLICATION_SECTION_OFFSET)` of the verified official MAIN
/// images (v1.15). The loader code fixes where the application section starts, so an image whose
/// loader region is byte-identical to a verified one has the verified layout whatever its label.
/// Every rebuild keeps this region byte-identical, so rebuilt files with a higher label (which
/// the updater needs) are decoded too.
pub const VERIFIED_LOADER_SHA256: &[&str] =
    &["ff211e68ba533f1508234d9f92c435b4a4943370a079589881609e528bf0943c"];

/// Start of the stock section stream; see [`crate::lzss::SECTION_TAG`]. [`decode_section`] checks
/// only its invariant part, because flag bits 2..=7 belong to the data that follows.
pub use crate::lzss::SECTION_TAG;

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
    let frame = section_frame(image, offset)?;
    let decoded = lzss::decode(&image[frame.stream.clone()], MAX_DECODED_LEN)?;

    Ok(DecodedSection {
        offset,
        compressed_len: frame.stream.len(),
        checksum: frame.checksum,
        decoded,
    })
}

/// A section whose framing is verified (bounds, checksum, tag) but whose stream is not decoded.
pub(crate) struct SectionFrame {
    /// Image range of the compressed stream (after the size field, before the checksum).
    pub(crate) stream: std::ops::Range<usize>,
    pub(crate) checksum: u16,
}

impl SectionFrame {
    /// Image offset just past the checksum.
    pub(crate) fn end(&self) -> usize {
        self.stream.end + CHECKSUM_LEN
    }
}

/// Every check of [`decode_section`] except decompression.
pub(crate) fn section_frame(image: &[u8], offset: usize) -> Result<SectionFrame, SectionError> {
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
    Ok(SectionFrame {
        stream: data_start..data_end,
        checksum: stored,
    })
}

/// Worst-case section stream for [`MAX_DECODED_LEN`] input: the tag, every byte a literal, and
/// one flag byte per 8 items. It fits the `u32` size field with a wide margin.
const MAX_STREAM_LEN: usize = SECTION_TAG.len() + MAX_DECODED_LEN + MAX_DECODED_LEN.div_ceil(8);
const _: () = assert!(MAX_STREAM_LEN <= u32::MAX as usize);

/// Encodes `decoded` into complete section bytes (`[u32 LE size][stream][u16 LE checksum]`).
///
/// Self-checked with [`verify_encoded_section`] before returning.
///
/// # Errors
///
/// [`SectionError::DecodedTooLarge`] above [`MAX_DECODED_LEN`] (the most a section may decode
/// to), [`SectionError::Encode`] if the input cannot be encoded (for example a missing 19-byte
/// zero prefix), or the self-check errors of [`verify_encoded_section`].
pub fn encode_section(decoded: &[u8]) -> Result<Vec<u8>, SectionError> {
    if decoded.len() > MAX_DECODED_LEN {
        return Err(SectionError::DecodedTooLarge {
            len: decoded.len(),
            limit: MAX_DECODED_LEN,
        });
    }
    let mut bytes = vec![0; SIZE_FIELD_LEN];
    lzss::encode_section_stream_into(decoded, &mut bytes).map_err(SectionError::Encode)?;
    let size = u32::try_from(bytes.len() - SIZE_FIELD_LEN)
        .expect("stream is at most MAX_STREAM_LEN, which fits u32");
    bytes[..SIZE_FIELD_LEN].copy_from_slice(&size.to_le_bytes());
    let checksum = section_checksum(&bytes);
    bytes.extend_from_slice(&checksum.to_le_bytes());

    verify_encoded_section(&bytes, decoded)?;
    Ok(bytes)
}

/// The writer self-check of [`encode_section`]: `bytes` must be a complete section (as
/// [`decode_section`] reads it at offset 0) that decodes to exactly `decoded`.
///
/// # Errors
///
/// [`SectionError::EncodeSelfCheckDecode`] (with the decode error) if `bytes` is not a valid
/// section, or [`SectionError::EncodeSelfCheckMismatch`] if it decodes to different bytes.
pub fn verify_encoded_section(bytes: &[u8], decoded: &[u8]) -> Result<(), SectionError> {
    let section = decode_section(bytes, 0)
        .map_err(|error| SectionError::EncodeSelfCheckDecode(Box::new(error)))?;
    if section.decoded() != decoded {
        return Err(SectionError::EncodeSelfCheckMismatch);
    }
    Ok(())
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

/// Refuses a MAIN image whose application-section layout has not been verified: its version is
/// not in [`VERIFIED_MAIN_VERSIONS`] and its loader region does not match
/// [`VERIFIED_LOADER_SHA256`].
///
/// `image` must be `main.image()`.
///
/// # Errors
///
/// [`SectionError::UnverifiedVersion`] if neither the version nor the loader region is verified.
pub fn verify_main_layout(main: &UpdDocument, image: &DocumentImage) -> Result<(), SectionError> {
    verify_main_layout_with(main, image, VERIFIED_LOADER_SHA256)
}

/// [`verify_main_layout`] against the given loader-region identities (lowercase SHA-256 hex of
/// `[0, APPLICATION_SECTION_OFFSET)` in an image based at 0) instead of
/// [`VERIFIED_LOADER_SHA256`].
///
/// # Errors
///
/// [`SectionError::UnverifiedVersion`] if neither the version nor the loader region is verified.
pub fn verify_main_layout_with(
    main: &UpdDocument,
    image: &DocumentImage,
    verified_loaders: &[&str],
) -> Result<(), SectionError> {
    let version = main.descriptor().version();
    let loader_verified = image.base() == 0
        && image
            .bytes()
            .get(..APPLICATION_SECTION_OFFSET)
            .is_some_and(|loader| verified_loaders.contains(&sha256_hex(loader).as_str()));
    if VERIFIED_MAIN_VERSIONS.contains(&version) || loader_verified {
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
    verify_main_layout(main, image)?;
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
    let image = main.image()?;
    decode_main_image(main, &image)
}

fn is_xdj700_main(document: &UpdDocument) -> bool {
    document.descriptor().model() == "XDJ-700" && document.descriptor().kind() == "MAIN"
}
