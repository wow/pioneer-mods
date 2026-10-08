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
/// was skipped, so stage 1b uses `Ver1.16`, the smallest higher label, and the recovery stick
/// uses `Ver1.17`. All are cross-checked byte-identical against the reference serializer.
pub const STAGE_FILES: [(&str, &str); 3] = [
    (
        "Ver0.90",
        "79f25fa1be84e0e5323273eb36ca5cbfd0f532824f6a2fde0a80db6965380252",
    ),
    (
        "Ver1.16",
        "9e1ac10e09c701cb6863b8667131e03452156a0bd7702823bc5f0502a88b6a08",
    ),
    // Spare, prepared in case labels stuck. They do not (observed: after `Ver1.16` was written
    // the unit still reports the application's `1.15`), so the `Ver1.16` file is the recovery.
    (
        "Ver1.17",
        "2d0a4a09a90494c8af26fd585ec5bc1b058b2d731f9d5d72d8ed03fafc4a758d",
    ),
];
