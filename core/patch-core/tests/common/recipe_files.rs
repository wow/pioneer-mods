//! The committed recipes (`recipes/**/*.json`), shared by the CI and owner-input tests of
//! `patch-core` (included with `#[path]`). The walk is `patch_core::recipe_files`, the same one
//! `patch-cli precondition` uses.

use patch_schema::RecipeV2;
use std::path::{Path, PathBuf};

/// Every committed recipe, sorted by path, with its parsed contents.
pub fn committed_recipes() -> Vec<(PathBuf, RecipeV2)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recipes");
    let files = patch_core::recipe_files(&root).expect("read recipes directory");
    assert!(!files.is_empty(), "no recipes under {}", root.display());
    files
        .into_iter()
        .map(|path| {
            let raw = std::fs::read(&path).expect("read recipe");
            let recipe = serde_json::from_slice(&raw)
                .unwrap_or_else(|error| panic!("{}: {error}", path.display()));
            (path, recipe)
        })
        .collect()
}
