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
//! The second variable is needed only by `rebuild_reproduces_the_reference_alpha2_pins` (add
//! `--skip alpha2` otherwise). It names the decoded alpha.2 application that the
//! reference implementation's recipe produces from the official file (see the README's
//! Acknowledgements).
//! Its identities, and those of the hardware-tested alpha.2 MAIN image and update, are public
//! pins and contain no firmware bytes.

#[path = "common/official_pins.rs"]
mod official_pins;

use official_pins::{
    NOOP_MAIN_LEN, NOOP_MAIN_SHA256, NOOP_UPD_LEN, NOOP_UPD_SHA256, STAGE_FILES,
    STAGE3_APPLICATION_SHA256, STAGE3_LABEL, STAGE3_MAIN_LEN, STAGE3_MAIN_SHA256,
    STAGE3_REPORTED_VERSION, STAGE3_UPD_LEN, STAGE3_UPD_SHA256, STOCK_APPLICATION_SHA256, UPD_ENV,
    UPD_SHA256,
};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, OFFICIAL_V115, decode_application, rebuild_with_application,
    rebuild_with_stock_application, verify_rebuild,
};
use patch_core::{RebuildError, parse_upd, read_firmware, read_regular_file, sha256_hex, xdj700};
use std::path::PathBuf;

const ALPHA2_ENV: &str = "PIONEER_XDJ700_REFERENCE_ALPHA2_DECODED";
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
fn stage_files_are_pinned_and_decode_to_the_stock_application() {
    let official = official_upd();

    for (label, sha256) in STAGE_FILES {
        let rebuilt =
            rebuild_with_stock_application(&official, &OFFICIAL_V115, label).expect("rebuild");

        assert_eq!(rebuilt.bytes().len(), NOOP_UPD_LEN, "{label}");
        assert_eq!(rebuilt.sha256(), sha256, "{label}");
        // Pins every MAIN byte, so the loader region, and the fallback updater pinned in
        // `official_firmware`, are the official ones.
        assert_eq!(rebuilt.main_image_sha256(), NOOP_MAIN_SHA256, "{label}");
        let parsed = parse_upd(rebuilt.bytes()).expect("parse");
        assert_eq!(
            xdj700::main_document(&parsed)
                .expect("main")
                .descriptor()
                .version(),
            label
        );
        // The label is not verified, but the loader region is, so the layout is accepted.
        let application = decode_application(&parsed).expect("stage file decodes");
        assert_eq!(
            application.decoded_sha256(),
            STOCK_APPLICATION_SHA256,
            "{label}"
        );
    }
}

#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn stage3_reported_version_file_is_pinned() {
    let official = official_upd();
    let stock = decode_application(&parse_upd(&official).expect("parse")).expect("decode");
    assert_eq!(xdj700::reported_version(stock.decoded()), Some("1.15"));

    let modified = xdj700::with_reported_version(stock.decoded(), STAGE3_REPORTED_VERSION)
        .expect("lower version");
    let rebuilt = rebuild_with_application(&official, &OFFICIAL_V115, &modified, STAGE3_LABEL)
        .expect("rebuild");

    assert_eq!(rebuilt.application_sha256(), STAGE3_APPLICATION_SHA256);
    assert_eq!(rebuilt.main_image_len(), STAGE3_MAIN_LEN);
    assert_eq!(rebuilt.main_image_sha256(), STAGE3_MAIN_SHA256);
    assert_eq!(rebuilt.bytes().len(), STAGE3_UPD_LEN);
    assert_eq!(rebuilt.sha256(), STAGE3_UPD_SHA256);
    let application = decode_application(&parse_upd(rebuilt.bytes()).expect("parse output"))
        .expect("the stage-3 file decodes");
    assert_eq!(
        xdj700::reported_version(application.decoded()),
        Some(STAGE3_REPORTED_VERSION)
    );
}
