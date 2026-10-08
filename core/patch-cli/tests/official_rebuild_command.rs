//! Owner-input test of `patch-cli rebuild` against the official XDJ-700 v1.15 update.
//!
//! Vendor firmware and rebuilt files are never committed; the output goes to a temporary
//! directory. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-cli --test official_rebuild_command -- --ignored
//! ```

#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use official_pins::{
    NOOP_UPD_LEN, NOOP_UPD_SHA256, STAGE_FILES, STOCK_APPLICATION_SHA256, UPD_ENV,
};
use patch_core::{parse_upd, sha256_hex, xdj700};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn official_input() -> PathBuf {
    std::env::var_os(UPD_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {UPD_ENV} to an owner-supplied official XDJ700.UPD"))
}

fn run_rebuild(input: &Path, label: &str, output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .args(["rebuild", "--application", "stock", "--label", label])
        .arg("--input")
        .arg(input)
        .arg("--output")
        .arg(output)
        .output()
        .expect("run patch-cli rebuild")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn rebuild_writes_the_pinned_noop_update_once() {
    let input = official_input();
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");

    let first = run_rebuild(&input, "Ver1.15", &output);

    assert!(first.status.success(), "{}", text(&first.stderr));
    assert!(
        text(&first.stderr).contains("warning: label Ver1.15 is not higher"),
        "{}",
        text(&first.stderr)
    );
    let stdout = text(&first.stdout);
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

    let second = run_rebuild(&input, "Ver1.15", &output);

    assert!(!second.status.success(), "a second run must not overwrite");
    assert!(
        text(&second.stderr).contains("choose a new output path"),
        "{}",
        text(&second.stderr)
    );
    assert_eq!(std::fs::read(&output).expect("reread output"), written);
}

/// Every pinned hardware stage file (stage 1, stage 1b, which is also the recovery stick, and the
/// `Ver1.17` spare), written by the binary and checked with `inspect`, as an owner would.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn rebuild_writes_every_pinned_stage_file() {
    let input = official_input();

    for (label, sha256) in STAGE_FILES {
        let dir = tempfile::tempdir().expect("tempdir");
        let output = dir.path().join("XDJ700.UPD");

        let result = run_rebuild(&input, label, &output);

        assert!(result.status.success(), "{label}: {}", text(&result.stderr));
        // Only the lower stage-1 probe is a label the updater skips.
        assert_eq!(
            text(&result.stderr).contains("warning:"),
            label == "Ver0.90",
            "{label}: {}",
            text(&result.stderr)
        );
        assert!(text(&result.stdout).contains(&format!("output_sha256_hex: {sha256}")));
        let written = std::fs::read(&output).expect("read output");
        assert_eq!(written.len(), NOOP_UPD_LEN, "{label}");
        assert_eq!(sha256_hex(&written), sha256, "{label}");

        let inspect = Command::new(env!("CARGO_BIN_EXE_patch-cli"))
            .args(["inspect", "--structure", "--format", "json", "--input"])
            .arg(&output)
            .output()
            .expect("run patch-cli inspect");

        assert!(
            inspect.status.success(),
            "{label}: {}",
            text(&inspect.stderr)
        );
        let report: serde_json::Value =
            serde_json::from_slice(&inspect.stdout).expect("inspect JSON");
        assert_eq!(
            report["application"]["decoded_sha256"], STOCK_APPLICATION_SHA256,
            "{label}: {report}"
        );
    }
}
