//! The protected set against recipes: `check_recipe_against_protected_set` refuses an overlapping
//! span or precondition window, a set that covers the version string, and a set for another
//! release, and the recipe entry points run it through their `RecipeChecks`. Synthetic ranges
//! only; the measured set stays outside the repository. Parsing is in `xdj700_protected_set.rs`.

mod common;
#[path = "common/recipe_files.rs"]
mod recipe_files;

use common::recipe::{Fixture, replacement};
use patch_core::xdj700::{
    ProtectedSet, RecipeChecks, RecipeError, RecipeTarget, apply_recipe_v2_to,
    check_recipe_against_protected_set, check_recipe_v2, precondition_hashes, recipe_target,
};
use patch_schema::RecipeV2;
use recipe_files::committed_recipes;

const RELEASE: &str = "xdj700-v1.15";

fn target() -> &'static RecipeTarget<'static> {
    recipe_target(RELEASE).expect("known release")
}

/// A v1.15 set of the `ranges` lines.
fn set(ranges: &str) -> ProtectedSet {
    ProtectedSet::parse(&format!("release {RELEASE}\n{ranges}"), target()).expect("a valid set")
}

/// The committed recipe whose file name ends with `name`.
fn committed(name: &str) -> RecipeV2 {
    committed_recipes()
        .into_iter()
        .find(|(path, _)| path.ends_with(name))
        .map(|(_, recipe)| recipe)
        .expect("a committed recipe")
}

/// The committed beat-loop recipe: one replacement at decoded `0xD6234` (run-time `0x080D6234`)
/// whose precondition window runs to `0xD66F4` inclusive (run-time `0x080D66F4`).
fn beat_loop() -> RecipeV2 {
    committed("beat-loop-16-plays-32.json")
}

fn check(recipe: &RecipeV2, ranges: &str) -> Result<(), RecipeError> {
    check_recipe_against_protected_set(recipe, target(), &set(ranges))
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
        check(&beat_loop(), "08000800 080d6233\n080d66f5 080d7000\n"),
        Ok(())
    );
}

#[test]
fn finds_an_overlap_in_any_line_of_an_unsorted_set() {
    let result = check(
        &beat_loop(),
        "08e00000 08e00010\n08000800 08000805\n080d66f4 080d66f4\n",
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
fn refuses_a_set_that_covers_the_version_string_for_every_recipe() {
    // Every rebuild writes the version string (decoded 0x740..0x744), replacements or not.
    let version_marker = committed("version-marker-0.10.json");
    let covering = "08000600 0800074f\n";

    for recipe in [&version_marker, &beat_loop()] {
        assert_eq!(
            check(recipe, covering),
            Err(RecipeError::ProtectedSetVersion {
                start: 0x0800_0740,
                last: 0x0800_0743,
                set_start: 0x0800_0600,
                set_last: 0x0800_074f,
            })
        );
    }
    // Either side of it is fine for a recipe without replacements.
    assert_eq!(
        check(&version_marker, "08000600 0800073f\n08000744 08000fff\n"),
        Ok(())
    );
}

#[test]
fn refuses_a_set_parsed_for_another_target() {
    let fixture = Fixture::new();
    let synthetic = fixture.target();
    let set = set("08000800 08000805\n");

    assert_eq!(
        check_recipe_against_protected_set(&fixture.recipe(Vec::new()), &synthetic, &set),
        Err(RecipeError::ProtectedSetRelease {
            set: RELEASE.to_owned(),
            target: "synthetic".to_owned()
        })
    );
}

/// The entry points run the set check themselves, before the input is read: `b"not it"` is never
/// looked at.
#[test]
fn the_entry_points_refuse_an_overlap_in_their_checks() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let set = ProtectedSet::parse("release synthetic\n08000900 08000901\n", &target)
        .expect("a synthetic set");
    let checks = RecipeChecks {
        protected_set: Some(&set),
    };
    let recipe = fixture.recipe(vec![replacement(0x900, &[0xde, 0xad])]);
    let refused = Err(RecipeError::ProtectedSet {
        index: 0,
        what: "span",
        start: 0x0800_0900,
        last: 0x0800_0901,
        set_start: 0x0800_0900,
        set_last: 0x0800_0901,
    });

    assert_eq!(
        apply_recipe_v2_to(&recipe, &target, b"not it", checks).map(|_| ()),
        refused
    );
    assert_eq!(
        precondition_hashes(&recipe, &target, b"not it", checks).map(|_| ()),
        refused
    );
    // Without the set the same recipe applies.
    assert!(apply_recipe_v2_to(&recipe, &target, &fixture.update, RecipeChecks::NONE).is_ok());
}
