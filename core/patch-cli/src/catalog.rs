//! Loading the catalog (`catalog/` under the repository root) and the recipes its implementations
//! name, without trusting the files: each file is size-capped, parsed strictly and checked on its
//! own before anything it names is read, then the whole is checked ([`Catalog::check`]), and so
//! is every player and recipe against the engine's own release pins and checks. What loads here
//! is what the engine accepts, short of the firmware itself.

use crate::recipe::{read_capped, read_recipe_versioned};
use anyhow::{Context, Result, bail};
use patch_core::xdj700::{
    MAX_MAIN_GROWTH, check_label_and_version, check_recipe_v2, recipe_target, unknown_release,
};
use patch_schema::catalog::{
    Catalog, CatalogEntry, CheckedCatalog, Feature, Player, Profile, Resolution, ResolveError,
    Screen, Skin, resolve,
};
use patch_schema::{RecipeV2, SCHEMA_VERSION_V2};
use serde::de::DeserializeOwned;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Largest catalog file read: entries are a few KB.
pub const MAX_CATALOG_FILE_LEN: u64 = 256 * 1024;

/// The directories `catalog/` may hold.
const DIRS: [&str; 4] = ["players", "screens", "features", "skins"];

/// Names skipped wherever they appear: notes for contributors, and the file Finder leaves.
const SKIPPED: [&str; 2] = ["README.md", ".DS_Store"];

/// Loads and checks `root/catalog`: `players/<id>.json`, `screens/<player>/<screen>.json`,
/// `features/<id>.json` and `skins/<id>.json`, each file named after its id, and the recipes the
/// implementations name (paths relative to `root`). `catalog/` must exist; a missing
/// subdirectory is empty. Anything else (but a `README.md` or `.DS_Store`) is refused, so a
/// misnamed file cannot be skipped silently, and so is a symbolic link anywhere under `root`
/// that the walk, a recipe path or a player's flashing guide meets: it could point outside the
/// tree. Each player's flashing guide (`restore.guide`) must be a file under `root`.
///
/// # Errors
///
/// An unreadable, oversized, malformed or invalid file, a misplaced one, a recipe that is not a
/// valid schema-v2 recipe, or a [`CatalogError`](patch_schema::catalog::CatalogError).
pub fn load_catalog(root: &Path) -> Result<CheckedCatalog> {
    let dir = root.join("catalog");
    if !is_dir(&dir)? {
        bail!("refusing catalog '{}': no such directory", dir.display());
    }
    let mut catalog = Catalog::default();
    for name in entries(&dir)? {
        if !DIRS.contains(&name.as_str()) {
            bail!(
                "refusing catalog '{}': unexpected entry '{name}'; it holds {}",
                dir.display(),
                DIRS.join(", ")
            );
        }
    }
    for path in json_files(&dir.join("players"))? {
        let player = read_entry::<Player>(&path)?;
        let guide = &player.restore.guide;
        refuse_links(root, guide, "flashing guide")?;
        if !root.join(guide).is_file() {
            bail!(
                "refusing catalog file '{}': its flashing guide '{guide}' is not a file",
                path.display()
            );
        }
        catalog.players.push(player);
    }
    let screens = dir.join("screens");
    for player in entries(&screens)? {
        if !is_dir(&screens.join(&player))? || catalog.player(&player).is_none() {
            bail!(
                "refusing catalog directory '{}': unexpected entry '{player}'; it holds a \
                 directory per player in catalog/players",
                screens.display()
            );
        }
        let files = json_files(&screens.join(&player))?;
        if files.is_empty() {
            bail!(
                "refusing catalog directory '{}': it holds no screen",
                screens.join(&player).display()
            );
        }
        for path in files {
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
    let refusing = || format!("refusing catalog '{}'", dir.display());
    let checked = catalog.into_checked(recipes).with_context(refusing)?;
    check_against_engine(checked.catalog(), checked.recipes()).with_context(refusing)?;
    Ok(checked)
}

/// Resolves `profile` against `catalog` as every front end should: the release's label and
/// reported-version rules first (the engine's, as a build applies them), then
/// [`resolve`].
///
/// # Errors
///
/// An invalid profile, a player the catalog lacks, a label or reported version the release
/// refuses, or a search too large.
pub fn resolve_profile(catalog: &CheckedCatalog, profile: &Profile) -> Result<Resolution> {
    profile.validate().map_err(ResolveError::Profile)?;
    if catalog.catalog().player(&profile.player).is_some() {
        let target =
            recipe_target(&profile.player).ok_or_else(|| unknown_release(&profile.player))?;
        check_label_and_version(&profile.label, &profile.reported_version, target)?;
    }
    Ok(resolve(catalog, profile)?)
}

/// Reads an owner's profile: size-capped and parsed strictly (unknown fields and repeated keys
/// refused). Whether it is valid is for [`patch_schema::catalog::resolve`] to say.
pub fn read_profile(path: &Path) -> Result<Profile> {
    let raw = read_capped(
        path,
        MAX_CATALOG_FILE_LEN,
        "profile",
        "profile",
        "any profile",
    )?;
    serde_json::from_slice(&raw)
        .with_context(|| format!("failed to parse profile '{}'", path.display()))
}

/// Every player is a release the engine pins, with the same pins and budget, and every recipe
/// passes the engine's firmware-free checks for its release.
fn check_against_engine(catalog: &Catalog, recipes: &BTreeMap<String, RecipeV2>) -> Result<()> {
    for player in &catalog.players {
        let target = recipe_target(&player.id).ok_or_else(|| unknown_release(&player.id))?;
        let release = &target.release;
        let stock_application = release
            .version_block
            .map(|block| block.stock_application_sha256);
        let firmware = &player.firmware;
        let agrees = firmware.upd_sha256.eq_ignore_ascii_case(release.upd_sha256)
            && stock_application
                .is_some_and(|pin| firmware.application_sha256.eq_ignore_ascii_case(pin))
            && player.budgets.compressed_main_growth_bytes == MAX_MAIN_GROWTH as u64;
        if !agrees {
            bail!(
                "player {}: its firmware pins or budget differ from the engine's release {}",
                player.id,
                target.id
            );
        }
    }
    for (path, recipe) in recipes {
        let target = recipe_target(&recipe.target.release)
            .ok_or_else(|| unknown_release(&recipe.target.release))?;
        check_recipe_v2(recipe, target).with_context(|| format!("recipe {path}"))?;
    }
    Ok(())
}

fn read_entry<T: CatalogEntry + DeserializeOwned>(path: &Path) -> Result<T> {
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
        .flat_map(|f| f.implementations.values().flatten().map(|i| &i.recipe));
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
        refuse_links(root, path, "recipe")?;
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

/// Whether `path` is a directory: `false` if nothing is there; a symbolic link or anything else
/// is refused.
fn is_dir(path: &Path) -> Result<bool> {
    match std::fs::symlink_metadata(path) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error).with_context(|| format!("failed to read '{}'", path.display())),
        Ok(metadata) if metadata.is_dir() => Ok(true),
        Ok(metadata) if metadata.file_type().is_symlink() => bail!(
            "refusing catalog path '{}': a symbolic link",
            path.display()
        ),
        Ok(_) => bail!(
            "refusing catalog path '{}': not a directory",
            path.display()
        ),
    }
}

/// The names in `dir`, sorted, without the skipped ones; none if it does not exist. A symbolic
/// link or a name that is not UTF-8 is refused.
fn entries(dir: &Path) -> Result<Vec<String>> {
    if !is_dir(dir)? {
        return Ok(Vec::new());
    }
    let read_failed = || format!("failed to read catalog directory '{}'", dir.display());
    let mut names = Vec::new();
    for entry in std::fs::read_dir(dir).with_context(read_failed)? {
        let entry = entry.with_context(read_failed)?;
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            bail!(
                "refusing catalog directory '{}': a name is not UTF-8",
                dir.display()
            );
        };
        if entry.file_type().with_context(read_failed)?.is_symlink() {
            bail!(
                "refusing catalog directory '{}': '{name}' is a symbolic link",
                dir.display()
            );
        }
        if !SKIPPED.contains(&name) {
            names.push(name.to_owned());
        }
    }
    names.sort();
    Ok(names)
}

/// Refuses a symbolic link at any component of `path` (relative, validated) under `root`; `what`
/// names the file in messages ("recipe").
fn refuse_links(root: &Path, path: &str, what: &str) -> Result<()> {
    let mut at = root.to_path_buf();
    for component in path.split('/') {
        at.push(component);
        let metadata = std::fs::symlink_metadata(&at)
            .with_context(|| format!("failed to read {what} '{}'", root.join(path).display()))?;
        if metadata.file_type().is_symlink() {
            bail!(
                "refusing {what} '{}': '{}' is a symbolic link",
                root.join(path).display(),
                at.display()
            );
        }
    }
    Ok(())
}

/// The `.json` files in `dir`, sorted; anything else (but the skipped names) is refused.
fn json_files(dir: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for name in entries(dir)? {
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
