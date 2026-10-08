//! The verified input of a rebuild ([`StockMain`]) and the verification every rebuilt output
//! passes ([`StockMain::verify`]).

use super::grid::{S2_ADDRESS_SPACE, follows_grid};
use super::release::{StockRelease, VersionBlock};
use super::{
    APPLICATION_SECTION_OFFSET, DecodedSection, decode_section, main_document, reported_version_at,
    section_frame, verify_main_layout,
};
use crate::error::{RebuildCheck, RebuildError, SectionError};
use crate::identity::sha256_hex;
use crate::upd::{DocumentImage, GAP_FILL, UpdContainer, UpdDocument, parse_upd, verify_roundtrip};
use std::ops::Range;

const SECTION_FRAMING_LEN: usize = 4 + 2;

/// Identity of a verified output's MAIN image.
pub(super) struct VerifiedMain {
    pub(super) len: usize,
    pub(super) sha256: String,
    pub(super) application_sha256: String,
    /// At the release's version block; `None` without one.
    pub(super) reported_version: Option<String>,
}

/// The verified input: its container, single MAIN document, image, extents, size bound and
/// version rule.
pub(super) struct StockMain<'a> {
    pub(super) container: UpdContainer,
    pub(super) main_position: usize,
    pub(super) image: DocumentImage,
    pub(super) extents: Vec<Range<u64>>,
    pub(super) max_image_len: usize,
    pub(super) version_block: Option<VersionBlock<'a>>,
}

impl<'a> StockMain<'a> {
    pub(super) fn load(input: &[u8], release: &StockRelease<'a>) -> Result<Self, RebuildError> {
        let sha256 = sha256_hex(input);
        if input.len() != release.upd_len || sha256 != release.upd_sha256 {
            return Err(RebuildError::UnpinnedInput { sha256 });
        }
        let container = verify_roundtrip(input).map_err(RebuildError::Input)?;
        let main = main_document(&container).map_err(RebuildError::InputSection)?;
        let image = main
            .image()
            .map_err(|error| RebuildError::InputSection(SectionError::Image(error)))?;
        verify_main_layout(main, &image).map_err(RebuildError::InputSection)?;
        if image.base() != 0 {
            let base = image.base();
            return Err(RebuildError::InputSection(SectionError::ImageBase { base }));
        }
        // Framing only here; `application` decodes the stock application for callers that need it.
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
            version_block: release.version_block,
        })
    }

    /// The stock application, decoded from the verified input.
    pub(super) fn application(&self) -> Result<DecodedSection, RebuildError> {
        decode_section(self.image.bytes(), APPLICATION_SECTION_OFFSET)
            .map_err(RebuildError::InputSection)
    }

    pub(super) fn main(&self) -> &UpdDocument {
        &self.container.documents()[self.main_position]
    }

    pub(super) fn check_size(&self, len: usize) -> Result<(), RebuildError> {
        if len > self.max_image_len {
            return Err(RebuildError::ImageTooLarge {
                len,
                limit: self.max_image_len,
            });
        }
        Ok(())
    }

    pub(super) fn verify(
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
        // Every public rebuild and verification passes here: the one place the rule is enforced.
        let application_sha256 = section.decoded_sha256();
        if let Some(block) = &self.version_block {
            block.check_application_hashed(decoded, &application_sha256)?;
        }
        Ok(VerifiedMain {
            len: image.bytes().len(),
            sha256: sha256_hex(image.bytes()),
            application_sha256,
            reported_version: self
                .version_block
                .and_then(|block| reported_version_at(decoded, block.offset))
                .map(str::to_owned),
        })
    }
}

fn section_end(compressed_len: usize) -> usize {
    APPLICATION_SECTION_OFFSET + SECTION_FRAMING_LEN + compressed_len
}

fn document_bytes<'a>(container_bytes: &'a [u8], document: &UpdDocument) -> &'a [u8] {
    &container_bytes[document.offset()..document.offset() + document.length()]
}
