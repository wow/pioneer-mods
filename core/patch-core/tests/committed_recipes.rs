//! Every committed recipe (`recipes/**/*.json`) must be a valid schema-v2 recipe for a known
//! release, pass every check that needs no firmware, and pin its output identities; across the
//! committed recipes of a release, precondition windows must be identical or disjoint. Applying
//! them needs the official file; see the owner-input tests.

#[path = "common/recipe_files.rs"]
mod recipe_files;

use patch_core::xdj700::{check_recipe_v2, recipe_target};
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
fn committed_precondition_windows_are_identical_or_disjoint_across_recipes() {
    let recipes = committed_recipes();

    let result = check_windows_across(recipes.iter().map(|(_, recipe)| recipe));

    assert_eq!(result, Ok(()));
}
