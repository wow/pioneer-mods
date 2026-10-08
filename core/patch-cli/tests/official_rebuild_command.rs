//! Owner-input tests of `patch-cli rebuild`, `patch` and `precondition` against the official
//! XDJ-700 v1.15 update.
//!
//! Vendor firmware and rebuilt files are never committed; the output goes to a temporary
//! directory, and no firmware byte is printed. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-cli --test official_rebuild_command -- --ignored
//! ```

#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use official_pins::{
    NOOP_UPD_LEN, NOOP_UPD_SHA256, STAGE_FILES, STAGE3_APPLICATION_SHA256, STAGE3_LABEL,
    STAGE3_REPORTED_VERSION, STAGE3_UPD_LEN, STAGE3_UPD_SHA256, STOCK_APPLICATION_SHA256, UPD_ENV,
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

const VERSION_MARKER: &str = "../../recipes/xdj700-v1.15/version-marker-0.10.json";

/// Runs `patch` (with `output`) or `precondition` on `input` and `recipe`.
fn run_recipe_command(command: &str, input: &Path, recipe: &Path, output: Option<&Path>) -> Output {
    let mut run = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    run.arg(command)
        .arg("--input")
        .arg(input)
        .arg("--recipe")
        .arg(recipe);
    if let Some(output) = output {
        run.arg("--output").arg(output);
    }
    run.output().expect("run patch-cli")
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

/// The stage-3 file (stock application reporting `0.10`), written by the binary and checked with
/// `inspect`, as an owner would.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn rebuild_writes_the_pinned_stage3_reported_version_file() {
    let input = official_input();
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");

    let result = Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .args(["rebuild", "--application", "stock", "--label", STAGE3_LABEL])
        .args(["--report-version", STAGE3_REPORTED_VERSION, "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .expect("run patch-cli rebuild");

    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(
        !text(&result.stderr).contains("warning:"),
        "{}",
        text(&result.stderr)
    );
    let stdout = text(&result.stdout);
    assert!(
        stdout.contains("application_reported_version: 0.10"),
        "{stdout}"
    );
    assert!(stdout.contains(&format!(
        "application_sha256_hex: {STAGE3_APPLICATION_SHA256}"
    )));
    let written = std::fs::read(&output).expect("read output");
    assert_eq!(written.len(), STAGE3_UPD_LEN);
    assert_eq!(sha256_hex(&written), STAGE3_UPD_SHA256);

    let inspect = Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .args(["inspect", "--structure", "--format", "json", "--input"])
        .arg(&output)
        .output()
        .expect("run patch-cli inspect");

    assert!(inspect.status.success(), "{}", text(&inspect.stderr));
    let report: serde_json::Value = serde_json::from_slice(&inspect.stdout).expect("inspect JSON");
    assert_eq!(
        report["application"]["reported_version"],
        STAGE3_REPORTED_VERSION
    );
    assert_eq!(
        report["application"]["decoded_sha256"],
        STAGE3_APPLICATION_SHA256
    );
}

/// `patch` with the committed version-marker recipe writes the stage-3 file.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn patch_with_the_version_marker_recipe_writes_the_stage3_file() {
    let input = official_input();
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");
    let recipe = Path::new(env!("CARGO_MANIFEST_DIR")).join(VERSION_MARKER);

    let result = run_recipe_command("patch", &input, &recipe, Some(&output));

    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(text(&result.stdout).contains(&format!("output_sha256_hex: {STAGE3_UPD_SHA256}")));
    let written = std::fs::read(&output).expect("read output");
    assert_eq!(written.len(), STAGE3_UPD_LEN);
    assert_eq!(sha256_hex(&written), STAGE3_UPD_SHA256);
}

/// The authoring flow on the official file: `precondition` hashes a draft's window, reports the
/// completed recipe as declared, and `patch` applies it. The replacement inverts two bytes in the
/// code after the header, inside a window of distinct bytes; no byte is printed.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn precondition_completes_a_draft_that_patch_then_applies() {
    let input = official_input();
    let official = std::fs::read(&input).expect("read official update");
    let stock = xdj700::decode_application(&parse_upd(&official).expect("parse")).expect("decode");
    let decoded = stock.decoded();
    let distinct = |offset: usize| {
        let mut seen = [false; 256];
        decoded[offset - 16..offset]
            .iter()
            .chain(&decoded[offset + 2..offset + 18])
            .all(|&byte| !std::mem::replace(&mut seen[usize::from(byte)], true))
    };
    let offset = (0x1000..decoded.len() - 18)
        .find(|&offset| distinct(offset))
        .expect("a window of distinct bytes");
    let dir = tempfile::tempdir().expect("tempdir");
    let recipe_path = dir.path().join("draft.json");
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(VERSION_MARKER);
    let mut recipe: serde_json::Value =
        serde_json::from_slice(&std::fs::read(path).expect("read recipe")).expect("recipe JSON");
    recipe.as_object_mut().expect("object").remove("expected");
    recipe["replacements"] = serde_json::json!([{
        "offset": offset,
        "bytes_hex": format!("{:02x}{:02x}", !decoded[offset], !decoded[offset + 1]),
        "precondition": {"before": 16, "after": 16, "sha256": "00".repeat(32)},
        "purpose": "an authoring test; never flashed"
    }]);
    let write = |recipe: &serde_json::Value| {
        std::fs::write(&recipe_path, serde_json::to_vec(recipe).expect("JSON")).expect("write")
    };
    write(&recipe);
    let sha256 = sha256_hex(&decoded[offset - 16..offset + 18]);

    let draft = run_recipe_command("precondition", &input, &recipe_path, None);
    assert!(draft.status.success(), "{}", text(&draft.stderr));
    let line = format!(
        "replacements[0].precondition: {:#x}..{:#x} sha256 {sha256} (the recipe declares another \
         hash)",
        offset - 16,
        offset + 18
    );
    assert!(
        text(&draft.stdout).contains(&line),
        "{}",
        text(&draft.stdout)
    );

    recipe["replacements"][0]["precondition"]["sha256"] = serde_json::json!(sha256);
    write(&recipe);
    let complete = run_recipe_command("precondition", &input, &recipe_path, None);
    assert!(text(&complete.stdout).contains(&format!("sha256 {sha256} (as declared)")));

    let output = dir.path().join("XDJ700.UPD");
    let patched = run_recipe_command("patch", &input, &recipe_path, Some(&output));
    assert!(patched.status.success(), "{}", text(&patched.stderr));
    assert!(text(&patched.stdout).contains("application_reported_version: 0.10"));
}
