//! The protected set's contents on `patch` and `precondition`: an overlap, a set for another
//! release, one that covers the version string, and a malformed or unreadable file are refused
//! before the input is read, so these tests need no firmware. Where the set comes from is in
//! `protected_set_source.rs`. Synthetic ranges only; the measured set stays outside the
//! repository.

mod common;

use common::protected_set::{
    CLEAR, OVERLAPPING, assert_refused, beat_loop, patch, precondition, setup, stderr,
};
use patch_cli::recipe::MAX_PROTECTED_SET_LEN;

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
    let run = setup(b"# measured\nrelease xdj700-v1.15\n08000800 08000805 5\n");

    let result = patch(&run, &beat_loop(), &run.set);

    assert_refused(&result, &run, "refusing protected set");
    assert!(
        stderr(&result).contains("line 3: bytes is 5, but 0x08000800..=0x08000805 holds 6"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn refuses_a_set_without_its_release_or_for_another_release() {
    let unnamed = setup(b"08000800 08000805\n");
    assert_refused(
        &patch(&unnamed, &beat_loop(), &unnamed.set),
        &unnamed,
        "line 1: expected `release <id>`",
    );

    let other = setup(b"release xdj700-v1.16\n08000800 08000805\n");
    assert_refused(
        &precondition(&other, &beat_loop()),
        &other,
        "the set was measured on release xdj700-v1.16, not xdj700-v1.15",
    );
}

#[test]
fn refuses_a_set_that_covers_the_version_string() {
    let run = setup(b"release xdj700-v1.15\n08000600 0800074f\n");

    let result = patch(&run, &beat_loop(), &run.set);

    assert_refused(
        &result,
        &run,
        "the version string every rebuild writes, run-time 0x08000740..=0x08000743",
    );
}

#[test]
fn refuses_decoded_offsets_in_place_of_run_time_addresses() {
    let run = setup(b"release xdj700-v1.15\n00000600 00000605\n");

    let result = precondition(&run, &beat_loop());

    assert_refused(&result, &run, "is not inside the application");
}

#[test]
fn refuses_an_empty_set() {
    let run = setup(b"release xdj700-v1.15\nstart end bytes\n");

    let result = patch(&run, &beat_loop(), &run.set);

    assert_refused(&result, &run, "the protected set holds no ranges");
}

#[test]
fn accepts_a_byte_order_mark() {
    let mut bytes = "\u{feff}".as_bytes().to_vec();
    bytes.extend_from_slice(CLEAR.as_bytes());
    let run = setup(&bytes);

    let result = patch(&run, &beat_loop(), &run.set);

    assert_refused(&result, &run, "input path");
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
    let cap = usize::try_from(MAX_PROTECTED_SET_LEN).expect("fits");
    let large = setup(&vec![b'#'; cap + 1]);
    assert_refused(
        &patch(&large, &beat_loop(), &large.set),
        &large,
        &format!(
            "{} bytes is larger than a protected set (at most {MAX_PROTECTED_SET_LEN})",
            cap + 1
        ),
    );

    let binary = setup(&[0xff, 0xfe, 0x00]);
    assert_refused(
        &patch(&binary, &beat_loop(), &binary.set),
        &binary,
        "not UTF-8 text",
    );
}
