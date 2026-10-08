//! The version the XDJ-700 application reports about itself.
//!
//! The decoded v1.15 application carries a version block: the model string at `0x720`, the
//! version string `1.15` (NUL-terminated) at [`VERSION_STRING_OFFSET`], and the build date at
//! `0x760`. On an owner's unit the reported version, not the MAIN label, is what the updater
//! compares a file against (observed: after a `Ver1.16` no-op the unit still reported `1.15`);
//! this string is its most likely source (static analysis).
//!
//! A modified application reports a version **lower** than the official one, so that the official
//! update is written over it (a higher version) and restores the stock application.

use super::label::{OFFICIAL_V115_LABEL, is_label_higher};
use crate::error::RebuildError;

/// Decoded-application offset of the NUL-terminated version string (`X.YY`) in v1.15.
pub const VERSION_STRING_OFFSET: usize = 0x740;

/// `X.YY` plus the terminating NUL.
const VERSION_FIELD_LEN: usize = 5;

/// The version string the decoded application reports, if it holds one of the form `X.YY` at
/// [`VERSION_STRING_OFFSET`].
pub fn reported_version(decoded: &[u8]) -> Option<&str> {
    let field = decoded.get(VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + VERSION_FIELD_LEN)?;
    let (version, nul) = field.split_at(VERSION_FIELD_LEN - 1);
    (nul == [0] && is_version(version))
        .then(|| std::str::from_utf8(version).expect("checked ASCII"))
}

/// Checks that `version` may be reported by a modified application: of the form `X.YY` and
/// lower than the official v1.15.
///
/// # Errors
///
/// [`RebuildError::InvalidReportedVersion`] or [`RebuildError::ReportedVersionNotLower`].
pub fn validate_reported_version(version: &str) -> Result<(), RebuildError> {
    if !is_version(version.as_bytes()) {
        return Err(RebuildError::InvalidReportedVersion {
            version: version.to_owned(),
        });
    }
    if is_label_higher(OFFICIAL_V115_LABEL, &format!("Ver{version}"))? {
        Ok(())
    } else {
        Err(RebuildError::ReportedVersionNotLower {
            version: version.to_owned(),
            official: OFFICIAL_V115_LABEL.trim_start_matches("Ver").to_owned(),
        })
    }
}

/// Returns a copy of `decoded` whose version string reports `version` instead. Only those four
/// bytes change.
///
/// # Errors
///
/// Any [`validate_reported_version`] error, or [`RebuildError::MissingVersionString`] if
/// `decoded` holds no version string at [`VERSION_STRING_OFFSET`].
pub fn with_reported_version(decoded: &[u8], version: &str) -> Result<Vec<u8>, RebuildError> {
    validate_reported_version(version)?;
    if reported_version(decoded).is_none() {
        return Err(RebuildError::MissingVersionString {
            offset: VERSION_STRING_OFFSET,
        });
    }
    let mut modified = decoded.to_vec();
    modified[VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + VERSION_FIELD_LEN - 1]
        .copy_from_slice(version.as_bytes());
    Ok(modified)
}

fn is_version(bytes: &[u8]) -> bool {
    matches!(bytes, [major, b'.', tens, units]
        if [major, tens, units].iter().all(|digit| digit.is_ascii_digit()))
}
