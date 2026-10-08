//! Owner-input validation against the official XDJ-700 v1.15 update.
//!
//! Vendor firmware is never committed. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-core --test official_firmware -- --ignored
//! ```
//!
//! The pinned identities are public (published by the reference implementation; see the README's
//! Acknowledgements) and contain no firmware bytes.

#[path = "common/official_pins.rs"]
mod official_pins;

use official_pins::{
    FALLBACK_COMPRESSED_LEN, FALLBACK_DECODED_LEN, FALLBACK_DECODED_SHA256, STOCK_APPLICATION_LEN,
    STOCK_APPLICATION_SHA256, UPD_ENV, UPD_LEN, UPD_SHA256,
};
use patch_core::upd::ImageReport;
use patch_core::{read_firmware, sha256_hex, verify_roundtrip, xdj700};
use std::path::PathBuf;

const MAIN_IMAGE_SHA256: &str = "de683f253eba02e86ada5c89f2302a0f3a45331ffdcb5e6f9f20359aa6f6ce3a";
const PANL_IMAGE_SHA256: &str = "52c5a54320c11477c50ed1da93fc585128c50e9e78d624c5ae27a8f4ad4c3a99";
/// Re-encoding of the stock application (not the stock packer's bytes). Verified byte-identical
/// to the reference encoder's output for the same input (2026-10-07), and pinned so any
/// encoder drift is noticed.
const REENCODED_STREAM_LEN: usize = 6_988_604;
const REENCODED_STREAM_SHA256: &str =
    "00b29057b9b92e20de9b5c899ac36095bbd1128f94e7aa1dd5b8314c8b0c8063";

fn official_upd_path() -> PathBuf {
    std::env::var_os(UPD_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {UPD_ENV} to an owner-supplied official XDJ700.UPD"))
}

#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn official_xdj700_v115_roundtrips_and_matches_pinned_images() {
    let (identity, bytes) = read_firmware(&official_upd_path()).expect("read official UPD");
    assert_eq!(
        identity.size_bytes, UPD_LEN as u64,
        "not the official v1.15 file"
    );
    assert_eq!(
        identity.sha256_hex, UPD_SHA256,
        "not the official v1.15 file"
    );

    let container = verify_roundtrip(&bytes).expect("byte-identical roundtrip");

    let [main, panel] = container.documents() else {
        panic!("expected MAIN and PANL documents");
    };
    assert_eq!(main.descriptor().kind(), "MAIN");
    assert_eq!(main.descriptor().version(), "Ver1.15");
    assert_eq!(main.data_records().len(), 222_510);
    assert_eq!(panel.descriptor().kind(), "PANL");
    assert_eq!(panel.descriptor().version(), "Ver1.00");
    assert_eq!(panel.data_records().len(), 207);

    let main_image = main.image().expect("MAIN image");
    assert_eq!(main_image.base(), 0);
    assert_eq!(main_image.bytes().len(), 0x6E_A7C0);
    assert_eq!(sha256_hex(main_image.bytes()), MAIN_IMAGE_SHA256);
    assert_eq!(
        [sha256_hex(&main_image.bytes()[..xdj700::APPLICATION_SECTION_OFFSET]).as_str()],
        xdj700::VERIFIED_LOADER_SHA256,
        "the verified loader region is the official v1.15 one"
    );

    let panel_image = panel.image().expect("PANL image");
    assert_eq!(panel_image.base(), 0x0C_0000);
    assert_eq!(panel_image.bytes().len(), 0x4_0000);
    assert_eq!(sha256_hex(panel_image.bytes()), PANL_IMAGE_SHA256);

    // The structure report must show the same identities that users compare with the pins.
    let summary = container.summary().expect("summary");
    let reported: Vec<Option<&str>> = summary
        .documents
        .iter()
        .map(|doc| match &doc.image {
            ImageReport::Reconstructed { sha256, .. } => Some(sha256.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(reported, [Some(MAIN_IMAGE_SHA256), Some(PANL_IMAGE_SHA256)]);

    let application = xdj700::decode_application(&container).expect("application section");
    assert_eq!(application.offset(), xdj700::APPLICATION_SECTION_OFFSET);
    assert_eq!(application.decoded().len(), STOCK_APPLICATION_LEN);
    // The recipe engine bounds-checks windows against this pin before reading the input.
    let target = xdj700::recipe_target("xdj700-v1.15").expect("recipe target");
    assert_eq!(target.application_len, application.decoded().len());
    assert_eq!(application.decoded_sha256(), STOCK_APPLICATION_SHA256);
    assert_eq!(&application.decoded()[..19], &[0; 19]);
    assert_eq!(
        xdj700::reported_version(application.decoded()),
        Some("1.15")
    );

    let fallback = xdj700::decode_section(main_image.bytes(), xdj700::FALLBACK_SECTION_OFFSET)
        .expect("fallback updater section");
    assert_eq!(fallback.compressed_len(), FALLBACK_COMPRESSED_LEN);
    assert_eq!(fallback.decoded().len(), FALLBACK_DECODED_LEN);
    assert_eq!(fallback.decoded_sha256(), FALLBACK_DECODED_SHA256);

    let reencoded = xdj700::encode_section(application.decoded()).expect("re-encode");
    let stream = &reencoded[4..reencoded.len() - 2];
    assert_eq!(stream.len(), REENCODED_STREAM_LEN);
    assert_eq!(sha256_hex(stream), REENCODED_STREAM_SHA256);
    let redecoded = xdj700::decode_section(&reencoded, 0).expect("decode re-encoded");
    assert_eq!(redecoded.decoded_sha256(), STOCK_APPLICATION_SHA256);
}
