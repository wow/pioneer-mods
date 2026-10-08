//! The version the XDJ-700 application reports about itself.
//!
//! The decoded v1.15 application carries a version block: the model string at `0x720`, the
//! version string `1.15` (NUL-terminated) at [`VERSION_STRING_OFFSET`], and the build date at
//! `0x760`. The updater compares a file's MAIN label against this string, and UTILITY shows it
//! (observed on an owner's unit: with only this string changed to `0.10`, UTILITY showed `0.10`
//! and the next official update showed `MAIN Ver0.10 -> Ver1.15`).
//!
//! A modified application reports a version **lower** than the official one, so that the official
//! update is written over it (a higher version) and restores the stock application (observed for
//! an application changed only in its version string).

use super::label::bare_version_number;
use super::release::VersionBlock;
use crate::error::RebuildError;
use crate::identity::sha256_hex;

/// Decoded-application offset of the NUL-terminated version string (`X.YY`) in v1.15.
pub const VERSION_STRING_OFFSET: usize = 0x740;

/// `X.YY` plus the terminating NUL.
const VERSION_FIELD_LEN: usize = 5;

/// The version string a decoded v1.15 application reports, if it holds one of the form `X.YY` at
/// [`VERSION_STRING_OFFSET`].
pub fn reported_version(decoded: &[u8]) -> Option<&str> {
    reported_version_at(decoded, VERSION_STRING_OFFSET)
}

/// The NUL-terminated `X.YY` version string at `offset` in `decoded`, if there is one.
pub fn reported_version_at(decoded: &[u8], offset: usize) -> Option<&str> {
    let field = decoded.get(offset..offset.checked_add(VERSION_FIELD_LEN)?)?;
    let (version, nul) = field.split_at(VERSION_FIELD_LEN - 1);
    let version = std::str::from_utf8(version).ok()?;
    (nul == [0] && bare_version_number(version).is_some()).then_some(version)
}

impl VersionBlock<'_> {
    /// Checks that a modified application may report `version`: of the form `X.YY` and lower than
    /// [`Self::stock_version`].
    ///
    /// # Errors
    ///
    /// [`RebuildError::InvalidReportedVersion`] or [`RebuildError::ReportedVersionNotLower`].
    pub fn validate_reported_version(&self, version: &str) -> Result<(), RebuildError> {
        let number =
            bare_version_number(version).ok_or_else(|| RebuildError::InvalidReportedVersion {
                version: version.to_owned(),
            })?;
        if self.is_lower(number) {
            Ok(())
        } else {
            Err(RebuildError::ReportedVersionNotLower {
                version: version.to_owned(),
                official: self.stock_version.to_owned(),
            })
        }
    }

    /// Changes `decoded`'s version string, in place, to report `version`. Only those bytes change.
    ///
    /// # Errors
    ///
    /// Any [`Self::validate_reported_version`] error, or [`RebuildError::MissingVersionString`] if
    /// `decoded` holds no version string at [`Self::offset`].
    pub fn set_reported_version(
        &self,
        decoded: &mut [u8],
        version: &str,
    ) -> Result<(), RebuildError> {
        self.validate_reported_version(version)?;
        if reported_version_at(decoded, self.offset).is_none() {
            return Err(RebuildError::MissingVersionString {
                offset: self.offset,
            });
        }
        decoded[self.offset..self.offset + VERSION_FIELD_LEN - 1]
            .copy_from_slice(version.as_bytes());
        Ok(())
    }

    /// The rule every rebuild enforces: the stock application (by SHA-256) may report anything,
    /// but any other application must report a version lower than [`Self::stock_version`], so that
    /// the official update can restore it.
    ///
    /// # Errors
    ///
    /// [`RebuildError::ModifiedApplicationVersion`] for a modified application that reports no
    /// version string, or one not lower than [`Self::stock_version`].
    pub fn check_application(&self, decoded: &[u8]) -> Result<(), RebuildError> {
        self.check(decoded, || sha256_hex(decoded))
    }

    /// [`Self::check_application`] with the application's SHA-256 already computed.
    pub(crate) fn check_application_hashed(
        &self,
        decoded: &[u8],
        sha256: &str,
    ) -> Result<(), RebuildError> {
        self.check(decoded, || sha256.to_owned())
    }

    /// Hashes `decoded` (through `sha256`) only when it does not report a lower version.
    fn check(&self, decoded: &[u8], sha256: impl FnOnce() -> String) -> Result<(), RebuildError> {
        let reported = reported_version_at(decoded, self.offset);
        let lower = reported
            .and_then(bare_version_number)
            .is_some_and(|number| self.is_lower(number));
        if lower || sha256() == self.stock_application_sha256 {
            Ok(())
        } else {
            Err(RebuildError::ModifiedApplicationVersion {
                reported: reported.map(str::to_owned),
                official: self.stock_version.to_owned(),
            })
        }
    }

    fn is_lower(&self, number: u16) -> bool {
        bare_version_number(self.stock_version).is_some_and(|stock| number < stock)
    }
}
