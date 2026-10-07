//! Rebuilds a complete XDJ-700 `.UPD` around a new decoded application.
//!
//! A rebuild starts only from a pinned official release ([`StockRelease`]): its loader, PANL and
//! framing are what the output keeps, and its pinned size bound limits the output.
//!
//! Integrity procedure, in order:
//!
//! 1. encode the application section ([`encode_section`], self-checked);
//! 2. MAIN image = the input's loader region `[0, APPLICATION_SECTION_OFFSET)` followed by the
//!    section. The input's trailing `0xFF` padding is not carried over;
//! 3. re-cut MAIN into [`RECORD_DATA_LEN`](super::RECORD_DATA_LEN)-byte S2 records over the
//!    input's extents, the last extent ending at the new image end;
//! 4. set the declared version label; keep the S0 header, termination and every other document;
//! 5. recompute the document CRCs and the length header (the canonical writer).
//!
//! The output is then re-parsed and checked by the same verification [`verify_rebuild`] runs.

use super::grid::{S2_ADDRESS_SPACE, follows_grid, grid_records};
use super::{
    APPLICATION_SECTION_OFFSET, decode_section, encode_section, main_document, section_frame,
    verify_main_version,
};
use crate::error::{RebuildCheck, RebuildError, SectionError};
use crate::identity::sha256_hex;
use crate::upd::{
    self, DocumentImage, DocumentParts, GAP_FILL, UpdContainer, UpdDocument, parse_upd,
    verify_roundtrip,
};
use std::ops::Range;

/// Most a rebuilt MAIN image may grow beyond the official one. Where the device keeps its
/// fallback updater, and how large the application flash region is, are unconfirmed (ambiguity
/// A3), so growth is bounded. The hardware-tested reference alpha.2 build grows by 44,399 bytes.
/// Raise this only with evidence about the flash layout.
pub const MAX_MAIN_GROWTH: usize = 256 * 1024;

/// An official release a rebuild may start from.
///
/// The library guarantees that the input matches the release passed in. The fields are public so
/// that tests can pin synthetic files, so the flash-safety bound ([`MAX_MAIN_GROWTH`] over the
/// official image) holds only when callers use a pinned constant such as [`OFFICIAL_V115`].
/// Production code, including the CLI, must never construct a release from user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockRelease<'a> {
    /// SHA-256 of the complete official `.UPD` (lowercase hex). Only this exact file is accepted
    /// as input, so the loader, PANL and framing a rebuild keeps are the vendor's own.
    pub upd_sha256: &'a str,
    /// Largest MAIN image (loader plus section) a rebuild of this release may produce.
    pub max_main_image_len: usize,
}

/// Official XDJ-700 v1.15 (`XDJ700.UPD`, 17,371,335 bytes; MAIN image 7,251,904 bytes).
pub const OFFICIAL_V115: StockRelease<'static> = StockRelease {
    upd_sha256: "73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c",
    max_main_image_len: 0x6E_A7C0 + MAX_MAIN_GROWTH,
};

const SECTION_FRAMING_LEN: usize = 4 + 2;

/// A rebuilt update that passed verification against its input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuiltUpdate {
    bytes: Vec<u8>,
    sha256: String,
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

    /// SHA-256 of [`Self::bytes`], lowercase hex (computed once).
    pub fn sha256(&self) -> &str {
        &self.sha256
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

/// Rebuilds `input`, which must be the `release` file, so that its application decodes to
/// `decoded` and its MAIN version label reads `version`.
///
/// # Errors
///
/// - [`RebuildError::InvalidVersionLabel`] unless `version` looks like `Ver1.22`.
/// - Input refusals: [`RebuildError::UnpinnedInput`], [`RebuildError::Input`],
///   [`RebuildError::InputSection`], [`RebuildError::DataAfterSection`] or
///   [`RebuildError::NonCanonicalRecordLayout`].
/// - [`RebuildError::Encode`] if `decoded` cannot be encoded, or [`RebuildError::ImageTooLarge`]
///   beyond the release's bound (and 24-bit addresses).
/// - Any [`verify_rebuild`] error, which would indicate a defect in this library.
pub fn rebuild_with_application(
    input: &[u8],
    release: &StockRelease<'_>,
    decoded: &[u8],
    version: &str,
) -> Result<RebuiltUpdate, RebuildError> {
    check_version_label(version)?;
    let stock = StockMain::load(input, release)?;
    let main = stock.main();

    let section = encode_section(decoded).map_err(RebuildError::Encode)?;
    let mut image = stock.image.bytes()[..APPLICATION_SECTION_OFFSET].to_vec();
    image.extend_from_slice(&section);
    stock.check_size(image.len())?;
    // `load` guarantees the extents fit any image longer than the loader region within 24-bit
    // addresses; if not, the output is what is wrong, so report it as such.
    let records = grid_records(&image, &stock.extents)
        .ok_or(RebuildError::Verification(RebuildCheck::RecordLayout))?;
    let descriptor = main
        .descriptor()
        .with_version(version)
        .expect("check_version_label guarantees a 7-byte printable label");

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
        sha256: sha256_hex(&bytes),
        bytes,
        main_image_len: verified.len,
        main_image_sha256: verified.sha256,
    })
}

/// Checks that `output` is a rebuild of `input` (the `release` file) whose application decodes
/// to `decoded` and whose MAIN version label is `version`, independently of how `output` was
/// produced.
///
/// Every document other than MAIN must be byte-identical to the input's. MAIN must keep the
/// input's descriptor (except the version), S0 header, termination and loader region, follow the
/// input's record grid, stay within the release's size bound, and end exactly at the end of a
/// valid application section.
///
/// # Errors
///
/// The input refusals of [`rebuild_with_application`], [`RebuildError::OutputUnparseable`],
/// [`RebuildError::OutputSection`] if the output's MAIN image cannot be built
/// ([`SectionError::Image`]) or its section is invalid, [`RebuildError::ImageTooLarge`], or
/// [`RebuildError::Verification`] naming the failed property.
pub fn verify_rebuild(
    input: &[u8],
    release: &StockRelease<'_>,
    output: &[u8],
    decoded: &[u8],
    version: &str,
) -> Result<(), RebuildError> {
    check_version_label(version)?;
    StockMain::load(input, release)?
        .verify(input, output, decoded, version)
        .map(|_| ())
}

/// Identity of a verified output's MAIN image.
struct VerifiedMain {
    len: usize,
    sha256: String,
}

/// The verified input: its container, single MAIN document, image, extents and size bound.
struct StockMain {
    container: UpdContainer,
    main_position: usize,
    image: DocumentImage,
    extents: Vec<Range<u64>>,
    max_image_len: usize,
}

impl StockMain {
    fn load(input: &[u8], release: &StockRelease<'_>) -> Result<Self, RebuildError> {
        let sha256 = sha256_hex(input);
        if sha256 != release.upd_sha256 {
            return Err(RebuildError::UnpinnedInput { sha256 });
        }
        let container = verify_roundtrip(input).map_err(RebuildError::Input)?;
        let main = main_document(&container).map_err(RebuildError::InputSection)?;
        verify_main_version(main).map_err(RebuildError::InputSection)?;
        let image = main
            .image()
            .map_err(|error| RebuildError::InputSection(SectionError::Image(error)))?;
        if image.base() != 0 {
            let base = image.base();
            return Err(RebuildError::InputSection(SectionError::ImageBase { base }));
        }
        // Framing only: the stock application itself is never used, so it is not decompressed.
        let frame = section_frame(image.bytes(), APPLICATION_SECTION_OFFSET)
            .map_err(RebuildError::InputSection)?;
        if image.bytes()[frame.end()..]
            .iter()
            .any(|&byte| byte != GAP_FILL)
        {
            return Err(RebuildError::DataAfterSection);
        }
        let extents = main.data_extents();
        let in_order = extents
            .last()
            .is_some_and(|last| last.start <= APPLICATION_SECTION_OFFSET as u64);
        if !in_order || !follows_grid(image.bytes(), &extents, main.data_records()) {
            return Err(RebuildError::NonCanonicalRecordLayout);
        }
        let main_position = main.index();
        Ok(Self {
            container,
            main_position,
            image,
            extents,
            max_image_len: release.max_main_image_len.min(S2_ADDRESS_SPACE),
        })
    }

    fn main(&self) -> &UpdDocument {
        &self.container.documents()[self.main_position]
    }

    fn check_size(&self, len: usize) -> Result<(), RebuildError> {
        if len > self.max_image_len {
            return Err(RebuildError::ImageTooLarge {
                len,
                limit: self.max_image_len,
            });
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
        let on_grid =
            image.base() == 0 && follows_grid(image.bytes(), &self.extents, new.data_records());
        if !on_grid {
            return failed(RebuildCheck::RecordLayout);
        }
        let loader = ..APPLICATION_SECTION_OFFSET;
        if image.bytes().get(loader) != Some(&self.image.bytes()[loader]) {
            return failed(RebuildCheck::Loader);
        }
        self.check_size(image.bytes().len())?;
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
        Err(RebuildError::InvalidVersionLabel {
            label: version.to_owned(),
        })
    }
}
