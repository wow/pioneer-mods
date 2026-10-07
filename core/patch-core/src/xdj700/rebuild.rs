//! Rebuilds a complete XDJ-700 `.UPD` around a new decoded application.
//!
//! Integrity procedure, in order:
//!
//! 1. encode the application section ([`encode_section`], self-checked);
//! 2. MAIN image = the input's loader region `[0, APPLICATION_SECTION_OFFSET)` followed by the
//!    section. The input's trailing `0xFF` padding is not carried over;
//! 3. re-cut MAIN into [`RECORD_DATA_LEN`]-byte S2 records over the input's extents, the last
//!    extent ending at the new image end;
//! 4. set the declared version label; keep the S0 header, termination and every other document;
//! 5. recompute the document CRCs and the length header (the canonical writer).
//!
//! The output is then re-parsed and checked by the same verification [`verify_rebuild`] runs.

use super::{
    APPLICATION_SECTION_OFFSET, decode_main_image, decode_section, encode_section, main_document,
    verify_main_version,
};
use crate::error::{RebuildCheck, RebuildError, SectionError};
use crate::identity::sha256_hex;
use crate::upd::{
    self, DocumentImage, DocumentParts, GAP_FILL, SRecord, SRecordType, UpdContainer, UpdDocument,
    parse_upd, verify_roundtrip,
};
use std::ops::Range;

/// Data bytes per MAIN S2 record, as in the official update.
pub const RECORD_DATA_LEN: usize = 32;
/// Most a rebuilt MAIN image may grow beyond the input's image. Where the device keeps its fallback
/// updater, and how large the application flash region is, are unconfirmed (ambiguity A3), so
/// growth is bounded. The hardware-tested reference alpha.2 build grows by 44,399 bytes. Raise
/// this only with evidence about the flash layout.
pub const MAX_MAIN_GROWTH: usize = 256 * 1024;
/// S2 records carry 24-bit addresses.
const S2_ADDRESS_SPACE: usize = 1 << 24;
const SECTION_FRAMING_LEN: usize = 4 + 2;

/// A rebuilt update that passed verification against its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuiltUpdate {
    bytes: Vec<u8>,
    main_image_len: usize,
    main_image_sha256: String,
}

impl RebuiltUpdate {
    /// The verified `.UPD` bytes.
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// The verified `.UPD` bytes, without copying.
    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// SHA-256 of [`Self::bytes`], lowercase hex.
    pub fn sha256(&self) -> String {
        sha256_hex(&self.bytes)
    }

    /// Length of the MAIN image (loader plus section), as re-parsed from the verified bytes.
    pub fn main_image_len(&self) -> usize {
        self.main_image_len
    }

    /// SHA-256 of the MAIN image, as re-parsed from the verified bytes.
    pub fn main_image_sha256(&self) -> &str {
        &self.main_image_sha256
    }
}

/// Rebuilds `input` (an official XDJ-700 update) so that its application decodes to `decoded`
/// and its MAIN version label reads `version`.
///
/// # Errors
///
/// - [`RebuildError::InvalidVersionLabel`] unless `version` looks like `Ver1.22`.
/// - Input refusals: [`RebuildError::Input`], [`RebuildError::Section`] (no single MAIN document,
///   unverified version, invalid stock section), [`RebuildError::DataAfterSection`] or
///   [`RebuildError::NonCanonicalRecordLayout`].
/// - [`RebuildError::Section`] with [`SectionError::Encode`] or a section self-check error if
///   `decoded` cannot be encoded, [`RebuildError::ImageTooLarge`] beyond 24-bit addresses, or
///   [`RebuildError::ImageGrowthTooLarge`] beyond [`MAX_MAIN_GROWTH`].
/// - Any [`verify_rebuild`] error, which would indicate a defect in this library.
pub fn rebuild_with_application(
    input: &[u8],
    decoded: &[u8],
    version: &str,
) -> Result<RebuiltUpdate, RebuildError> {
    check_version_label(version)?;
    let stock = StockMain::load(input)?;
    let main = stock.main();

    let section = encode_section(decoded)?;
    let mut image = stock.image.bytes()[..APPLICATION_SECTION_OFFSET].to_vec();
    image.extend_from_slice(&section);
    if image.len() > S2_ADDRESS_SPACE {
        return Err(RebuildError::ImageTooLarge { len: image.len() });
    }
    stock.check_growth(image.len())?;
    let records =
        grid_records(&image, &stock.extents).ok_or(RebuildError::NonCanonicalRecordLayout)?;
    let descriptor = main
        .descriptor()
        .with_version(version)
        .ok_or_else(|| invalid_label(version))?;

    let parts: Vec<DocumentParts<'_>> = stock
        .container
        .documents()
        .iter()
        .map(|document| {
            if document.index() == main.index() {
                DocumentParts {
                    descriptor: &descriptor,
                    header: main.header(),
                    data: &records,
                    termination: main.termination(),
                }
            } else {
                document.parts()
            }
        })
        .collect();
    let bytes = upd::encode_parts(&parts);

    let verified = stock.verify(input, &bytes, decoded, version)?;
    Ok(RebuiltUpdate {
        bytes,
        main_image_len: verified.len,
        main_image_sha256: verified.sha256,
    })
}

/// Checks that `output` is a rebuild of `input` whose application decodes to `decoded` and whose
/// MAIN version label is `version`, independently of how `output` was produced.
///
/// Every document other than MAIN must be byte-identical to the input's. MAIN must keep the
/// input's descriptor (except the version), S0 header, termination and loader region, follow the
/// input's record grid, grow by at most [`MAX_MAIN_GROWTH`], and end exactly at the end of a
/// valid application section.
///
/// # Errors
///
/// The input refusals of [`rebuild_with_application`], [`RebuildError::OutputUnparseable`],
/// [`RebuildError::OutputSection`] if the output's MAIN image cannot be built
/// ([`SectionError::Image`]) or its section is invalid, [`RebuildError::ImageGrowthTooLarge`], or
/// [`RebuildError::Verification`] naming the failed property.
pub fn verify_rebuild(
    input: &[u8],
    output: &[u8],
    decoded: &[u8],
    version: &str,
) -> Result<(), RebuildError> {
    check_version_label(version)?;
    StockMain::load(input)?
        .verify(input, output, decoded, version)
        .map(|_| ())
}

/// Identity of a verified output's MAIN image.
struct VerifiedMain {
    len: usize,
    sha256: String,
}

/// The verified input: its container, single MAIN document, image and record extents.
struct StockMain {
    container: UpdContainer,
    main_position: usize,
    image: DocumentImage,
    extents: Vec<Range<u64>>,
}

impl StockMain {
    fn load(input: &[u8]) -> Result<Self, RebuildError> {
        let container = verify_roundtrip(input).map_err(RebuildError::Input)?;
        let main = main_document(&container)?;
        verify_main_version(main)?;
        let image = main.image().map_err(SectionError::Image)?;
        let section = decode_main_image(main, &image)?;
        let section_end = section_end(section.compressed_len());
        if image.bytes()[section_end..]
            .iter()
            .any(|&byte| byte != GAP_FILL)
        {
            return Err(RebuildError::DataAfterSection);
        }
        let extents = main.data_extents();
        let in_order = extents
            .last()
            .is_some_and(|last| last.start <= APPLICATION_SECTION_OFFSET as u64);
        if !in_order
            || grid_records(image.bytes(), &extents).as_deref() != Some(main.data_records())
        {
            return Err(RebuildError::NonCanonicalRecordLayout);
        }
        let main_position = main.index();
        Ok(Self {
            container,
            main_position,
            image,
            extents,
        })
    }

    fn main(&self) -> &UpdDocument {
        &self.container.documents()[self.main_position]
    }

    fn check_growth(&self, len: usize) -> Result<(), RebuildError> {
        let limit = self.image.bytes().len() + MAX_MAIN_GROWTH;
        if len > limit {
            return Err(RebuildError::ImageGrowthTooLarge { len, limit });
        }
        Ok(())
    }

    fn verify(
        &self,
        input: &[u8],
        output: &[u8],
        decoded: &[u8],
        version: &str,
    ) -> Result<VerifiedMain, RebuildError> {
        let failed = |check| Err(RebuildError::Verification(check));
        let rebuilt = parse_upd(output).map_err(RebuildError::OutputUnparseable)?;
        if rebuilt.documents().len() != self.container.documents().len() {
            return failed(RebuildCheck::DocumentCount);
        }
        for (old, new) in self.container.documents().iter().zip(rebuilt.documents()) {
            if old.index() != self.main_position
                && document_bytes(input, old) != document_bytes(output, new)
            {
                return failed(RebuildCheck::UntouchedDocument { index: old.index() });
            }
        }

        let (old, new) = (self.main(), &rebuilt.documents()[self.main_position]);
        let framing_kept = new
            .descriptor()
            .with_version(old.descriptor().version())
            .as_ref()
            == Some(old.descriptor())
            && new.descriptor().version() == version
            && new.header() == old.header()
            && new.termination() == old.termination();
        if !framing_kept {
            return failed(RebuildCheck::MainFraming);
        }

        let image = new
            .image()
            .map_err(|error| RebuildError::OutputSection(SectionError::Image(error)))?;
        // The grid comparison already implies base 0 (the input's first extent starts at 0); the
        // explicit check states the precondition for indexing the image by absolute address.
        let on_grid = image.base() == 0
            && grid_records(image.bytes(), &self.extents).as_deref() == Some(new.data_records());
        if !on_grid {
            return failed(RebuildCheck::RecordLayout);
        }
        let loader = ..APPLICATION_SECTION_OFFSET;
        if image.bytes().get(loader) != Some(&self.image.bytes()[loader]) {
            return failed(RebuildCheck::Loader);
        }
        self.check_growth(image.bytes().len())?;
        let section = decode_section(image.bytes(), APPLICATION_SECTION_OFFSET)
            .map_err(RebuildError::OutputSection)?;
        if image.bytes().len() != section_end(section.compressed_len()) {
            return failed(RebuildCheck::TrailingData);
        }
        if section.decoded() != decoded {
            return failed(RebuildCheck::Application);
        }
        Ok(VerifiedMain {
            len: image.bytes().len(),
            sha256: sha256_hex(image.bytes()),
        })
    }
}

/// Cuts `image` (based at 0) into S2 records over `extents`, the last extent ending at the image
/// end, or `None` if the extents do not fit the image.
fn grid_records(image: &[u8], extents: &[Range<u64>]) -> Option<Vec<SRecord>> {
    let mut records = Vec::with_capacity(image.len() / RECORD_DATA_LEN + extents.len());
    for (position, extent) in extents.iter().enumerate() {
        let start = usize::try_from(extent.start).ok()?;
        let end = if position + 1 == extents.len() {
            image.len()
        } else {
            usize::try_from(extent.end).ok()?
        };
        if start >= end || end > image.len() || end > S2_ADDRESS_SPACE {
            return None;
        }
        for address in (start..end).step_by(RECORD_DATA_LEN) {
            let stop = (address + RECORD_DATA_LEN).min(end);
            let data = image[address..stop].to_vec();
            records.push(SRecord::data_record(SRecordType::S2, address as u32, data));
        }
    }
    Some(records)
}

fn section_end(compressed_len: usize) -> usize {
    APPLICATION_SECTION_OFFSET + SECTION_FRAMING_LEN + compressed_len
}

fn document_bytes<'a>(container_bytes: &'a [u8], document: &UpdDocument) -> &'a [u8] {
    &container_bytes[document.offset()..document.offset() + document.length()]
}

/// XDJ-700 MAIN labels have the form `VerX.YY`.
fn check_version_label(version: &str) -> Result<(), RebuildError> {
    let bytes = version.as_bytes();
    let valid = matches!(bytes, [b'V', b'e', b'r', major, b'.', minor @ ..]
        if major.is_ascii_digit() && minor.len() == 2 && minor.iter().all(u8::is_ascii_digit));
    if valid {
        Ok(())
    } else {
        Err(invalid_label(version))
    }
}

fn invalid_label(version: &str) -> RebuildError {
    RebuildError::InvalidVersionLabel {
        label: version.to_owned(),
    }
}
