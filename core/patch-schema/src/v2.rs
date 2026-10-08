//! Recipe schema v2: changes to the **decoded application** of a pinned official release, rebuilt
//! into a complete, installable update.
//!
//! A v2 recipe never carries vendor bytes. Each replaced span is identified by the SHA-256 of the
//! original bytes (a precondition), and only the project's own replacement bytes are written. The
//! static checks here need no firmware; the release-specific rules (protected ranges, label and
//! reported-version order, preconditions) are enforced by the engine in `patch-core`.

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The only `schema_version` this module accepts.
pub const SCHEMA_VERSION_V2: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecipeV2 {
    pub schema_version: u32,
    pub recipe_id: String,
    pub description: String,
    pub target: TargetV2,
    /// MAIN version label of the output (`VerX.YY`), higher than the release's own version so
    /// that the updater writes it.
    pub label: String,
    /// Version the modified application reports about itself (`X.YY`), lower than the release's
    /// own version so that the official update restores it.
    pub reported_version: String,
    /// Same-length replacements in the decoded application, in ascending, non-overlapping order.
    #[serde(default)]
    pub replacements: Vec<Replacement>,
    /// Identities the output must have, when declared.
    #[serde(default)]
    pub expected: Option<ExpectedV2>,
}

/// The pinned release a recipe applies to. Both identities are checked against the engine's own
/// pins, so a typo in `release` cannot select a different file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetV2 {
    /// Release identifier known to the engine, e.g. `xdj700-v1.15`.
    pub release: String,
    /// SHA-256 of the official `.UPD` the recipe was written for.
    pub upd_sha256: String,
    /// SHA-256 of that release's decoded stock application.
    pub application_sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Replacement {
    /// Decoded-application offset of the first replaced byte.
    pub offset: u64,
    /// SHA-256 of the original bytes in `offset..offset + len` (the precondition).
    pub original_sha256: String,
    /// The replacement bytes, as lowercase or uppercase hex; its length is the span length.
    pub bytes_hex: String,
    /// Why this span changes: what the original code does and what the replacement does.
    pub purpose: String,
}

impl Replacement {
    /// Span length in bytes (half the hex length, once validated).
    pub fn len(&self) -> u64 {
        (self.bytes_hex.len() / 2) as u64
    }

    /// Whether the span is empty (never true for a validated recipe).
    pub fn is_empty(&self) -> bool {
        self.bytes_hex.is_empty()
    }

    /// The replacement bytes, or `None` unless `bytes_hex` is valid, non-empty hex.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        decode_hex(&self.bytes_hex)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExpectedV2 {
    /// SHA-256 of the decoded output application.
    #[serde(default)]
    pub application_sha256: Option<String>,
    /// SHA-256 of the complete output `.UPD`.
    #[serde(default)]
    pub upd_sha256: Option<String>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum RecipeV2Error {
    #[error("unsupported schema_version {0}; this is a schema_version=2 recipe parser")]
    SchemaVersion(u32),
    #[error("recipe_id must not be empty")]
    EmptyRecipeId,
    #[error("target.release must not be empty")]
    EmptyRelease,
    #[error("{field} must be 64 hexadecimal characters")]
    InvalidSha256 { field: String },
    #[error("label {0:?} is not of the form VerX.YY")]
    InvalidLabel(String),
    #[error("reported_version {0:?} is not of the form X.YY")]
    InvalidReportedVersion(String),
    #[error("replacements[{index}].bytes_hex must be non-empty hex with an even length")]
    InvalidReplacementBytes { index: usize },
    #[error("replacements[{index}].purpose must not be empty")]
    EmptyPurpose { index: usize },
    #[error("replacements[{index}] starts before the end of replacements[{previous}]")]
    UnorderedOrOverlapping { index: usize, previous: usize },
    #[error("replacements[{index}] extends past the 64-bit offset range")]
    OffsetOverflow { index: usize },
}

impl RecipeV2 {
    /// Static checks that need no firmware.
    ///
    /// # Errors
    ///
    /// The first failed check, as a [`RecipeV2Error`].
    pub fn validate(&self) -> Result<(), RecipeV2Error> {
        if self.schema_version != SCHEMA_VERSION_V2 {
            return Err(RecipeV2Error::SchemaVersion(self.schema_version));
        }
        if self.recipe_id.trim().is_empty() {
            return Err(RecipeV2Error::EmptyRecipeId);
        }
        if self.target.release.trim().is_empty() {
            return Err(RecipeV2Error::EmptyRelease);
        }
        check_sha256("target.upd_sha256", &self.target.upd_sha256)?;
        check_sha256("target.application_sha256", &self.target.application_sha256)?;
        if !self.label.strip_prefix("Ver").is_some_and(is_bare_version) {
            return Err(RecipeV2Error::InvalidLabel(self.label.clone()));
        }
        if !is_bare_version(&self.reported_version) {
            return Err(RecipeV2Error::InvalidReportedVersion(
                self.reported_version.clone(),
            ));
        }
        let mut previous_end: Option<(usize, u64)> = None;
        for (index, replacement) in self.replacements.iter().enumerate() {
            check_sha256(
                &format!("replacements[{index}].original_sha256"),
                &replacement.original_sha256,
            )?;
            if replacement.bytes().is_none() {
                return Err(RecipeV2Error::InvalidReplacementBytes { index });
            }
            if replacement.purpose.trim().is_empty() {
                return Err(RecipeV2Error::EmptyPurpose { index });
            }
            let end = replacement
                .offset
                .checked_add(replacement.len())
                .ok_or(RecipeV2Error::OffsetOverflow { index })?;
            if let Some((previous, previous_end)) = previous_end
                && replacement.offset < previous_end
            {
                return Err(RecipeV2Error::UnorderedOrOverlapping { index, previous });
            }
            previous_end = Some((index, end));
        }
        if let Some(expected) = &self.expected {
            if let Some(sha256) = &expected.application_sha256 {
                check_sha256("expected.application_sha256", sha256)?;
            }
            if let Some(sha256) = &expected.upd_sha256 {
                check_sha256("expected.upd_sha256", sha256)?;
            }
        }
        Ok(())
    }
}

/// Reads only `schema_version`, so a front end can choose the parser for a recipe file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
pub struct SchemaVersionProbe {
    pub schema_version: u32,
}

fn check_sha256(field: &str, value: &str) -> Result<(), RecipeV2Error> {
    if value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        Ok(())
    } else {
        Err(RecipeV2Error::InvalidSha256 {
            field: field.to_owned(),
        })
    }
}

/// `X.YY`: one digit, a dot, two digits.
fn is_bare_version(value: &str) -> bool {
    matches!(value.as_bytes(), [major, b'.', tens, units]
        if [major, tens, units].iter().all(|digit| digit.is_ascii_digit()))
}

fn decode_hex(hex: &str) -> Option<Vec<u8>> {
    if hex.is_empty() || !hex.len().is_multiple_of(2) {
        return None;
    }
    hex.as_bytes()
        .chunks(2)
        .map(|pair| {
            let digit = |byte: u8| (byte as char).to_digit(16);
            Some((digit(pair[0])? * 16 + digit(pair[1])?) as u8)
        })
        .collect()
}
