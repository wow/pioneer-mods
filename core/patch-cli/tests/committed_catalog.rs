//! The committed catalog (`catalog/`): it loads and checks, agrees with the engine's pins, and
//! names only recipes that pass the engine's checks. Loader refusals on copies of it.

mod common;

use common::write_bytes;
use patch_cli::catalog::load_catalog;
use patch_core::xdj700::{MAX_MAIN_GROWTH, check_recipe_v2, recipe_target};
use std::path::{Path, PathBuf};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

#[test]
fn the_committed_catalog_loads_and_checks() {
    let loaded = load_catalog(&repo_root()).expect("committed catalog");
    let catalog = loaded.catalog();

    let player = catalog.player("xdj700-v1.15").expect("the XDJ-700 v1.15");
    assert_eq!(player.screens, ["main", "perform"]);
    assert!(catalog.screen("xdj700-v1.15", "perform").is_some());
    assert!(catalog.feature("beat-loop-1-to-32").is_some());
    assert!(catalog.feature("beat-loop-16-plays-32").is_some());
}

#[test]
fn each_player_carries_the_engine_pins_and_budget() {
    let loaded = load_catalog(&repo_root()).expect("committed catalog");

    for player in &loaded.catalog().players {
        let target = recipe_target(&player.id).expect("a release the engine knows");
        let block = target.release.version_block.expect("a version block");
        assert_eq!(player.firmware.upd_sha256, target.release.upd_sha256);
        assert_eq!(
            player.firmware.application_sha256,
            block.stock_application_sha256
        );
        assert_eq!(
            player.budgets.compressed_main_growth_bytes,
            MAX_MAIN_GROWTH as u64
        );
    }
}

#[test]
fn each_named_recipe_passes_the_engine_checks() {
    let loaded = load_catalog(&repo_root()).expect("committed catalog");

    assert!(!loaded.recipes().is_empty());
    for (path, recipe) in loaded.recipes() {
        let target = recipe_target(&recipe.target.release).expect("a known release");
        check_recipe_v2(recipe, target).unwrap_or_else(|error| panic!("{path}: {error}"));
    }
}

/// A copy of the committed catalog and the recipes it names, under a temporary root.
fn copy() -> tempfile::TempDir {
    let root = tempfile::tempdir().expect("tempdir");
    for dir in ["catalog", "recipes"] {
        copy_tree(&repo_root().join(dir), &root.path().join(dir));
    }
    root
}

fn copy_tree(from: &Path, to: &Path) {
    std::fs::create_dir_all(to).expect("create dir");
    for entry in std::fs::read_dir(from).expect("read dir") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            copy_tree(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

fn refusal(root: &Path) -> String {
    format!("{:#}", load_catalog(root).expect_err("refused"))
}

#[test]
fn a_copy_loads_and_misplaced_files_are_refused() {
    let root = copy();
    load_catalog(root.path()).expect("an unchanged copy");
    let catalog = root.path().join("catalog");

    // A file not named after its id.
    let feature = catalog.join("features/beat-loop-1-to-32.json");
    let renamed = catalog.join("features/beat-loop.json");
    std::fs::rename(&feature, &renamed).expect("rename");
    assert!(refusal(root.path()).contains("must be named beat-loop-1-to-32.json"));
    std::fs::rename(&renamed, &feature).expect("rename back");

    // Anything but .json files (and a README.md).
    write_bytes(&catalog.join("features/notes.txt"), b"notes");
    assert!(refusal(root.path()).contains("unexpected entry 'notes.txt'"));
    std::fs::remove_file(catalog.join("features/notes.txt")).expect("remove");
    write_bytes(&catalog.join("feature"), b"");
    assert!(refusal(root.path()).contains("unexpected entry 'feature'"));
    std::fs::remove_file(catalog.join("feature")).expect("remove");

    // A screen for another player under this player's directory.
    let main = catalog.join("screens/xdj700-v1.15/main.json");
    let text = std::fs::read_to_string(&main).expect("read");
    write_bytes(
        &main,
        text.replace(
            "\"player\": \"xdj700-v1.15\"",
            "\"player\": \"xdj700-v1.16\"",
        )
        .as_bytes(),
    );
    assert!(refusal(root.path()).contains("is for player xdj700-v1.16, but lies under"));
}

#[test]
fn a_missing_recipe_or_an_inconsistent_catalog_is_refused() {
    let root = copy();
    std::fs::remove_file(
        root.path()
            .join("recipes/xdj700-v1.15/beat-loop-16-plays-32.json"),
    )
    .expect("remove");
    let message = refusal(root.path());
    assert!(
        message.contains("failed to read recipe")
            && message.contains("beat-loop-16-plays-32.json")
            && message.contains("No such file or directory"),
        "{message}"
    );

    let root = copy();
    let screen = root
        .path()
        .join("catalog/screens/xdj700-v1.15/perform.json");
    let text = std::fs::read_to_string(&screen).expect("read");
    write_bytes(
        &screen,
        text.replace("\"count\": 6", "\"count\": 5").as_bytes(),
    );
    let message = refusal(root.path());
    assert!(
        message.contains("6 labels for perform.beat_loop.pad, which has 5"),
        "{message}"
    );
}

#[test]
fn notes_are_skipped_and_a_missing_catalog_or_an_invalid_recipe_is_refused() {
    let root = copy();
    let catalog = root.path().join("catalog");
    write_bytes(&catalog.join("screens/README.md"), b"notes");
    write_bytes(&catalog.join("features/.DS_Store"), b"");
    load_catalog(root.path()).expect("README.md and .DS_Store are skipped");

    let recipe = root
        .path()
        .join("recipes/xdj700-v1.15/beat-loop-1-to-32.json");
    let text = std::fs::read_to_string(&recipe).expect("read");
    write_bytes(
        &recipe,
        text.replace("\"label\": \"Ver1.16\"", "\"label\": \"1.16\"")
            .as_bytes(),
    );
    let message = refusal(root.path());
    assert!(message.contains("refusing recipe"), "{message}");

    let empty = tempfile::tempdir().expect("tempdir");
    assert!(refusal(empty.path()).contains("no such directory"));
}

#[cfg(unix)]
#[test]
fn symbolic_links_are_refused() {
    use std::os::unix::fs::symlink;

    // A catalog file linked to a file outside the tree.
    let root = copy();
    let outside = tempfile::tempdir().expect("tempdir");
    let feature = root.path().join("catalog/features/beat-loop-1-to-32.json");
    std::fs::rename(&feature, outside.path().join("f.json")).expect("move");
    symlink(outside.path().join("f.json"), &feature).expect("link");
    assert!(refusal(root.path()).contains("is a symbolic link"));

    // A recipe directory linked elsewhere.
    let root = copy();
    let recipes = root.path().join("recipes/xdj700-v1.15");
    let moved = outside.path().join("recipes");
    std::fs::rename(&recipes, &moved).expect("move");
    symlink(&moved, &recipes).expect("link");
    assert!(refusal(root.path()).contains("is a symbolic link"));

    // The catalog directory itself.
    let root = copy();
    let catalog = root.path().join("catalog");
    let moved = outside.path().join("catalog");
    std::fs::rename(&catalog, &moved).expect("move");
    symlink(&moved, &catalog).expect("link");
    assert!(refusal(root.path()).contains("a symbolic link"));

    // A dangling directory link is refused, not read as empty.
    let root = copy();
    let features = root.path().join("catalog/features");
    std::fs::remove_dir_all(&features).expect("remove");
    symlink(outside.path().join("missing"), &features).expect("link");
    assert!(refusal(root.path()).contains("is a symbolic link"));
}

/// Replaces `from` with `to` in every JSON file under `root`, and in every file and directory
/// name, as if the copy were for another release.
fn rename_release(root: &Path, from: &str, to: &str) {
    for entry in std::fs::read_dir(root).expect("read dir") {
        let path = entry.expect("entry").path();
        if path.is_dir() {
            rename_release(&path, from, to);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let text = std::fs::read_to_string(&path).expect("read");
            write_bytes(&path, text.replace(from, to).as_bytes());
        }
        let name = path
            .file_name()
            .expect("name")
            .to_string_lossy()
            .into_owned();
        if name.contains(from) {
            std::fs::rename(&path, path.with_file_name(name.replace(from, to))).expect("rename");
        }
    }
}

#[test]
fn a_catalog_the_engine_would_refuse_is_refused() {
    // A release the engine does not pin, consistent within the catalog.
    let root = copy();
    rename_release(root.path(), "xdj700-v1.15", "xdj700-v9.99");
    let message = refusal(root.path());
    assert!(
        message.contains("unknown release \"xdj700-v9.99\""),
        "{message}"
    );

    // Player and recipes that agree on another application pin.
    let root = copy();
    let application = "1875381b56d065a2b0a97a63b64ead5ce71397c521b7a62713c5bb4a0e055939";
    rename_release(root.path(), application, &"a".repeat(64));
    let message = refusal(root.path());
    assert!(
        message.contains("differ from the engine's release"),
        "{message}"
    );

    // Another budget.
    let root = copy();
    let player = root.path().join("catalog/players/xdj700-v1.15.json");
    let text = std::fs::read_to_string(&player).expect("read");
    write_bytes(&player, text.replace("262144", "262145").as_bytes());
    assert!(refusal(root.path()).contains("differ from the engine's release"));

    // A recipe the engine's own checks refuse (its label is not higher than the release's).
    let root = copy();
    let recipe = root
        .path()
        .join("recipes/xdj700-v1.15/beat-loop-1-to-32.json");
    let text = std::fs::read_to_string(&recipe).expect("read");
    write_bytes(
        &recipe,
        text.replace("\"Ver1.16\"", "\"Ver1.15\"").as_bytes(),
    );
    let message = refusal(root.path());
    assert!(
        message.contains("recipe recipes/xdj700-v1.15/beat-loop-1-to-32.json")
            && message.contains("not higher"),
        "{message}"
    );
}

#[test]
fn a_screen_directory_must_hold_a_known_players_screens() {
    let root = copy();
    std::fs::create_dir(root.path().join("catalog/screens/xdj700-v1.16")).expect("dir");
    assert!(refusal(root.path()).contains("unexpected entry 'xdj700-v1.16'"));

    let root = copy();
    let screens = root.path().join("catalog/screens/xdj700-v1.15");
    for screen in ["main.json", "perform.json"] {
        std::fs::remove_file(screens.join(screen)).expect("remove");
    }
    assert!(refusal(root.path()).contains("it holds no screen"));
}
