//! A synthetic release and recipes for the schema-v2 engine tests (no vendor bytes).

use super::xdj700::{release, stock_update};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, RecipeChecks, RecipeError, RecipeTarget, StockRelease,
    VERSION_STRING_OFFSET, VersionBlock, apply_recipe_v2_to, decode_section, main_document,
};
use patch_core::{parse_upd, sha256_hex};
use patch_schema::{Precondition, RecipeV2, Replacement, TargetV2};
use std::ops::Range;

pub const PROTECTED: &[Range<usize>] = &[Range {
    start: 0,
    end: 0x800,
}];

/// Where the synthetic application has zero padding.
pub const PADDING: Range<usize> = Range {
    start: 0xc00,
    end: 0xc40,
};

/// A synthetic stock application: the zero seed, a pattern with zero padding at [`PADDING`], and
/// `1.15` at the version offset.
pub fn stock_application() -> Vec<u8> {
    let mut decoded: Vec<u8> = (0..0x1000u32).map(|i| (i * 7 + 3) as u8).collect();
    decoded[..19].fill(0);
    decoded[PADDING].fill(0);
    decoded[VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + 5].copy_from_slice(b"1.15\0");
    decoded
}

/// The synthetic stock update, its SHA-256, and the stock application's SHA-256.
pub struct Fixture {
    pub update: Vec<u8>,
    pub upd_sha256: String,
    pub application_sha256: String,
}

impl Fixture {
    pub fn new() -> Self {
        let application = stock_application();
        let update = stock_update(&application);
        Self {
            upd_sha256: sha256_hex(&update),
            application_sha256: sha256_hex(&application),
            update,
        }
    }

    pub fn target(&self) -> RecipeTarget<'_> {
        RecipeTarget {
            id: "synthetic",
            release: StockRelease {
                version_block: Some(VersionBlock {
                    offset: VERSION_STRING_OFFSET,
                    stock_version: "1.15",
                    stock_application_sha256: &self.application_sha256,
                }),
                ..release(self.update.len(), &self.upd_sha256)
            },
            application_len: 0x1000,
            load_address: 0x0800_0000,
            protected: PROTECTED,
        }
    }

    pub fn recipe(&self, replacements: Vec<Replacement>) -> RecipeV2 {
        RecipeV2 {
            schema_version: 2,
            recipe_id: "synthetic-test".to_owned(),
            description: "test".to_owned(),
            target: TargetV2 {
                release: "synthetic".to_owned(),
                upd_sha256: self.upd_sha256.clone(),
                application_sha256: self.application_sha256.clone(),
            },
            label: "Ver1.16".to_owned(),
            reported_version: "0.10".to_owned(),
            replacements,
            image_edits: Vec::new(),
            expected: None,
        }
    }

    pub fn apply(&self, recipe: &RecipeV2) -> Result<Vec<u8>, RecipeError> {
        let rebuilt = apply_recipe_v2_to(recipe, &self.target(), &self.update, RecipeChecks::NONE)?;
        let parsed = parse_upd(rebuilt.bytes()).expect("parse");
        let image = main_document(&parsed)
            .expect("main")
            .image()
            .expect("image");
        // The synthetic loader is not a verified one, so decode the section directly.
        let section = decode_section(image.bytes(), APPLICATION_SECTION_OFFSET).expect("decode");
        Ok(section.decoded().to_vec())
    }
}

/// A replacement of `bytes` at `offset`. Its precondition covers the span and 32 stock bytes
/// around it (8 before it, fewer near the protected header and more near the end), identified by
/// their SHA-256.
pub fn replacement(offset: usize, bytes: &[u8]) -> Replacement {
    let stock = stock_application();
    let len = bytes.len() + 32;
    let start = if offset >= 0x808 { offset - 8 } else { offset }.min(stock.len() - len);
    let (before, after) = (offset - start, len - (offset - start) - bytes.len());
    Replacement {
        offset: offset as u64,
        bytes_hex: bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        precondition: Precondition {
            before: before as u64,
            after: after as u64,
            sha256: sha256_hex(&stock[start..start + len]),
        },
        purpose: "test".to_owned(),
    }
}

/// [`replacement`] with a window of `before` stock bytes before the span and `after` after it.
pub fn windowed(offset: usize, bytes: &[u8], before: usize, after: usize) -> Replacement {
    let stock = stock_application();
    let mut windowed = replacement(offset, bytes);
    windowed.precondition = Precondition {
        before: before as u64,
        after: after as u64,
        sha256: sha256_hex(&stock[offset - before..offset + bytes.len() + after]),
    };
    windowed
}
