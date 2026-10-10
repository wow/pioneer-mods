//! Every committed recipe (`recipes/**/*.json`) must be a valid schema-v2 recipe for a known
//! release, pass every check that needs no firmware, and pin its output identities; across the
//! committed recipes of a release, precondition windows must be disjoint (or belong to identical
//! replacements). Applying them needs the official file; see the owner-input tests.

#[path = "common/recipe_files.rs"]
mod recipe_files;

use patch_core::xdj700::{
    ProtectedSet, check_recipe_against_protected_set, check_recipe_v2, recipe_target,
};
use patch_schema::{ImageEdit, check_windows_across};
use recipe_files::committed_recipes;

#[test]
fn every_committed_recipe_passes_the_firmware_free_checks() {
    for (path, recipe) in committed_recipes() {
        let name = path.display();
        let target = recipe_target(&recipe.target.release)
            .unwrap_or_else(|| panic!("{name}: unknown release {}", recipe.target.release));

        assert_eq!(check_recipe_v2(&recipe, target), Ok(()), "{name}");
        let expected = recipe.expected.as_ref();
        assert!(
            expected.is_some_and(|e| e.application_sha256.is_some() && e.upd_sha256.is_some()),
            "{name}: committed recipes pin both output identities"
        );
        let file_stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .expect("UTF-8");
        assert!(
            recipe.recipe_id.ends_with(file_stem),
            "{name}: recipe_id {:?} should end with the file name",
            recipe.recipe_id
        );
    }
}

#[test]
fn committed_recipe_ids_are_distinct() {
    let recipes = committed_recipes();
    let mut ids: Vec<&str> = recipes.iter().map(|(_, r)| r.recipe_id.as_str()).collect();
    ids.sort_unstable();
    let before = ids.len();
    ids.dedup();

    // `precondition` skips committed recipes with the draft's id, so ids must name one recipe.
    assert_eq!(ids.len(), before, "two committed recipes share a recipe_id");
}

#[test]
fn committed_precondition_windows_are_disjoint_across_recipes() {
    let recipes = committed_recipes();

    let result = check_windows_across(recipes.iter().map(|(_, recipe)| recipe));

    assert_eq!(result, Ok(()));
}

/// BEAT LOOP 1, 2, 4, 8, 16, 32 draws one label per button: the six state images of a button share
/// their erase box, glyph box and mask, and each `purpose` names its button. A slip in one copy
/// would be built into the pinned output, so no other test would see it; it would show only on
/// the unit, perhaps in a state with no known trigger.
#[test]
fn each_relabelled_button_shares_one_drawing() {
    let recipes = committed_recipes();
    let (_, recipe) = recipes
        .iter()
        .find(|(path, _)| path.ends_with("xdj700-v1.15/beat-loop-1-to-32.json"))
        .expect("committed");
    let mut drawings: Vec<(&ImageEdit, Vec<&str>)> = Vec::new();
    for edit in &recipe.image_edits {
        let button = edit.purpose.split_once("(pad ").expect("names its pad").1;
        let button = button.split_once(')').expect("closes").0;
        let same = |(drawn, _): &&mut (&ImageEdit, Vec<&str>)| {
            (drawn.erase, drawn.glyph.at, &drawn.glyph.alpha_hex)
                == (edit.erase, edit.glyph.at, &edit.glyph.alpha_hex)
        };
        match drawings.iter_mut().find(same) {
            Some((_, buttons)) => buttons.push(button),
            None => drawings.push((edit, vec![button])),
        }
    }

    let mut buttons: Vec<&str> = drawings
        .iter()
        .map(|(_, buttons)| {
            assert_eq!(buttons.len(), 6, "{buttons:?}: six states per drawing");
            assert!(buttons.iter().all(|b| *b == buttons[0]), "{buttons:?}");
            buttons[0]
        })
        .collect();
    buttons.sort_unstable();
    let expected: Vec<String> = (1..=6).map(|pad| format!("{pad} of 6")).collect();
    assert_eq!(buttons, expected, "one drawing per button");
}

/// The maintainer's check against the protected set measured in emulation, which is kept outside
/// the repository (`docs/xdj700-flashing.md`, section 5):
///
/// ```text
/// XDJ700_PROTECTED_SET=/path/to/set.tsv \
///   cargo test -p patch-core --test committed_recipes -- --ignored --nocapture
/// ```
///
/// Without the variable it is skipped and says so, so that the owner-input suites can run with
/// `--ignored` and only the firmware. With it, it fails unless every known v1.15 recipe was
/// checked, and prints how many recipes and ranges it checked.
#[test]
#[ignore = "needs the protected set kept outside the repository; see the doc comment"]
fn committed_recipes_avoid_the_protected_set() {
    let Some(path) = std::env::var_os("XDJ700_PROTECTED_SET") else {
        eprintln!("SKIPPED: XDJ700_PROTECTED_SET is not set, so no recipe was checked");
        return;
    };
    let text = std::fs::read_to_string(&path).expect("read the protected set");
    let target = recipe_target("xdj700-v1.15").expect("known release");
    let set = ProtectedSet::parse(&text, target).expect("a valid protected set");

    let mut checked = Vec::new();
    for (path, recipe) in committed_recipes() {
        if recipe.target.release != target.id {
            continue;
        }
        assert_eq!(
            check_recipe_against_protected_set(&recipe, target, &set),
            Ok(()),
            "{}",
            path.display()
        );
        checked.push(recipe.recipe_id);
    }

    // A renamed or moved recipe directory must not turn this into a check of nothing.
    for known in [
        "version-marker-0.10",
        "beat-loop-16-plays-32",
        "beat-loop-1-to-32",
    ] {
        assert!(
            checked.iter().any(|id| id.ends_with(known)),
            "{known} was not checked; checked: {checked:?}"
        );
    }
    eprintln!(
        "checked {} recipes against {} ranges of the protected set",
        checked.len(),
        set.len()
    );
}
