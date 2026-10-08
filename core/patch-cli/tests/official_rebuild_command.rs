//! Owner-input test of `patch-cli rebuild` against the official XDJ-700 v1.15 update.
//!
//! Vendor firmware and rebuilt files are never committed; the output goes to a temporary
//! directory. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-cli --test official_rebuild_command -- --ignored
//! ```

use patch_core::{parse_upd, sha256_hex, xdj700};
use std::path::PathBuf;
use std::process::Command;

const ENV_VAR: &str = "PIONEER_XDJ700_V115_UPD";
/// The no-op rebuild (stock application re-encoded, label `Ver1.15`), pinned in `patch-core`'s
/// `official_rebuild` test.
const NOOP_UPD_LEN: usize = 17_368_545;
const NOOP_UPD_SHA256: &str = "f2dd19d47b8253fbea189009166f958b2d9f29a0bb8a5d7d258f98144134d06c";
const STOCK_APPLICATION_SHA256: &str =
    "1875381b56d065a2b0a97a63b64ead5ce71397c521b7a62713c5bb4a0e055939";

#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn rebuild_writes_the_pinned_noop_update_once() {
    let input = std::env::var_os(ENV_VAR)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {ENV_VAR} to an owner-supplied official XDJ700.UPD"));
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_patch-cli"))
            .args(["rebuild", "--application", "stock", "--label", "Ver1.15"])
            .arg("--input")
            .arg(&input)
            .arg("--output")
            .arg(&output)
            .output()
            .expect("run patch-cli rebuild")
    };

    let first = run();

    assert!(
        first.status.success(),
        "{}",
        String::from_utf8_lossy(&first.stderr)
    );
    let stdout = String::from_utf8_lossy(&first.stdout);
    assert!(stdout.contains(&format!("output_sha256_hex: {NOOP_UPD_SHA256}")));
    assert!(stdout.contains(&format!(
        "application_sha256_hex: {STOCK_APPLICATION_SHA256}"
    )));
    let written = std::fs::read(&output).expect("read output");
    assert_eq!(written.len(), NOOP_UPD_LEN);
    assert_eq!(sha256_hex(&written), NOOP_UPD_SHA256);
    let section = xdj700::decode_application(&parse_upd(&written).expect("parse output"))
        .expect("the no-op output decodes");
    assert_eq!(section.decoded_sha256(), STOCK_APPLICATION_SHA256);

    let second = run();

    assert!(!second.status.success(), "a second run must not overwrite");
    assert!(
        String::from_utf8_lossy(&second.stderr).contains("choose a new output path"),
        "{}",
        String::from_utf8_lossy(&second.stderr)
    );
    assert_eq!(std::fs::read(&output).expect("reread output"), written);
}
