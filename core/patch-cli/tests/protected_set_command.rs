//! `--protected-set` on `patch` and `precondition`: the set is read and checked before the input,
//! so these tests need no firmware. Synthetic ranges only; the measured set stays outside the
//! repository.

mod common;

use common::{recipes_dir, write_bytes};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The committed beat-loop recipe: its precondition window is run-time `0x080D6234..=0x080D66F4`.
fn beat_loop() -> PathBuf {
    recipes_dir().join("xdj700-v1.15/beat-loop-16-plays-32.json")
}

const OVERLAPPING: &str = "start end bytes\n080d6300 080d63ff 256\n";
const CLEAR: &str = "08000600 080d6233\n080d66f5 080d7000\n";

struct Run {
    _dir: tempfile::TempDir,
    input: PathBuf,
    output: PathBuf,
    set: PathBuf,
}

/// A temporary directory with no input (so a command that gets past the set stops at the input),
/// an output path, and `set` written as the protected set.
fn setup(set: &[u8]) -> Run {
    let dir = tempfile::tempdir().expect("tempdir");
    let run = Run {
        input: dir.path().join("XDJ700.UPD"),
        output: dir.path().join("out.UPD"),
        set: dir.path().join("set.tsv"),
        _dir: dir,
    };
    write_bytes(&run.set, set);
    run
}

fn patch(run: &Run, recipe: &Path, set: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("patch")
        .arg("--input")
        .arg(&run.input)
        .arg("--recipe")
        .arg(recipe)
        .arg("--output")
        .arg(&run.output)
        .arg("--protected-set")
        .arg(set)
        .output()
        .expect("run patch-cli patch")
}

fn precondition(run: &Run, recipe: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("precondition")
        .arg("--input")
        .arg(&run.input)
        .arg("--recipe")
        .arg(recipe)
        .arg("--committed-recipes")
        .arg(recipes_dir())
        .arg("--protected-set")
        .arg(&run.set)
        .output()
        .expect("run patch-cli precondition")
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// Refused with `message`, nothing printed and no output written.
fn assert_refused(result: &Output, run: &Run, message: &str) {
    assert!(!result.status.success());
    assert!(stderr(result).contains(message), "{}", stderr(result));
    assert!(result.stdout.is_empty(), "nothing may be printed");
    assert!(!run.output.exists(), "no output may be written");
}

#[test]
fn patch_refuses_a_recipe_overlapping_the_set_before_reading_the_input() {
    let run = setup(OVERLAPPING.as_bytes());

    let result = patch(&run, &beat_loop(), &run.set);

    assert_refused(
        &result,
        &run,
        "replacements[0] precondition window at run-time 0x080d6234..=0x080d66f4 overlaps the \
         protected set's range 0x080d6300..=0x080d63ff",
    );
    assert!(stderr(&result).contains("refusing to apply recipe"));
}

#[test]
fn patch_gets_past_a_clear_set_to_the_input() {
    let run = setup(CLEAR.as_bytes());

    let result = patch(&run, &beat_loop(), &run.set);

    // The set passed: the next step, the input, is what fails.
    assert_refused(&result, &run, "input path");
    assert!(
        !stderr(&result).contains("protected set"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn precondition_refuses_a_recipe_overlapping_the_set_before_hashing() {
    let run = setup(OVERLAPPING.as_bytes());

    let result = precondition(&run, &beat_loop());

    assert_refused(
        &result,
        &run,
        "overlaps the protected set's range 0x080d6300..=0x080d63ff",
    );
    assert!(stderr(&result).contains("refusing to hash the preconditions"));
}

#[test]
fn precondition_gets_past_a_clear_set_to_the_input() {
    let run = setup(CLEAR.as_bytes());

    let result = precondition(&run, &beat_loop());

    assert_refused(&result, &run, "failed to read input update");
}

#[test]
fn refuses_a_malformed_set_with_its_line_number() {
    let run = setup(b"# measured\n08000600 08000605 5\n");

    let result = patch(&run, &beat_loop(), &run.set);

    assert_refused(&result, &run, "refusing protected set");
    assert!(
        stderr(&result).contains("line 2: bytes is 5, but 0x08000600..=0x08000605 holds 6"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn refuses_decoded_offsets_in_place_of_run_time_addresses() {
    let run = setup(b"00000600 00000605\n");

    let result = precondition(&run, &beat_loop());

    assert_refused(&result, &run, "is not inside the application");
}

#[test]
fn refuses_an_empty_set() {
    let run = setup(b"start end bytes\n");

    let result = patch(&run, &beat_loop(), &run.set);

    assert_refused(&result, &run, "the protected set holds no ranges");
}

#[test]
fn refuses_a_set_that_is_not_a_regular_file() {
    let run = setup(b"");
    let directory = run.set.parent().expect("parent").to_owned();

    let result = patch(&run, &beat_loop(), &directory);

    assert_refused(&result, &run, "failed to read protected set");
}

#[test]
fn refuses_a_set_that_is_too_large_or_not_text() {
    let large = setup(&vec![b'#'; 8 * 1024 * 1024 + 1]);
    assert_refused(
        &patch(&large, &beat_loop(), &large.set),
        &large,
        "larger than 8388608 bytes",
    );

    let binary = setup(&[0xff, 0xfe, 0x00]);
    assert_refused(
        &patch(&binary, &beat_loop(), &binary.set),
        &binary,
        "not UTF-8 text",
    );
}

#[test]
fn patch_refuses_the_set_with_a_schema_v1_manifest() {
    let run = setup(CLEAR.as_bytes());
    let manifest = run.set.with_file_name("v1.json");
    write_bytes(&manifest, br#"{"schema_version": 1}"#);

    let result = patch(&run, &manifest, &run.set);

    assert_refused(
        &result,
        &run,
        "--protected-set applies only to schema-v2 recipes",
    );
}

#[test]
fn the_report_line_says_whether_a_set_was_checked() {
    use patch_cli::recipe::protected_set_line;
    use patch_core::xdj700::{ProtectedSet, recipe_target};

    let target = recipe_target("xdj700-v1.15").expect("known release");
    let one = ProtectedSet::parse("08000000 08000000\n", target).expect("valid");
    let two = ProtectedSet::parse(CLEAR, target).expect("valid");

    assert_eq!(
        protected_set_line(None),
        "protected_set: not given (--protected-set); start-up and update-path code not checked"
    );
    assert_eq!(
        protected_set_line(Some(&one)),
        "protected_set: 1 range; no span or precondition window overlaps it"
    );
    assert_eq!(
        protected_set_line(Some(&two)),
        "protected_set: 2 ranges; no span or precondition window overlaps them"
    );
}
