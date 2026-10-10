//! Owner-input test of `patch-cli compose` on the official XDJ-700 v1.15 update. The output goes
//! to a temporary directory; no firmware byte is printed. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-cli --test official_compose_command -- --ignored
//! ```

mod common;
#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use common::recipes_dir;
use official_pins::{
    STAGE5_RECIPE, STAGE7_RECIPE, STAGE7_REPORTED_VERSION, STAGE7_UPD_LEN, STAGE7_UPD_SHA256,
    UPD_ENV,
};
use patch_core::sha256_hex;
use std::path::PathBuf;
use std::process::Command;

/// Composing stage 5 and stage 7 (which repeats stage 5's change) under stage 7's label and
/// version writes the stage-7 file byte for byte, and reports each recipe as reproduced alone.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn compose_writes_the_stage7_file_from_stages_5_and_7() {
    let input = std::env::var_os(UPD_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {UPD_ENV} to an owner-supplied official XDJ700.UPD"));
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");

    let result = Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("compose")
        .arg("--input")
        .arg(&input)
        .arg("--recipe")
        .arg(recipes_dir().join(STAGE5_RECIPE))
        .arg("--recipe")
        .arg(recipes_dir().join(STAGE7_RECIPE))
        .args([
            "--label",
            "Ver1.16",
            "--report-version",
            STAGE7_REPORTED_VERSION,
        ])
        .arg("--output")
        .arg(&output)
        .arg("--no-protected-set")
        .env_remove("XDJ700_PROTECTED_SET")
        .output()
        .expect("run patch-cli compose");

    let stdout = String::from_utf8_lossy(&result.stdout);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let written = std::fs::read(&output).expect("read output");
    assert_eq!(written.len(), STAGE7_UPD_LEN);
    assert_eq!(sha256_hex(&written), STAGE7_UPD_SHA256);
    assert!(stdout.contains(&format!("output_sha256_hex: {STAGE7_UPD_SHA256}")));
    assert!(stdout.contains("recipe[0]: xdj700-v1.15-beat-loop-16-plays-32 (alone:"));
    assert!(stdout.contains("recipe[1]: xdj700-v1.15-beat-loop-1-to-32 (alone:"));
    assert!(stdout.contains("rehearse it both ways in emulation"));
}
