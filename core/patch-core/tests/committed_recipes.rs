//! Every committed recipe (`recipes/**/*.json`) must be a valid schema-v2 recipe for a known
//! release, pass every check that needs no firmware, and pin its output identities. Applying them
//! needs the official file; see the owner-input tests.

use patch_core::xdj700::{check_recipe_v2, recipe_target};
use patch_schema::RecipeV2;
use std::path::{Path, PathBuf};

fn recipe_files(dir: &Path, found: &mut Vec<PathBuf>) {
    for entry in std::fs::read_dir(dir).expect("read recipes directory") {
        let path = entry.expect("directory entry").path();
        if path.is_dir() {
            recipe_files(&path, found);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            found.push(path);
        }
    }
}

#[test]
fn every_committed_recipe_passes_the_firmware_free_checks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recipes");
    let mut files = Vec::new();
    recipe_files(&root, &mut files);
    assert!(!files.is_empty(), "no recipes under {}", root.display());

    for path in files {
        let name = path.display();
        let recipe: RecipeV2 = serde_json::from_slice(&std::fs::read(&path).expect("read"))
            .unwrap_or_else(|error| panic!("{name}: {error}"));
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
