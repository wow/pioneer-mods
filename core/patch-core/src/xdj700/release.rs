//! The official releases a rebuild may start from, and what each one pins.

use super::app_version::VERSION_STRING_OFFSET;

/// Most a rebuilt MAIN image may grow beyond the official one. The fallback updater lies in the
/// loader region, which every rebuild keeps byte-identical (see
/// [`FALLBACK_SECTION_OFFSET`](super::FALLBACK_SECTION_OFFSET)), but the size of the application
/// flash region is unconfirmed, so growth is bounded. The hardware-tested reference alpha.2
/// build grows by 44,399 bytes. Raise this only with evidence about the flash layout.
pub const MAX_MAIN_GROWTH: usize = 256 * 1024;

/// An official release a rebuild may start from.
///
/// The library guarantees that the input matches the release passed in. The fields are public so
/// that tests can pin synthetic files, so the flash-safety bound ([`MAX_MAIN_GROWTH`] over the
/// official image) holds only when callers use a pinned constant such as [`OFFICIAL_V115`].
/// The same holds for the version rule ([`StockRelease::version_block`]). Production code,
/// including the CLI, must never construct a release from user input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StockRelease<'a> {
    /// Length of the complete official `.UPD`, checked before its hash (and before reading, by
    /// callers that can).
    pub upd_len: usize,
    /// SHA-256 of the complete official `.UPD` (lowercase hex). Only this exact file is accepted
    /// as input, so the loader, PANL and framing a rebuild keeps are the vendor's own.
    pub upd_sha256: &'a str,
    /// Largest MAIN image (loader plus section) a rebuild of this release may produce.
    pub max_main_image_len: usize,
    /// Where the release's application reports its own version. Every real release pins one, so
    /// that a modified application is refused unless it reports a lower version; `None` is for
    /// synthetic test releases only.
    pub version_block: Option<VersionBlock<'a>>,
}

/// Where a release's decoded application reports its own version, and the version its stock
/// application reports. The unit reports this string, and the updater compares a file's MAIN
/// label against it (observed on an owner's unit, v1.15).
///
/// A modified application must report a version **lower** than [`Self::stock_version`], so that
/// the official update is a higher version and restores stock. Every rebuild and verification
/// refuses one that does not ([`RebuildError::ModifiedApplicationVersion`]).
///
/// [`RebuildError::ModifiedApplicationVersion`]: crate::RebuildError::ModifiedApplicationVersion
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionBlock<'a> {
    /// Decoded-application offset of the NUL-terminated `X.YY` version string.
    pub offset: usize,
    /// The version the stock application reports (`X.YY`).
    pub stock_version: &'a str,
    /// SHA-256 of the stock decoded application (lowercase hex): the only application allowed to
    /// report [`Self::stock_version`].
    pub stock_application_sha256: &'a str,
}

/// Official XDJ-700 v1.15 (`XDJ700.UPD`, 17,371,335 bytes; MAIN image 7,251,904 bytes).
pub const OFFICIAL_V115: StockRelease<'static> = StockRelease {
    upd_len: 17_371_335,
    upd_sha256: "73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c",
    max_main_image_len: 0x6E_A7C0 + MAX_MAIN_GROWTH,
    version_block: Some(OFFICIAL_V115_VERSION_BLOCK),
};

// Every pinned release carries a version block, so the rule for modified applications applies.
const _: () = assert!(OFFICIAL_V115.version_block.is_some());

/// The v1.15 application's version block: `1.15` at decoded offset `0x740`.
pub const OFFICIAL_V115_VERSION_BLOCK: VersionBlock<'static> = VersionBlock {
    offset: VERSION_STRING_OFFSET,
    stock_version: "1.15",
    stock_application_sha256: "1875381b56d065a2b0a97a63b64ead5ce71397c521b7a62713c5bb4a0e055939",
};
