//! Where the protected set comes from on `patch` and `precondition`: `--protected-set`, the
//! `XDJ700_PROTECTED_SET` variable, or an explicit `--no-protected-set`; without one a schema-v2
//! recipe is refused. No firmware is needed. Synthetic ranges only.

mod common;

use common::protected_set::{
    CLEAR, OVERLAPPING, assert_refused, beat_loop, patch_with, precondition_with, setup, stderr,
};
use common::write_bytes;
use patch_cli::recipe::{ProtectedSetSource, protected_set_line};
use patch_core::xdj700::{ProtectedSet, recipe_target};
use std::path::Path;

#[test]
fn both_commands_refuse_a_schema_v2_recipe_without_a_decision() {
    let run = setup(CLEAR.as_bytes());

    for result in [
        patch_with(&run, &beat_loop(), &[], None),
        precondition_with(&run, &beat_loop(), &[], None),
    ] {
        assert_refused(
            &result,
            &run,
            "no protected set: pass --protected-set <file> (or set XDJ700_PROTECTED_SET)",
        );
        assert!(stderr(&result).contains("or --no-protected-set to skip that check deliberately"));
    }
}

#[test]
fn an_explicit_skip_reaches_the_input_with_a_warning() {
    let run = setup(OVERLAPPING.as_bytes());

    // Even with an overlapping set in the environment: the explicit flag wins.
    let patched = patch_with(
        &run,
        &beat_loop(),
        &["--no-protected-set".as_ref()],
        Some(&run.set),
    );
    let hashed = precondition_with(&run, &beat_loop(), &["--no-protected-set"], Some(&run.set));

    assert_refused(&patched, &run, "input path");
    assert_refused(&hashed, &run, "failed to read input update");
    for result in [&patched, &hashed] {
        assert!(
            stderr(result).contains("warning: --no-protected-set: recipe"),
            "{}",
            stderr(result)
        );
    }
}

#[test]
fn the_environment_names_the_set_when_the_flag_is_absent() {
    let overlapping = setup(OVERLAPPING.as_bytes());
    let clear = setup(CLEAR.as_bytes());

    assert_refused(
        &patch_with(&overlapping, &beat_loop(), &[], Some(&overlapping.set)),
        &overlapping,
        "overlaps the protected set's range",
    );
    assert_refused(
        &precondition_with(&overlapping, &beat_loop(), &[], Some(&overlapping.set)),
        &overlapping,
        "overlaps the protected set's range",
    );
    assert_refused(
        &patch_with(&clear, &beat_loop(), &[], Some(&clear.set)),
        &clear,
        "input path",
    );
    // The flag wins over the environment.
    assert_refused(
        &patch_with(
            &clear,
            &beat_loop(),
            &["--protected-set".as_ref(), overlapping.set.as_os_str()],
            Some(&clear.set),
        ),
        &clear,
        "overlaps the protected set's range",
    );
}

#[test]
fn the_set_and_the_skip_cannot_both_be_given() {
    let run = setup(CLEAR.as_bytes());

    let result = patch_with(
        &run,
        &beat_loop(),
        &[
            "--protected-set".as_ref(),
            run.set.as_os_str(),
            "--no-protected-set".as_ref(),
        ],
        None,
    );

    assert!(!result.status.success());
    assert!(
        stderr(&result).contains("cannot be used with"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn force_is_refused_before_the_set_is_read() {
    let run = setup(b"not a set");

    let result = patch_with(
        &run,
        &beat_loop(),
        &[
            "--force".as_ref(),
            "--protected-set".as_ref(),
            run.set.as_os_str(),
        ],
        None,
    );

    assert_refused(
        &result,
        &run,
        "--force is not accepted with a schema-v2 recipe",
    );
    assert!(
        !stderr(&result).contains("protected set"),
        "{}",
        stderr(&result)
    );
}

#[test]
fn patch_refuses_the_set_options_with_a_schema_v1_manifest() {
    let run = setup(CLEAR.as_bytes());
    let manifest = run.set.with_file_name("v1.json");
    write_bytes(&manifest, br#"{"schema_version": 1}"#);

    for extra in [
        vec!["--protected-set".as_ref(), run.set.as_os_str()],
        vec!["--no-protected-set".as_ref()],
    ] {
        assert_refused(
            &patch_with(&run, &manifest, &extra, None),
            &run,
            "--protected-set and --no-protected-set apply only to schema-v2 recipes",
        );
    }
}

#[test]
fn the_source_is_the_flag_the_environment_or_an_explicit_skip() {
    let flag = Path::new("flag.tsv");
    let env = || Some("env.tsv".into());

    assert_eq!(
        ProtectedSetSource::choose(Some(flag), false, env()).expect("flag"),
        ProtectedSetSource::Flag(flag.to_owned())
    );
    assert_eq!(
        ProtectedSetSource::choose(None, true, env()).expect("skip"),
        ProtectedSetSource::Skipped
    );
    assert_eq!(
        ProtectedSetSource::choose(None, false, env()).expect("environment"),
        ProtectedSetSource::Env("env.tsv".into())
    );
    assert!(ProtectedSetSource::choose(None, false, None).is_err());
    assert!(ProtectedSetSource::choose(None, false, Some("".into())).is_err());
}

#[test]
fn the_report_line_says_whether_and_where_the_set_was_checked() {
    let target = recipe_target("xdj700-v1.15").expect("known release");
    let one =
        ProtectedSet::parse("release xdj700-v1.15\n08000800 08000800\n", target).expect("valid");
    let two = ProtectedSet::parse(CLEAR, target).expect("valid");
    let flag = ProtectedSetSource::Flag("set.tsv".into());

    assert_eq!(
        protected_set_line(&ProtectedSetSource::Skipped, None),
        "protected_set: skipped (--no-protected-set); start-up and update-path code not checked"
    );
    assert_eq!(
        protected_set_line(&flag, Some(&one)),
        "protected_set: 1 range; no span or precondition window overlaps it"
    );
    assert_eq!(
        protected_set_line(&flag, Some(&two)),
        "protected_set: 2 ranges; no span or precondition window overlaps them"
    );
    assert_eq!(
        protected_set_line(&ProtectedSetSource::Env("set.tsv".into()), Some(&two)),
        "protected_set: 2 ranges (from XDJ700_PROTECTED_SET); no span or precondition window \
         overlaps them"
    );
}
