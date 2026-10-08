//! Owner-input rebuild validation against the official XDJ-700 v1.15 update.
//!
//! Vendor firmware and anything derived from it are never committed. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//! PIONEER_XDJ700_REFERENCE_ALPHA2_DECODED=/path/to/alpha2-decoded.bin \
//!     cargo test -p patch-core --test official_rebuild -- --ignored
//! ```
//!
//! The second variable is needed only by `rebuild_reproduces_the_reference_alpha2_pins` (run
//! the no-op test alone by name otherwise). It names the decoded alpha.2 application that the
//! reference implementation's recipe produces from the official file (see the README's
//! Acknowledgements).
//! Its identities, and those of the hardware-tested alpha.2 MAIN image and update, are public
//! pins and contain no firmware bytes.

use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, OFFICIAL_V115, decode_application, rebuild_with_application,
    rebuild_with_stock_application, verify_rebuild,
};
use patch_core::{RebuildError, parse_upd, read_firmware, read_regular_file, sha256_hex, xdj700};
use std::path::PathBuf;

const UPD_ENV: &str = "PIONEER_XDJ700_V115_UPD";
const ALPHA2_ENV: &str = "PIONEER_XDJ700_REFERENCE_ALPHA2_DECODED";
const UPD_SHA256: &str = "73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c";

/// The stock application re-encoded under the stock label: the first hardware candidate (H1).
/// Cross-checked byte-identical against the reference serializer (2026-10-07).
const NOOP_UPD_LEN: usize = 17_368_545;
const NOOP_UPD_SHA256: &str = "f2dd19d47b8253fbea189009166f958b2d9f29a0bb8a5d7d258f98144134d06c";
const NOOP_MAIN_LEN: usize = 7_250_754;
const NOOP_MAIN_SHA256: &str = "c03360e5e93493d2d3a292707c74d7889e503ac4f7e7bfa81cbe8d9af88e9eef";
/// The same no-op rebuild labelled `Ver1.90`, the hardware stage-1 file. The updater skips a
/// document whose version equals the installed one, so stage 1 on a v1.15 unit needs another
/// label. The MAIN image is unchanged; only the descriptor and the CRCs differ.
const NOOP_VER190_UPD_SHA256: &str =
    "aff3a1b9f887dc6d6e35f5686d0edfa644ce9e661011775489315ddbcf928f99";
const FALLBACK_DECODED_SHA256: &str =
    "ef2e0aaabb2400bd7938ac0c2d737545db276f53ba257a12ed83063eed0cf9a2";

const ALPHA2_DECODED_LEN: usize = 18_655_132;
const ALPHA2_DECODED_SHA256: &str =
    "9bfb9df00336bf79c9c0acc529f29ed1a23afaa82c5fb49df7ed4dffd214d2b6";
const ALPHA2_MAIN_LEN: usize = 7_296_303;
const ALPHA2_MAIN_SHA256: &str = "40cf2ae9872a859f11d5c1bf45603fcb78513c3ace2eca36f8ab81f7d0e54a8f";
const ALPHA2_UPD_LEN: usize = 17_479_565;
const ALPHA2_UPD_SHA256: &str = "817d2ad30066d2e1c41a224e6cb63786374d6a0cf05ac48c1a609eb5388368b2";
const ALPHA2_LABEL: &str = "Ver1.22";

fn official_upd() -> Vec<u8> {
    let path = std::env::var_os(UPD_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {UPD_ENV} to an owner-supplied official XDJ700.UPD"));
    let (identity, bytes) = read_firmware(&path).expect("read official UPD");
    assert_eq!(
        identity.sha256_hex, UPD_SHA256,
        "not the official v1.15 file"
    );
    bytes
}

#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn noop_rebuild_of_official_v115_is_pinned_and_deterministic() {
    let official = official_upd();
    let stock = decode_application(&parse_upd(&official).expect("parse")).expect("decode");

    let first = rebuild_with_application(&official, &OFFICIAL_V115, stock.decoded(), "Ver1.15")
        .expect("rebuild");
    let second = rebuild_with_application(&official, &OFFICIAL_V115, stock.decoded(), "Ver1.15")
        .expect("rerun");

    assert_eq!(first, second, "reruns must be byte-identical");
    let from_stock =
        rebuild_with_stock_application(&official, &OFFICIAL_V115, "Ver1.15").expect("stock");
    assert_eq!(from_stock, first);
    assert_eq!(from_stock.application_sha256(), stock.decoded_sha256());
    assert_eq!(first.main_image_len(), NOOP_MAIN_LEN);
    assert_eq!(
        first.main_image_len() - APPLICATION_SECTION_OFFSET,
        4 + 6_988_604 + 2,
        "loader + re-encoded section, no padding"
    );
    assert_eq!(first.main_image_sha256(), NOOP_MAIN_SHA256);
    assert_eq!(first.bytes().len(), NOOP_UPD_LEN);
    assert_eq!(first.sha256(), NOOP_UPD_SHA256);
    assert_eq!(
        verify_rebuild(
            &official,
            &OFFICIAL_V115,
            first.bytes(),
            stock.decoded(),
            "Ver1.15"
        ),
        Ok(())
    );
    // A rebuild is never accepted as the next input, so chained rebuilds cannot grow MAIN past
    // the release bound.
    assert_eq!(
        rebuild_with_application(first.bytes(), &OFFICIAL_V115, stock.decoded(), "Ver1.15"),
        Err(RebuildError::UnpinnedInput {
            sha256: NOOP_UPD_SHA256.to_owned()
        })
    );
}

#[test]
#[ignore = "needs owner-supplied firmware and the reference alpha.2 decoded target"]
fn rebuild_reproduces_the_reference_alpha2_pins() {
    let path = std::env::var_os(ALPHA2_ENV)
        .unwrap_or_else(|| panic!("set {ALPHA2_ENV} to the reference alpha.2 decoded application"));
    let official = official_upd();
    let decoded = read_regular_file(&PathBuf::from(path)).expect("read decoded target");
    assert_eq!(decoded.len(), ALPHA2_DECODED_LEN);
    assert_eq!(sha256_hex(&decoded), ALPHA2_DECODED_SHA256);

    let rebuilt = rebuild_with_application(&official, &OFFICIAL_V115, &decoded, ALPHA2_LABEL)
        .expect("rebuild");

    assert_eq!(rebuilt.main_image_len(), ALPHA2_MAIN_LEN);
    assert_eq!(rebuilt.main_image_sha256(), ALPHA2_MAIN_SHA256);
    assert_eq!(rebuilt.bytes().len(), ALPHA2_UPD_LEN);
    assert_eq!(rebuilt.sha256(), ALPHA2_UPD_SHA256);
}

#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn stage1_noop_rebuild_labelled_ver190_is_pinned_and_keeps_the_fallback_updater() {
    let official = official_upd();

    let rebuilt =
        rebuild_with_stock_application(&official, &OFFICIAL_V115, "Ver1.90").expect("rebuild");

    assert_eq!(rebuilt.bytes().len(), NOOP_UPD_LEN);
    assert_eq!(rebuilt.sha256(), NOOP_VER190_UPD_SHA256);
    assert_eq!(rebuilt.main_image_sha256(), NOOP_MAIN_SHA256);
    let parsed = parse_upd(rebuilt.bytes()).expect("parse");
    let main = xdj700::main_document(&parsed).expect("main");
    assert_eq!(main.descriptor().version(), "Ver1.90");
    let image = main.image().expect("image");
    let fallback = xdj700::decode_section(image.bytes(), xdj700::FALLBACK_SECTION_OFFSET)
        .expect("fallback updater section survives the rebuild");
    assert_eq!(fallback.decoded_sha256(), FALLBACK_DECODED_SHA256);
}
