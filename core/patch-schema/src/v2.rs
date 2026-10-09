//! Recipe schema v2: changes to the **decoded application** of a pinned official release, rebuilt
//! into a complete, installable update.
//!
//! A v2 recipe is meant to carry no vendor bytes. Each replacement declares a precondition: the
//! SHA-256 of the stock bytes in a window around the replaced span, at most
//! [`MAX_PRECONDITION_LEN`] bytes long, with at least [`MIN_PRECONDITION_LEN`] of them outside the
//! span. The window is defined relative to the span (`before` and `after` it), so a wrong `offset`
//! moves the window and fails the hash. A hash over only a few unknown bytes could be inverted by
//! brute force, which would publish them. So the windows of a recipe may not overlap each other
//! (overlapping windows would share all but a few bytes, and each hash would reveal the
//! difference); CI checks the same across all committed recipes. On the stock application, the
//! engine in `patch-core` also refuses windows in protected ranges, windows whose bytes around the
//! span are predictable, and spans that would copy stock bytes into `bytes_hex`. These checks
//! guard against accidental leaks; they are heuristics, so windows belong over code, not strings
//! or tables, and review is the backstop.
//! Only the project's own replacement bytes are written. Image edits ([`crate::image`]) change
//! RGB565 images without publishing any stock pixel: their precondition window is the whole
//! image. The static checks here need no
//! firmware; the release-specific rules (protected ranges, label and reported-version order,
//! preconditions) are enforced by the engine in `patch-core`.

use crate::image::{ImageEdit, ImageEditError};
use crate::windows::WindowOwner;
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// The only `schema_version` this module accepts.
pub const SCHEMA_VERSION_V2: u32 = 2;

/// Fewest stock bytes a precondition window covers outside its span (`before + after`), so that
/// its hash cannot be inverted to recover vendor bytes. The span's own stock bytes do not count:
/// they may follow from the replacement (a flipped bit, a changed condition).
pub const MIN_PRECONDITION_LEN: u64 = 32;

/// Longest precondition window, so that a recipe stays local and cheap to check.
pub const MAX_PRECONDITION_LEN: u64 = 4096;

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
    /// Edits to RGB565 images in the decoded application, in ascending, non-overlapping order,
    /// clear of the replacements' windows ([`ImageEdit`]).
    #[serde(default)]
    pub image_edits: Vec<ImageEdit>,
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
    /// The replacement bytes, as lowercase or uppercase hex; its length is the span length.
    pub bytes_hex: String,
    /// What the stock application must hold around the span before anything is replaced.
    pub precondition: Precondition,
    /// Why this span changes: what the original code does and what the replacement does.
    pub purpose: String,
}

/// The stock bytes around a replaced span, `offset - before .. offset + len + after`, with at least
/// [`MIN_PRECONDITION_LEN`] of them outside the span, identified by their SHA-256.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Precondition {
    /// Bytes of the window before the span.
    pub before: u64,
    /// Bytes of the window after the span.
    pub after: u64,
    /// SHA-256 of the stock bytes in the window.
    pub sha256: String,
}

impl Replacement {
    /// Span length in bytes (half the hex length, once validated).
    pub fn len(&self) -> u64 {
        (self.bytes_hex.len() / 2) as u64
    }

    /// Whether the span is empty (never true for a validated recipe).
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The replacement bytes, or `None` unless `bytes_hex` is valid, non-empty hex.
    pub fn bytes(&self) -> Option<Vec<u8>> {
        decode_hex(&self.bytes_hex)
    }

    /// The precondition window `offset - before .. offset + len + after`, or `None` if it would
    /// start before 0 or end past the 64-bit range.
    pub fn precondition_window(&self) -> Option<std::ops::Range<u64>> {
        let start = self.offset.checked_sub(self.precondition.before)?;
        let end = self
            .offset
            .checked_add(self.len())?
            .checked_add(self.precondition.after)?;
        Some(start..end)
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

#[derive(Debug, Clone, Error, PartialEq, Eq)]
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
    #[error(
        "replacements[{index}].precondition must cover at least {MIN_PRECONDITION_LEN} bytes \
         before and after the span in all (a shorter hash could be inverted to recover vendor \
         bytes)"
    )]
    PreconditionTooShort { index: usize },
    #[error("replacements[{index}].precondition.before reaches before the application's start")]
    PreconditionBeforeStart { index: usize },
    #[error("replacements[{index}].precondition must be at most {MAX_PRECONDITION_LEN} bytes long")]
    PreconditionTooLong { index: usize },
    #[error(
        "replacements[{index}].precondition overlaps the window of replacements[{previous}]; \
         overlapping windows would reveal stock bytes one hash at a time"
    )]
    OverlappingPreconditions { index: usize, previous: usize },
    #[error("expected must pin at least one identity when present")]
    EmptyExpected,
    #[error(transparent)]
    ImageEdit(#[from] ImageEditError),
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
        let mut previous_window_end: Option<(usize, u64)> = None;
        for (index, replacement) in self.replacements.iter().enumerate() {
            let precondition = &replacement.precondition;
            check_sha256(
                &format!("replacements[{index}].precondition.sha256"),
                &precondition.sha256,
            )?;
            if replacement.bytes().is_none() {
                return Err(RecipeV2Error::InvalidReplacementBytes { index });
            }
            if replacement.purpose.trim().is_empty() {
                return Err(RecipeV2Error::EmptyPurpose { index });
            }
            if precondition.before > replacement.offset {
                return Err(RecipeV2Error::PreconditionBeforeStart { index });
            }
            let window = replacement
                .precondition_window()
                .ok_or(RecipeV2Error::OffsetOverflow { index })?;
            // Cannot overflow, since the window did not.
            if precondition.before + precondition.after < MIN_PRECONDITION_LEN {
                return Err(RecipeV2Error::PreconditionTooShort { index });
            }
            if window.end - window.start > MAX_PRECONDITION_LEN {
                return Err(RecipeV2Error::PreconditionTooLong { index });
            }
            // Inside the window, so it cannot overflow.
            let end = replacement.offset + replacement.len();
            if let Some((previous, previous_end)) = previous_end
                && replacement.offset < previous_end
            {
                return Err(RecipeV2Error::UnorderedOrOverlapping { index, previous });
            }
            previous_end = Some((index, end));
            if let Some((previous, previous_window_end)) = previous_window_end
                && window.start < previous_window_end
            {
                return Err(RecipeV2Error::OverlappingPreconditions { index, previous });
            }
            previous_window_end = Some((index, window.end));
        }
        self.validate_image_edits()?;
        if let Some(expected) = &self.expected {
            if expected.application_sha256.is_none() && expected.upd_sha256.is_none() {
                return Err(RecipeV2Error::EmptyExpected);
            }
            if let Some(sha256) = &expected.application_sha256 {
                check_sha256("expected.application_sha256", sha256)?;
            }
            if let Some(sha256) = &expected.upd_sha256 {
                check_sha256("expected.upd_sha256", sha256)?;
            }
        }
        Ok(())
    }

    /// Each image edit's own checks and their order, then their distance from the replacements'
    /// windows. Within each kind the windows are disjoint, so in the list of all windows sorted
    /// by start, an image that overlaps a replacement's window overlaps its neighbour.
    fn validate_image_edits(&self) -> Result<(), RecipeV2Error> {
        let mut previous_end: Option<(usize, u64)> = None;
        for (index, edit) in self.image_edits.iter().enumerate() {
            check_sha256(&format!("image_edits[{index}].sha256"), &edit.sha256)?;
            edit.validate(index)?;
            let window = edit.window().expect("validated");
            if let Some((previous, end)) = previous_end
                && window.start < end
            {
                return Err(ImageEditError::UnorderedOrOverlapping { index, previous }.into());
            }
            previous_end = Some((index, window.end));
        }
        let mut windows = self.windows();
        windows.sort_by_key(|labelled| (labelled.window.start, labelled.window.end));
        for pair in windows.windows(2) {
            let (first, next) = (&pair[0], &pair[1]);
            if next.window.start >= first.window.end {
                continue;
            }
            if let (WindowOwner::ImageEdit(index), WindowOwner::Replacement(replacement))
            | (WindowOwner::Replacement(replacement), WindowOwner::ImageEdit(index)) =
                (first.owner, next.owner)
            {
                return Err(ImageEditError::OverlapsReplacement { index, replacement }.into());
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
    if crate::is_valid_sha256_hex(value) {
        Ok(())
    } else {
        Err(RecipeV2Error::InvalidSha256 {
            field: field.to_owned(),
        })
    }
}

/// `X.YY` (one digit, a dot, two digits) as `X * 100 + YY`, or `None` for any other form. The one
/// parser for MAIN labels (after `Ver`) and reported versions, here and in `patch-core`.
pub fn bare_version_number(version: &str) -> Option<u16> {
    match version.as_bytes() {
        [major, b'.', tens, units]
            if [major, tens, units]
                .iter()
                .all(|digit| digit.is_ascii_digit()) =>
        {
            let digit = |byte: &u8| u16::from(byte - b'0');
            Some(digit(major) * 100 + digit(tens) * 10 + digit(units))
        }
        _ => None,
    }
}

fn is_bare_version(value: &str) -> bool {
    bare_version_number(value).is_some()
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
