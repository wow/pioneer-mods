//! The protected set: parsing the local range file, and refusing recipes whose spans or
//! precondition windows overlap it. Synthetic ranges only; the measured set stays outside the
//! repository.

#[path = "common/recipe_files.rs"]
mod recipe_files;

use patch_core::xdj700::{
    ProtectedSet, ProtectedSetError, RecipeError, check_recipe_against_protected_set,
    check_recipe_v2, recipe_target,
};
use patch_schema::RecipeV2;
use recipe_files::committed_recipes;

const RELEASE: &str = "xdj700-v1.15";

fn target() -> &'static patch_core::xdj700::RecipeTarget<'static> {
    recipe_target(RELEASE).expect("known release")
}

fn parse(text: &str) -> Result<ProtectedSet, ProtectedSetError> {
    ProtectedSet::parse(text, target())
}

/// The committed beat-loop recipe: one replacement at decoded `0xD6234` (run-time `0x080D6234`)
/// whose precondition window runs to `0xD66F4` inclusive (run-time `0x080D66F4`).
fn beat_loop() -> RecipeV2 {
    committed_recipes()
        .into_iter()
        .find(|(path, _)| path.ends_with("beat-loop-16-plays-32.json"))
        .map(|(_, recipe)| recipe)
        .expect("the committed beat-loop recipe")
}

fn check(recipe: &RecipeV2, text: &str) -> Result<(), RecipeError> {
    let set = parse(text).expect("a valid set");
    check_recipe_against_protected_set(recipe, target(), &set)
}

#[test]
fn parses_the_measured_format_with_header_comments_and_prefixes() {
    let set = parse(
        "# measured locally\n\nstart\tend\tbytes\n08000600\t08000605\t6\n0x08000610 0X08000649\n",
    )
    .expect("valid");

    assert_eq!(set.len(), 2);
    assert!(!set.is_empty());
    assert_eq!(set.ranges(), &[0x600..0x606, 0x610..0x64a]);
}

#[test]
fn accepts_the_first_and_last_addresses_of_the_application() {
    let last = 0x0800_0000u64 + 18_601_864 - 1;

    let set = parse(&format!("08000000 08000000\n{last:x} {last:x}\n")).expect("valid");

    assert_eq!(set.ranges(), &[0..1, 18_601_863..18_601_864]);
}

#[test]
fn refuses_an_empty_set() {
    assert_eq!(parse(""), Err(ProtectedSetError::Empty));
    assert_eq!(
        parse("# only a comment\nstart end bytes\n"),
        Err(ProtectedSetError::Empty)
    );
}

#[test]
fn refuses_a_header_after_the_first_range() {
    assert_eq!(
        parse("08000600 08000605\nstart end bytes\n"),
        Err(ProtectedSetError::NotHex {
            line: 2,
            field: "start",
            value: "start".to_owned()
        })
    );
}

#[test]
fn refuses_malformed_lines_with_their_line_number() {
    assert_eq!(
        parse("08000600\n"),
        Err(ProtectedSetError::Fields { line: 1, fields: 1 })
    );
    assert_eq!(
        parse("08000600 08000605 6 extra\n"),
        Err(ProtectedSetError::Fields { line: 1, fields: 4 })
    );
    assert_eq!(
        parse("\n08000600 0800060g\n"),
        Err(ProtectedSetError::NotHex {
            line: 2,
            field: "end",
            value: "0800060g".to_owned()
        })
    );
    assert_eq!(
        parse("+8000600 08000605\n"),
        Err(ProtectedSetError::NotHex {
            line: 1,
            field: "start",
            value: "+8000600".to_owned()
        })
    );
    assert_eq!(
        parse("0x 08000605\n"),
        Err(ProtectedSetError::NotHex {
            line: 1,
            field: "start",
            value: "0x".to_owned()
        })
    );
    assert_eq!(
        parse("08000600 08000605 six\n"),
        Err(ProtectedSetError::NotCount {
            line: 1,
            value: "six".to_owned()
        })
    );
    assert_eq!(
        parse("08000605 08000600\n"),
        Err(ProtectedSetError::Reversed {
            line: 1,
            start: 0x0800_0605,
            end: 0x0800_0600
        })
    );
}

#[test]
fn refuses_a_byte_count_that_does_not_match_inclusive_ends() {
    // A file written with exclusive ends would carry 5 here.
    assert_eq!(
        parse("08000600 08000605 5\n"),
        Err(ProtectedSetError::ByteCount {
            line: 1,
            start: 0x0800_0600,
            end: 0x0800_0605,
            count: 5,
            expected: 6
        })
    );
}

#[test]
fn refuses_decoded_offsets_and_ranges_past_the_application() {
    let outside = |line, start, end| {
        Err(ProtectedSetError::OutsideApplication {
            line,
            start,
            end,
            app_start: 0x0800_0000,
            app_end: 0x0800_0000 + 18_601_864 - 1,
        })
    };
    // Decoded offsets instead of run-time addresses.
    assert_eq!(parse("00000600 00000605\n"), outside(1, 0x600, 0x605));
    assert_eq!(
        parse("08000600 08000605\n091bd780 091bd788\n"),
        outside(2, 0x091b_d780, 0x091b_d788)
    );
    assert_eq!(
        parse("ffffffffffffffff ffffffffffffffff\n"),
        outside(1, u64::MAX, u64::MAX)
    );
    // The widest range: refused by its bounds before its byte count is computed.
    assert_eq!(parse("0 ffffffffffffffff\n"), outside(1, 0, u64::MAX));
    assert_eq!(parse("0 ffffffffffffffff 0\n"), outside(1, 0, u64::MAX));
    assert_eq!(
        parse("123456789abcdef01 123456789abcdef02\n"),
        Err(ProtectedSetError::NotHex {
            line: 1,
            field: "start",
            value: "123456789abcdef01".to_owned()
        })
    );
}

#[test]
fn every_committed_recipe_passes_its_firmware_free_checks_first() {
    // The protected-set check runs after these, which bound every window.
    for (path, recipe) in committed_recipes() {
        assert_eq!(
            check_recipe_v2(&recipe, target()),
            Ok(()),
            "{}",
            path.display()
        );
    }
}

#[test]
fn refuses_a_precondition_window_that_overlaps_a_range() {
    let result = check(&beat_loop(), "080d6300 080d63ff\n");

    assert_eq!(
        result,
        Err(RecipeError::ProtectedSet {
            index: 0,
            what: "precondition window",
            start: 0x080d_6234,
            last: 0x080d_66f4,
            set_start: 0x080d_6300,
            set_last: 0x080d_63ff,
        })
    );
    let message = result.unwrap_err().to_string();
    assert!(
        message.contains("run-time 0x080d6234..=0x080d66f4"),
        "{message}"
    );
    assert!(message.contains("section 5"), "{message}");
}

#[test]
fn reports_the_span_when_the_span_itself_overlaps() {
    assert_eq!(
        check(&beat_loop(), "080d6234 080d6234 1\n"),
        Err(RecipeError::ProtectedSet {
            index: 0,
            what: "span",
            start: 0x080d_6234,
            last: 0x080d_6234,
            set_start: 0x080d_6234,
            set_last: 0x080d_6234,
        })
    );
}

#[test]
fn accepts_ranges_that_only_touch_the_window() {
    // The last address before the window and the first after it.
    assert_eq!(
        check(&beat_loop(), "08000600 080d6233\n080d66f5 080d7000\n"),
        Ok(())
    );
}

#[test]
fn finds_an_overlap_in_any_line_of_an_unsorted_set() {
    let result = check(
        &beat_loop(),
        "08e00000 08e00010\n08000600 08000605\n080d66f4 080d66f4\n",
    );

    assert!(
        matches!(
            result,
            Err(RecipeError::ProtectedSet {
                set_start: 0x080d_66f4,
                ..
            })
        ),
        "{result:?}"
    );
}

#[test]
fn a_recipe_without_replacements_passes_any_set() {
    let version_marker = committed_recipes()
        .into_iter()
        .find(|(path, _)| path.ends_with("version-marker-0.10.json"))
        .map(|(_, recipe)| recipe)
        .expect("the committed version-marker recipe");

    assert_eq!(check(&version_marker, "08000000 08000fff\n"), Ok(()));
}
