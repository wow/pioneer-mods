//! Owner-input validation against the official XDJ-700 v1.15 update.
//!
//! Vendor firmware is never committed. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-core --test official_firmware -- --ignored
//! ```
//!
//! The pinned identities are public (DeckVolve xdj-700-mods, MIT) and contain no firmware bytes.

use patch_core::{read_firmware, sha256_hex, verify_roundtrip};
use std::path::PathBuf;

const ENV_VAR: &str = "PIONEER_XDJ700_V115_UPD";
const UPD_SIZE: u64 = 17_371_335;
const UPD_SHA256: &str = "73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c";
const MAIN_IMAGE_SHA256: &str = "de683f253eba02e86ada5c89f2302a0f3a45331ffdcb5e6f9f20359aa6f6ce3a";
const PANL_IMAGE_SHA256: &str = "52c5a54320c11477c50ed1da93fc585128c50e9e78d624c5ae27a8f4ad4c3a99";

fn official_upd_path() -> PathBuf {
    std::env::var_os(ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {ENV_VAR} to an owner-supplied official XDJ700.UPD"))
}

#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn official_xdj700_v115_roundtrips_and_matches_pinned_images() {
    let (identity, bytes) = read_firmware(&official_upd_path()).expect("read official UPD");
    assert_eq!(identity.size_bytes, UPD_SIZE, "not the official v1.15 file");
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

    let panel_image = panel.image().expect("PANL image");
    assert_eq!(panel_image.base(), 0x0C_0000);
    assert_eq!(panel_image.bytes().len(), 0x4_0000);
    assert_eq!(sha256_hex(panel_image.bytes()), PANL_IMAGE_SHA256);

    // The structure report must show the same identities that users compare with the pins.
    let summary = container.summary();
    let reported: Vec<_> = summary
        .documents
        .iter()
        .map(|doc| doc.image.as_ref().map(|image| image.sha256.as_str()))
        .collect();
    assert_eq!(reported, [Some(MAIN_IMAGE_SHA256), Some(PANL_IMAGE_SHA256)]);
}
