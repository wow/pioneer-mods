//! Loading the catalog (`catalog/` under the repository root) and the recipes its implementations
//! name, without trusting the files: each file is size-capped, parsed strictly and checked on its
//! own before anything it names is read, then the whole is checked ([`Catalog::check`]).

use crate::recipe::{read_capped, read_recipe_versioned};
use anyhow::{Context, Result, bail};
use patch_schema::catalog::{Catalog, EntryError, Feature, Player, Screen, Skin};
use patch_schema::{RecipeV2, SCHEMA_VERSION_V2};
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Largest catalog file read: entries are a few KB.
pub const MAX_CATALOG_FILE_LEN: u64 = 256 * 1024;

/// The directories `catalog/` may hold, besides a `README.md`.
const DIRS: [&str; 4] = ["players", "screens", "features", "skins"];

/// A checked catalog and the recipes its implementations name, by path.
#[derive(Debug, Clone)]
pub struct LoadedCatalog {
    pub catalog: Catalog,
    pub recipes: BTreeMap<String, RecipeV2>,
}

/// Loads and checks `root/catalog`: `players/<id>.json`, `screens/<player>/<screen>.json`,
/// `features/<id>.json` and `skins/<id>.json`, each file named after its id, and the recipes the
/// implementations name (paths relative to `root`). A missing directory is empty; anything else
/// in `catalog/` is refused, so a misnamed file cannot be skipped silently.
///
/// # Errors
///
/// An unreadable, oversized, malformed or invalid file, a misplaced one, a recipe that is not a
/// valid schema-v2 recipe, or a [`CatalogError`](patch_schema::catalog::CatalogError).
pub fn load_catalog(root: &Path) -> Result<LoadedCatalog> {
    let dir = root.join("catalog");
    let mut catalog = Catalog::default();
    for name in entries(&dir)? {
        if !(name == "README.md" || DIRS.contains(&name.as_str())) {
            bail!(
                "refusing catalog '{}': unexpected entry '{name}'; it holds {} and a README.md",
                dir.display(),
                DIRS.join(", ")
            );
        }
    }
    for path in json_files(&dir.join("players"))? {
        catalog.players.push(read_entry::<Player>(&path)?);
    }
    let screens = dir.join("screens");
    for player in entries(&screens)? {
        for path in json_files(&screens.join(&player))? {
            let screen = read_entry::<Screen>(&path)?;
            if screen.player != player {
                bail!(
                    "refusing catalog file '{}': it is for player {}, but lies under '{player}'",
                    path.display(),
                    screen.player
                );
            }
            catalog.screens.push(screen);
        }
    }
    for path in json_files(&dir.join("features"))? {
        catalog.features.push(read_entry::<Feature>(&path)?);
    }
    for path in json_files(&dir.join("skins"))? {
        catalog.skins.push(read_entry::<Skin>(&path)?);
    }
    let recipes = read_recipes(root, &catalog)?;
    catalog
        .check(&recipes)
        .with_context(|| format!("refusing catalog '{}'", dir.display()))?;
    Ok(LoadedCatalog { catalog, recipes })
}

/// A catalog file's type: how it names itself and checks itself.
trait Entry: DeserializeOwned {
    fn id(&self) -> &str;
    fn validate(&self) -> Result<(), EntryError>;
}

macro_rules! entry {
    ($($kind:ty),*) => {$(
        impl Entry for $kind {
            fn id(&self) -> &str {
                &self.id
            }
            fn validate(&self) -> Result<(), EntryError> {
                <$kind>::validate(self)
            }
        }
    )*};
}

entry!(Player, Screen, Feature, Skin);

fn read_entry<T: Entry>(path: &Path) -> Result<T> {
    let raw = read_capped(
        path,
        MAX_CATALOG_FILE_LEN,
        "catalog file",
        "catalog file",
        "any catalog file",
    )?;
    let entry: T = serde_json::from_slice(&raw)
        .with_context(|| format!("failed to parse catalog file '{}'", path.display()))?;
    entry
        .validate()
        .with_context(|| format!("refusing catalog file '{}'", path.display()))?;
    let stem = path.file_stem().and_then(|stem| stem.to_str());
    if stem != Some(entry.id()) {
        bail!(
            "refusing catalog file '{}': its id is {}; the file must be named {}.json",
            path.display(),
            entry.id(),
            entry.id()
        );
    }
    Ok(entry)
}

/// Every recipe an implementation names, read from `root` (paths already validated: relative,
/// under `recipes/`, no `..`).
fn read_recipes(root: &Path, catalog: &Catalog) -> Result<BTreeMap<String, RecipeV2>> {
    let features = catalog
        .features
        .iter()
        .flat_map(|f| f.implementations.values().map(|i| &i.recipe));
    let skins = catalog
        .skins
        .iter()
        .flat_map(|s| s.implementations.values().flatten().map(|i| &i.recipe));
    let mut recipes = BTreeMap::new();
    for path in features.chain(skins) {
        if recipes.contains_key(path) {
            continue;
        }
        let file = root.join(path);
        let (raw, schema_version) = read_recipe_versioned(&file)?;
        if schema_version != SCHEMA_VERSION_V2 {
            bail!(
                "refusing recipe '{}': schema_version {schema_version}; the catalog names \
                 schema-v2 recipes only",
                file.display()
            );
        }
        let recipe: RecipeV2 = serde_json::from_slice(&raw)
            .with_context(|| format!("failed to parse recipe '{}'", file.display()))?;
        recipe
            .validate()
            .with_context(|| format!("refusing recipe '{}'", file.display()))?;
        recipes.insert(path.clone(), recipe);
    }
    Ok(recipes)
}

/// The names in `dir`, sorted; none if it does not exist.
fn entries(dir: &Path) -> Result<Vec<String>> {
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let read_failed = || format!("failed to read catalog directory '{}'", dir.display());
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(read_failed)? {
        let name = entry.with_context(read_failed)?.file_name();
        let Some(name) = name.to_str() else {
            bail!(
                "refusing catalog directory '{}': a name is not UTF-8",
                dir.display()
            );
        };
        names.push(name.to_owned());
    }
    names.sort();
    Ok(names)
}

/// The `.json` files in `dir`, sorted; anything else but a `README.md` is refused.
fn json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for name in entries(dir)? {
        if name == "README.md" {
            continue;
        }
        if !name.ends_with(".json") {
            bail!(
                "refusing catalog directory '{}': unexpected entry '{name}'; it holds .json files",
                dir.display()
            );
        }
        files.push(dir.join(name));
    }
    Ok(files)
}
