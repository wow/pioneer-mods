//! Image edits through `patch` and `precondition`: refusals that need no firmware, and the
//! authoring flow on the official file (owner input, ignored without it). Synthetic glyphs only;
//! no firmware byte is printed or committed.
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-cli --test image_edit_command -- --ignored
//! ```

mod common;
#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use common::{committed_recipe, recipes_dir, write_bytes};
use official_pins::UPD_ENV;
use patch_core::{parse_upd, sha256_hex, xdj700};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The BEAT LOOP "16" pad's normal image: 80x53 RGB565 at decoded 0x34f940.
const PAD: u64 = 0x34_f940;
const PAD_LEN: u64 = 80 * 53 * 2;

/// The committed version-marker recipe with one image edit of the pad: a synthetic 3x3 square at
/// (60, 40), erasing x 30..49, y 19..33, in the colour of pixel (2, 2). `expected` is the output
/// pin, or `None` to leave it out.
fn image_recipe(expected: Option<Value>, alpha_hex: &str) -> Value {
    let mut recipe = committed_recipe();
    let object = recipe.as_object_mut().expect("object");
    object.remove("expected");
    if let Some(expected) = expected {
        object.insert("expected".to_owned(), expected);
    }
    object.insert("recipe_id".to_owned(), json!("image-edit-test"));
    object.insert(
        "image_edits".to_owned(),
        json!([{
            "offset": PAD,
            "width": 80,
            "height": 53,
            "erase": {"x": 30, "y": 19, "width": 19, "height": 14},
            "glyph": {
                "at": {"x": 60, "y": 40, "width": 3, "height": 3},
                "alpha_hex": alpha_hex,
                "colour_from": {"x": 2, "y": 2}
            },
            "purpose": "an authoring test; never flashed"
        }]),
    );
    recipe
}

/// A placeholder output pin, as in a draft.
fn placeholder_pin() -> Option<Value> {
    Some(json!({"application_sha256": "00".repeat(32)}))
}

fn run(args: &[&std::ffi::OsStr]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .args(args)
        .env_remove("XDJ700_PROTECTED_SET")
        .output()
        .expect("run patch-cli")
}

fn precondition(input: &Path, recipe: &Path, extra: &[&str]) -> Output {
    let mut args: Vec<&std::ffi::OsStr> = vec![
        "precondition".as_ref(),
        "--input".as_ref(),
        input.as_os_str(),
        "--recipe".as_ref(),
        recipe.as_os_str(),
        "--committed-recipes".as_ref(),
    ];
    let dir = recipes_dir();
    args.push(dir.as_os_str());
    args.extend(extra.iter().map(|arg| std::ffi::OsStr::new(*arg)));
    run(&args)
}

fn patch(input: &Path, recipe: &Path, output: &Path, extra: &[&std::ffi::OsStr]) -> Output {
    let mut args: Vec<&std::ffi::OsStr> = vec![
        "patch".as_ref(),
        "--input".as_ref(),
        input.as_os_str(),
        "--recipe".as_ref(),
        recipe.as_os_str(),
        "--output".as_ref(),
        output.as_os_str(),
    ];
    args.extend_from_slice(extra);
    run(&args)
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

fn write_recipe(dir: &Path, recipe: &Value) -> PathBuf {
    let path = dir.join("draft.json");
    write_bytes(&path, &serde_json::to_vec(recipe).expect("JSON"));
    path
}

#[test]
fn refuses_a_malformed_image_edit_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let recipe = write_recipe(dir.path(), &image_recipe(placeholder_pin(), "fff"));
    let output = dir.path().join("out.UPD");

    let result = patch(
        &dir.path().join("missing.UPD"),
        &recipe,
        &output,
        &["--no-protected-set".as_ref()],
    );

    assert!(!result.status.success());
    assert!(
        text(&result.stderr).contains("image_edits[0].glyph.alpha_hex must hold one hex digit"),
        "{}",
        text(&result.stderr)
    );
    assert!(!output.exists());
}

#[test]
fn refuses_image_edits_without_an_output_pin_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let recipe = write_recipe(dir.path(), &image_recipe(None, "fffffffff"));
    let output = dir.path().join("out.UPD");

    let result = patch(
        &dir.path().join("missing.UPD"),
        &recipe,
        &output,
        &["--no-protected-set".as_ref()],
    );

    assert!(!result.status.success());
    assert!(
        text(&result.stderr)
            .contains("a recipe with image_edits must pin expected.application_sha256"),
        "{}",
        text(&result.stderr)
    );
    assert!(!output.exists());
}

#[test]
fn refuses_an_image_in_the_protected_set_before_reading_the_input() {
    let dir = tempfile::tempdir().expect("tempdir");
    let recipe = write_recipe(dir.path(), &image_recipe(placeholder_pin(), "fffffffff"));
    let set = dir.path().join("set.tsv");
    write_bytes(&set, b"release xdj700-v1.15\n08350000 08350001\n");
    let output = dir.path().join("out.UPD");

    let result = patch(
        &dir.path().join("missing.UPD"),
        &recipe,
        &output,
        &["--protected-set".as_ref(), set.as_os_str()],
    );

    assert!(!result.status.success());
    assert!(
        text(&result.stderr).contains(
            "image_edits[0] image at run-time 0x0834f940..=0x08351a5f overlaps the protected \
             set's range 0x08350000..=0x08350001"
        ),
        "{}",
        text(&result.stderr)
    );
}

/// The authoring flow on the official file: `precondition` reports the image without a hash and
/// the draft's output identities, `--check` accepts the completed recipe, and `patch` writes an
/// update whose application differs from stock only in the image's edited rows and the version
/// string. Neither the recipe nor the output holds the stock image's hash.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn precondition_pins_the_output_that_patch_then_writes() {
    let input = std::env::var_os(UPD_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {UPD_ENV} to an owner-supplied official XDJ700.UPD"));
    let dir = tempfile::tempdir().expect("tempdir");
    let draft = write_recipe(dir.path(), &image_recipe(placeholder_pin(), "fffffffff"));

    let hashed = precondition(&input, &draft, &["--no-protected-set"]);
    assert!(hashed.status.success(), "{}", text(&hashed.stderr));
    let stdout = text(&hashed.stdout);
    assert!(stdout.contains("image_edits: 1"), "{stdout}");
    let image_line = format!(
        "image_edits[0]: {PAD:#x}..{:#x} (80x53), no hash published",
        PAD + PAD_LEN
    );
    assert!(stdout.lines().any(|line| line == image_line), "{stdout}");
    let identity = |field: &str| {
        let prefix = format!("expected.{field}: ");
        let line = stdout
            .lines()
            .find(|line| line.starts_with(&prefix))
            .unwrap_or_else(|| panic!("no {field} line in {stdout}"));
        (
            line[prefix.len()..prefix.len() + 64].to_owned(),
            line.to_owned(),
        )
    };
    let (application, line) = identity("application_sha256");
    assert!(
        line.ends_with("(the recipe declares another hash)"),
        "{line}"
    );
    let (upd, line) = identity("upd_sha256");
    assert!(line.ends_with("(not declared)"), "{line}");

    let pins = json!({"application_sha256": application, "upd_sha256": upd});
    let complete_recipe = image_recipe(Some(pins), "fffffffff");
    let complete = write_recipe(dir.path(), &complete_recipe);
    let checked = precondition(&input, &complete, &["--no-protected-set", "--check"]);
    assert!(checked.status.success(), "{}", text(&checked.stderr));
    let checked_stdout = text(&checked.stdout);
    assert_eq!(checked_stdout.matches("(as declared)").count(), 2);

    let output = dir.path().join("XDJ700.UPD");
    let patched = patch(&input, &complete, &output, &["--no-protected-set".as_ref()]);
    assert!(patched.status.success(), "{}", text(&patched.stderr));
    assert!(text(&patched.stdout).contains("image_edits: 1"));

    let decode = |path: &Path| {
        let bytes = std::fs::read(path).expect("read update");
        xdj700::decode_application(&parse_upd(&bytes).expect("parse"))
            .expect("decode")
            .into_decoded()
    };
    let (stock, edited) = (decode(&input), decode(&output));
    assert_eq!(sha256_hex(&edited), application);
    assert_eq!(sha256_hex(&std::fs::read(&output).expect("read")), upd);
    // The stock image's hash appears nowhere (never printed, even on failure).
    let image_hash = sha256_hex(&stock[PAD as usize..(PAD + PAD_LEN) as usize]);
    let recipe_text = serde_json::to_string(&complete_recipe).expect("JSON");
    let printed = [stdout, checked_stdout, text(&patched.stdout), recipe_text];
    assert!(printed.iter().all(|text| !text.contains(&image_hash)));
    let rows = |y: u64| PAD + y * 160..PAD + (y + 1) * 160;
    let changed: Vec<u64> = (0..53)
        .filter(|&y| {
            let row = rows(y);
            stock[row.start as usize..row.end as usize]
                != edited[row.start as usize..row.end as usize]
        })
        .collect();
    assert!(!changed.is_empty());
    assert!(
        changed
            .iter()
            .all(|y| (19..33).contains(y) || (40..43).contains(y))
    );
    let pixel = |bytes: &[u8], x: u64, y: u64| {
        let i = (PAD + (y * 80 + x) * 2) as usize;
        [bytes[i], bytes[i + 1]]
    };
    assert_eq!(pixel(&edited, 61, 41), pixel(&stock, 2, 2));
    let image = PAD as usize..(PAD + PAD_LEN) as usize;
    let version = xdj700::VERSION_STRING_OFFSET..xdj700::VERSION_STRING_OFFSET + 4;
    for (i, (a, b)) in stock.iter().zip(&edited).enumerate() {
        if !image.contains(&i) && !version.contains(&i) {
            assert_eq!(a, b, "byte {i:#x} outside the image changed");
        }
    }
}
