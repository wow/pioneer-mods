//! Public identities of the official XDJ-700 v1.15 update and of the files rebuilt from it, shared
//! by the owner-input tests of `patch-core` and `patch-cli` (included with `#[path]`) so that no
//! copy can go stale. They contain no firmware bytes.
// Each test crate uses a different subset of these pins.
#![allow(dead_code)]

pub const UPD_ENV: &str = "PIONEER_XDJ700_V115_UPD";
pub const UPD_LEN: usize = 17_371_335;
pub const UPD_SHA256: &str = "73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c";

/// The decoded stock application.
pub const STOCK_APPLICATION_LEN: usize = 18_601_864;
pub const STOCK_APPLICATION_SHA256: &str =
    "1875381b56d065a2b0a97a63b64ead5ce71397c521b7a62713c5bb4a0e055939";

/// The loader's fallback updater section (decoded), run on an application checksum mismatch.
pub const FALLBACK_COMPRESSED_LEN: usize = 127_776;
pub const FALLBACK_DECODED_LEN: usize = 222_684;
pub const FALLBACK_DECODED_SHA256: &str =
    "ef2e0aaabb2400bd7938ac0c2d737545db276f53ba257a12ed83063eed0cf9a2";

/// The no-op rebuild: the stock application re-encoded. Its MAIN image does not depend on the
/// label; only the descriptor and the CRCs do. Cross-checked byte-identical against the
/// reference serializer (2026-10-07).
pub const NOOP_UPD_LEN: usize = 17_368_545;
pub const NOOP_MAIN_LEN: usize = 7_250_754;
pub const NOOP_MAIN_SHA256: &str =
    "c03360e5e93493d2d3a292707c74d7889e503ac4f7e7bfa81cbe8d9af88e9eef";
/// The no-op rebuild under the stock label. A unit on v1.15 skips it.
pub const NOOP_UPD_SHA256: &str =
    "f2dd19d47b8253fbea189009166f958b2d9f29a0bb8a5d7d258f98144134d06c";

/// The no-op rebuild under the hardware-stage labels. The updater writes only versions higher
/// than the installed one (observed on an owner's unit): `Ver0.90` (stage 1, the lower probe)
/// was skipped, so stage 1b uses `Ver1.16`, the smallest higher label, which is also the
/// recovery stick. `Ver1.17` is a spare. All are cross-checked byte-identical against the
/// reference serializer.
pub const STAGE_FILES: [(&str, &str); 3] = [
    (
        "Ver0.90",
        "79f25fa1be84e0e5323273eb36ca5cbfd0f532824f6a2fde0a80db6965380252",
    ),
    (
        "Ver1.16",
        "9e1ac10e09c701cb6863b8667131e03452156a0bd7702823bc5f0502a88b6a08",
    ),
    // Spare, prepared in case labels stuck. They do not (observed: after `Ver1.16` was flashed
    // the unit still reports `1.15`), so the `Ver1.16` file is the recovery.
    (
        "Ver1.17",
        "2d0a4a09a90494c8af26fd585ec5bc1b058b2d731f9d5d72d8ed03fafc4a758d",
    ),
];

/// Stage 3: the stock application with only its version string set to `0.10` (lower than the
/// official 1.15, so the official update is written over it), labelled `Ver1.16`.
/// Cross-checked byte-identical against the reference serializer (2026-10-08).
pub const STAGE3_REPORTED_VERSION: &str = "0.10";
pub const STAGE3_LABEL: &str = "Ver1.16";
/// The committed recipe that reproduces the stage-3 file with `patch`.
pub const STAGE3_RECIPE: &str = "xdj700-v1.15/version-marker-0.10.json";
pub const STAGE3_APPLICATION_SHA256: &str =
    "74afbf4409242f58f11caac5142ccc220d39199f28c50155d0a2af9ed42d5ad3";
pub const STAGE3_MAIN_LEN: usize = 7_250_753;
pub const STAGE3_MAIN_SHA256: &str =
    "d183e5d580a9c516ec4c1f4ad91218617b7331a52f874afc405354f53c53c979";
pub const STAGE3_UPD_LEN: usize = 17_368_543;
pub const STAGE3_UPD_SHA256: &str =
    "84cbd2637b167893c6ad3ff8bc4a0b4cfb7cf5984dc399f6b018a8b5ddb0be5c";

/// Stage 5: the beat-loop experiment, built with `patch` from the committed recipe below. Its
/// application reports `0.11` and differs from stock in the version string and one entry of the
/// BEAT LOOP button table (the button labelled 16 selects 32 beats); labelled `Ver1.16`.
pub const STAGE5_RECIPE: &str = "xdj700-v1.15/beat-loop-16-plays-32.json";
pub const STAGE5_REPORTED_VERSION: &str = "0.11";
/// The decoded offset of the table entry, the recipe's only span.
pub const STAGE5_TABLE_ENTRY_OFFSET: usize = 0xd6234;
pub const STAGE5_APPLICATION_SHA256: &str =
    "dd5adad4ae531db95c9dae10d9fa534afc4e31d2ffd54daadc22da6e463f25db";
pub const STAGE5_MAIN_LEN: usize = 7_250_752;
pub const STAGE5_MAIN_SHA256: &str =
    "827936ea3c0e5d259766d3d164c04dc7e1157a427a6cedea50c6fdb1b9cf55e0";
pub const STAGE5_UPD_LEN: usize = 17_368_527;
pub const STAGE5_UPD_SHA256: &str =
    "144f4b557127a7eec2d5d2f5fadee0e6585f05a83af321097876a0edca345e12";

/// Stage 7: the beat-loop experiment with the button relabelled 32, built with `patch` from the
/// committed recipe below. Its application reports `0.12` and differs from stock in the version
/// string, the stage-5 table entry and the label box of each of the button's six images (80x53
/// RGB565, one after another from [`STAGE7_FIRST_IMAGE_OFFSET`]); labelled `Ver1.16`.
pub const STAGE7_RECIPE: &str = "xdj700-v1.15/beat-loop-32-labelled.json";
pub const STAGE7_REPORTED_VERSION: &str = "0.12";
/// The decoded offset of the button's first image.
pub const STAGE7_FIRST_IMAGE_OFFSET: usize = 0x34_f940;
pub const STAGE7_APPLICATION_SHA256: &str =
    "c942d11f81b03bfbe7855e3f77c03f0cd63ab5ea989671ce2f1fe4b91c3bb837";
pub const STAGE7_MAIN_LEN: usize = 7_250_910;
pub const STAGE7_MAIN_SHA256: &str =
    "f1de23e893f428c1dab20c1244ffdd61684d2853d725e1f337cd08a2eb71f28d";
pub const STAGE7_UPD_LEN: usize = 17_368_913;
pub const STAGE7_UPD_SHA256: &str =
    "891aa96397c6dc1f07dbf39d9ba828096e6f5f7dd6b396012968c8758ccef0c7";
