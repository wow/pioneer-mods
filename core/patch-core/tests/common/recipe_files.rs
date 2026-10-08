//! The committed recipes (`recipes/**/*.json`), shared by the CI and owner-input tests of
//! `patch-core` (included with `#[path]`) so that both agree on which files are recipes.

use patch_schema::RecipeV2;
use std::path::{Path, PathBuf};

/// Every committed recipe, sorted by path, with its parsed contents.
pub fn committed_recipes() -> Vec<(PathBuf, RecipeV2)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../recipes");
    let mut files = Vec::new();
    let mut directories = vec![root.clone()];
    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory).expect("read recipes directory") {
            let path = entry.expect("directory entry").path();
            if path.is_dir() {
                directories.push(path);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "json")
            {
                files.push(path);
            }
        }
    }
    assert!(!files.is_empty(), "no recipes under {}", root.display());
    files.sort();
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
