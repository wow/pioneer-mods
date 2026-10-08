//! XDJ-700 MAIN version labels (`VerX.YY`) and the order the updater compares them in.

use crate::error::RebuildError;
/// The one `X.YY` parser, shared with the recipe schema so the two cannot disagree.
pub(super) use patch_schema::bare_version_number;

/// MAIN label of the official v1.15 release.
pub const OFFICIAL_V115_LABEL: &str = "Ver1.15";

/// Checks that `version` is an XDJ-700 MAIN label of the form `VerX.YY`, as every rebuild
/// requires.
///
/// # Errors
///
/// [`RebuildError::InvalidVersionLabel`] otherwise.
pub fn validate_version_label(version: &str) -> Result<(), RebuildError> {
    version_number(version)
        .map(|_| ())
        .ok_or_else(|| RebuildError::InvalidVersionLabel {
            label: version.to_owned(),
        })
}

/// Whether `label` is a higher version than `installed`. The updater writes a document only when
/// its version is higher than the installed one (observed on an owner's unit), so a label that
/// is not higher is skipped.
///
/// # Errors
///
/// [`RebuildError::InvalidVersionLabel`] if either label is not of the form `VerX.YY`.
pub fn is_label_higher(label: &str, installed: &str) -> Result<bool, RebuildError> {
    let number = |version: &str| {
        version_number(version).ok_or_else(|| RebuildError::InvalidVersionLabel {
            label: version.to_owned(),
        })
    };
    Ok(number(label)? > number(installed)?)
}

/// `VerX.YY` as `X * 100 + YY`, or `None` for any other form.
fn version_number(label: &str) -> Option<u16> {
    label.strip_prefix("Ver").and_then(bare_version_number)
}
