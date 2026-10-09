//! Every committed recipe (`recipes/**/*.json`) must be a valid schema-v2 recipe for a known
//! release, pass every check that needs no firmware, and pin its output identities; across the
//! committed recipes of a release, precondition windows must be disjoint. Applying them needs the
//! official file; see the owner-input tests.

#[path = "common/recipe_files.rs"]
mod recipe_files;

use patch_core::xdj700::{
    ProtectedSet, check_recipe_against_protected_set, check_recipe_v2, recipe_target,
};
use patch_schema::check_windows_across;
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
fn committed_precondition_windows_are_disjoint_across_recipes() {
    let recipes = committed_recipes();

    let result = check_windows_across(recipes.iter().map(|(_, recipe)| recipe));

    assert_eq!(result, Ok(()));
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
    for known in ["version-marker-0.10", "beat-loop-16-plays-32"] {
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
