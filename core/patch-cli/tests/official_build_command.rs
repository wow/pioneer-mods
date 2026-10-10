//! Owner-input test of `patch-cli build` on the official XDJ-700 v1.15 update: profiles built
//! from the committed catalog give the stage files byte for byte, and another label gives a new
//! update with the same application. Outputs go to a temporary directory; no firmware byte is
//! printed. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-cli --test official_build_command -- --ignored
//! ```

mod common;
#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use common::{repo_root, write_bytes};
use official_pins::{
    STAGE5_REPORTED_VERSION, STAGE5_UPD_LEN, STAGE5_UPD_SHA256, STAGE7_APPLICATION_SHA256,
    STAGE7_REPORTED_VERSION, STAGE7_UPD_LEN, STAGE7_UPD_SHA256, UPD_ENV,
};
use patch_core::sha256_hex;
use serde_json::json;
use std::path::PathBuf;
use std::process::Command;

/// A profile choosing only `feature`, under `label` and `reported_version`, built from the
/// official file.
fn build(feature: &str, label: &str, reported_version: &str) -> (Vec<u8>, String) {
    let input = std::env::var_os(UPD_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {UPD_ENV} to an owner-supplied official XDJ700.UPD"));
    let dir = tempfile::tempdir().expect("tempdir");
    let profile = dir.path().join("profile.json");
    let value = json!({
        "schema_version": 1, "player": "xdj700-v1.15", "features": [feature],
        "label": label, "reported_version": reported_version, "maturity": "experimental"
    });
    write_bytes(&profile, &serde_json::to_vec(&value).expect("json"));
    let output = dir.path().join("XDJ700.UPD");

    let result = Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("build")
        .arg("--profile")
        .arg(&profile)
        .arg("--input")
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .arg("--root")
        .arg(repo_root())
        .arg("--no-protected-set")
        .env_remove("XDJ700_PROTECTED_SET")
        .output()
        .expect("run patch-cli build");

    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let written = std::fs::read(&output).expect("read output");
    (
        written,
        String::from_utf8_lossy(&result.stdout).into_owned(),
    )
}

/// The profile with stage 7's feature, under its recipe's own label and version, is the stage-7
/// file; the one with stage 5's feature is the stage-5 file.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn profiles_build_the_stage_files() {
    let (stage7, stdout) = build("beat-loop-1-to-32", "Ver1.16", STAGE7_REPORTED_VERSION);
    assert_eq!(stage7.len(), STAGE7_UPD_LEN);
    assert_eq!(sha256_hex(&stage7), STAGE7_UPD_SHA256);
    for line in [
        "  evidence: Rehearsed in emulation, both ways",
        "tier: experimental",
        "'s own output, under its own label and reported version",
        &format!("output_sha256_hex: {STAGE7_UPD_SHA256}"),
        "restore: the official update (XDJ700.UPD, the input) brings back the stock application: \
         the build reports 0.12",
    ] {
        assert!(stdout.contains(line), "{line}\n{stdout}");
    }

    let (stage5, _) = build("beat-loop-16-plays-32", "Ver1.16", STAGE5_REPORTED_VERSION);
    assert_eq!(stage5.len(), STAGE5_UPD_LEN);
    assert_eq!(sha256_hex(&stage5), STAGE5_UPD_SHA256);
}

/// Under another label, stage 7's feature builds a new update under that label: the same
/// application as the stage-7 file, in another file.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn another_label_builds_a_new_update() {
    let (built, stdout) = build("beat-loop-1-to-32", "Ver1.17", STAGE7_REPORTED_VERSION);
    assert_ne!(sha256_hex(&built), STAGE7_UPD_SHA256);
    for line in [
        "; under another label or reported version than its own)",
        "a new update to rehearse both ways in emulation before flashing",
        &format!("application_sha256_hex: {STAGE7_APPLICATION_SHA256}"),
        "version_label: Ver1.17",
    ] {
        assert!(stdout.contains(line), "{line}\n{stdout}");
    }
}
