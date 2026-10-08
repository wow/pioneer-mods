//! Every committed recipe (`recipes/**/*.json`) must be a valid schema-v2 recipe for a known
//! release, pass every check that needs no firmware, and pin its output identities; the
//! precondition windows of all committed recipes for a release must be disjoint. Applying them
//! needs the official file; see the owner-input tests.

#[path = "common/recipe_files.rs"]
mod recipe_files;

use patch_core::xdj700::{check_recipe_v2, recipe_target};
use recipe_files::committed_recipes;
use std::ops::Range;

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

/// A precondition window of a committed recipe: its release, its range, and where it is declared.
type Window = (String, Range<u64>, String);

/// Panics unless, per release, no two of `windows` overlap. Each recipe checks its own windows;
/// this extends the rule across recipes, so that windows shifted from file to file cannot reveal
/// stock bytes one hash at a time.
fn assert_disjoint(mut windows: Vec<Window>) {
    windows.sort_by(|a, b| (&a.0, a.1.start).cmp(&(&b.0, b.1.start)));
    for pair in windows.windows(2) {
        let ((release, first, first_name), (next_release, next, next_name)) = (&pair[0], &pair[1]);
        assert!(
            release != next_release || first.end <= next.start,
            "{first_name} and {next_name}: precondition windows overlap ({first:#x?}, {next:#x?})"
        );
    }
}

#[test]
fn committed_precondition_windows_are_disjoint_across_recipes() {
    let mut windows = Vec::new();
    for (path, recipe) in committed_recipes() {
        for (index, replacement) in recipe.replacements.iter().enumerate() {
            let window = replacement.precondition_window().expect("a valid recipe");
            let name = format!("{}: replacements[{index}]", path.display());
            windows.push((recipe.target.release.clone(), window, name));
        }
    }
    assert_disjoint(windows);
}

#[test]
#[should_panic(expected = "b and a: precondition windows overlap (0x900..0x931, 0x930..0x960)")]
fn overlapping_windows_of_two_recipes_are_refused() {
    let window = |name: &str, range| ("xdj700-v1.15".to_owned(), range, name.to_owned());
    assert_disjoint(vec![
        window("a", 0x930..0x960),
        window("c", 0x1000..0x1020),
        window("b", 0x900..0x931),
    ]);
}

#[test]
fn touching_windows_and_windows_of_other_releases_are_accepted() {
    let window = |release: &str, range| (release.to_owned(), range, String::new());
    assert_disjoint(vec![
        window("xdj700-v1.15", 0x930..0x960),
        window("xdj700-v1.15", 0x900..0x930),
        window("another-release", 0x900..0x960),
    ]);
}
