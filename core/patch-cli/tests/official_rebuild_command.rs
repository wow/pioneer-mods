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

mod common;
#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use common::{
    committed_recipe, recipes_dir, run_patch_command_with_args_in_dir, version_marker_path,
};
use official_pins::{
    NOOP_UPD_LEN, NOOP_UPD_SHA256, STAGE_FILES, STAGE3_APPLICATION_SHA256, STAGE3_LABEL,
    STAGE3_REPORTED_VERSION, STAGE3_UPD_LEN, STAGE3_UPD_SHA256, STAGE5_RECIPE, STAGE5_UPD_LEN,
    STAGE5_UPD_SHA256, STOCK_APPLICATION_SHA256, UPD_ENV,
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

/// Runs `patch` on `input` and `recipe`, writing `output`.
fn run_patch(input: &Path, recipe: &Path, output: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("patch")
        .args(["--input".as_ref(), input.as_os_str()])
        .args(["--recipe".as_ref(), recipe.as_os_str()])
        .args(["--output".as_ref(), output.as_os_str()])
        .output()
        .expect("run patch-cli patch")
}

/// Runs `precondition` on `input` and `recipe`, against the committed recipes, with `extra`.
fn run_precondition(input: &Path, recipe: &Path, extra: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("precondition")
        .args(["--input".as_ref(), input.as_os_str()])
        .args(["--recipe".as_ref(), recipe.as_os_str()])
        .arg("--committed-recipes")
        .arg(recipes_dir())
        .args(extra)
        .output()
        .expect("run patch-cli precondition")
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

/// Checks that a `patch` run succeeded and wrote the file pinned as `len` bytes with `sha256`.
fn assert_wrote(result: &Output, output: &Path, len: usize, sha256: &str) {
    assert!(result.status.success(), "{}", text(&result.stderr));
    assert!(text(&result.stdout).contains(&format!("output_sha256_hex: {sha256}")));
    let written = std::fs::read(output).expect("read output");
    assert_eq!(written.len(), len);
    assert_eq!(sha256_hex(&written), sha256);
}

/// `patch` with the committed version-marker recipe writes the stage-3 file.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn patch_with_the_version_marker_recipe_writes_the_stage3_file() {
    let input = official_input();
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");

    let result = run_patch(&input, &version_marker_path(), &output);

    assert_wrote(&result, &output, STAGE3_UPD_LEN, STAGE3_UPD_SHA256);
}

/// `patch` with the committed beat-loop recipe writes the stage-5 file, run as the flashing guide
/// gives it: from the repository root, with the recipe's relative path.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn patch_with_the_beat_loop_recipe_writes_the_stage5_file() {
    let input = std::fs::canonicalize(official_input()).expect("input path");
    let dir = tempfile::tempdir().expect("tempdir");
    let output = dir.path().join("XDJ700.UPD");
    let root = recipes_dir().join("..");
    let recipe = Path::new("recipes").join(STAGE5_RECIPE);

    let result = run_patch_command_with_args_in_dir(&input, &recipe, &output, &[], Some(&root));

    assert_wrote(&result, &output, STAGE5_UPD_LEN, STAGE5_UPD_SHA256);
}

/// The stock application of the official file, and the offset of a 2-byte span in the code after
/// the header whose window of 16 bytes on each side holds distinct bytes. Nothing is printed.
fn stock_and_code_offset(input: &Path) -> (Vec<u8>, usize) {
    let official = std::fs::read(input).expect("read official update");
    let stock = xdj700::decode_application(&parse_upd(&official).expect("parse")).expect("decode");
    let decoded = stock.into_decoded();
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
    (decoded, offset)
}

/// A draft replacement inverting the two stock bytes at `offset`, with a placeholder hash.
fn inverting(decoded: &[u8], offset: usize, before: usize, after: usize) -> serde_json::Value {
    serde_json::json!({
        "offset": offset,
        "bytes_hex": format!("{:02x}{:02x}", !decoded[offset], !decoded[offset + 1]),
        "precondition": {"before": before, "after": after, "sha256": "00".repeat(32)},
        "purpose": "an authoring test; never flashed"
    })
}

/// The authoring flow on the official file: `precondition` hashes a draft's window, `--check`
/// refuses the draft and accepts the completed recipe, and `patch` applies it. No byte is printed.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn precondition_completes_a_draft_that_patch_then_applies() {
    let input = official_input();
    let (decoded, offset) = stock_and_code_offset(&input);
    let dir = tempfile::tempdir().expect("tempdir");
    let recipe_path = dir.path().join("draft.json");
    let mut recipe = committed_recipe();
    recipe.as_object_mut().expect("object").remove("expected");
    recipe["replacements"] = serde_json::json!([inverting(&decoded, offset, 16, 16)]);
    let write = |recipe: &serde_json::Value| {
        std::fs::write(&recipe_path, serde_json::to_vec(recipe).expect("JSON")).expect("write")
    };
    write(&recipe);
    let sha256 = sha256_hex(&decoded[offset - 16..offset + 18]);

    let draft = run_precondition(&input, &recipe_path, &[]);
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
    let draft_checked = run_precondition(&input, &recipe_path, &["--check"]);
    assert!(!draft_checked.status.success());
    assert!(
        text(&draft_checked.stderr).contains("1 of 1 declared precondition hashes differ"),
        "{}",
        text(&draft_checked.stderr)
    );

    recipe["replacements"][0]["precondition"]["sha256"] = serde_json::json!(sha256);
    write(&recipe);
    let complete = run_precondition(&input, &recipe_path, &["--check"]);
    assert!(complete.status.success(), "{}", text(&complete.stderr));
    assert!(text(&complete.stdout).contains(&format!("sha256 {sha256} (as declared)")));

    let output = dir.path().join("XDJ700.UPD");
    let patched = run_patch(&input, &recipe_path, &output);
    assert!(patched.status.success(), "{}", text(&patched.stderr));
    assert!(text(&patched.stdout).contains("application_reported_version: 0.10"));
}

/// When a later window is refused, `precondition` prints no hash at all, not even the earlier
/// window's: here the second window lies over zero padding.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn precondition_prints_nothing_when_a_later_window_is_refused() {
    let input = official_input();
    let (decoded, offset) = stock_and_code_offset(&input);
    // Zero padding after the code window, so the refused window is the second.
    let padding = (offset + 64..decoded.len() - 40)
        .find(|&start| decoded[start..start + 32].iter().all(|&byte| byte == 0))
        .expect("zero padding after the code window");
    let spans = [(offset, 16, 16), (padding + 32, 32, 6)];
    let dir = tempfile::tempdir().expect("tempdir");
    let recipe_path = dir.path().join("draft.json");
    let mut recipe = committed_recipe();
    recipe["replacements"] = spans
        .iter()
        .map(|&(at, before, after)| inverting(&decoded, at, before, after))
        .collect();
    std::fs::write(&recipe_path, serde_json::to_vec(&recipe).expect("JSON")).expect("write");

    let result = run_precondition(&input, &recipe_path, &[]);

    assert!(!result.status.success());
    assert!(
        text(&result.stderr).contains("replacements[1]: the 4 most common byte values fill"),
        "{}",
        text(&result.stderr)
    );
    assert!(result.stdout.is_empty(), "no hash may be printed");
}
