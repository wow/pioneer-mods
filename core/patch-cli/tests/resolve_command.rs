//! `patch-cli resolve` on the committed catalog: every choice on or off with its reason, and the
//! fragments. Needs no firmware.

mod common;

use common::write_bytes;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn profile(features: &[&str], maturity: &str) -> Value {
    json!({
        "schema_version": 1, "player": "xdj700-v1.15", "screens": { "perform": "stock" },
        "features": features, "label": "Ver1.16", "reported_version": "0.12",
        "maturity": maturity
    })
}

fn run(profile: &Value, root: &Path) -> Output {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("profile.json");
    write_bytes(&path, &serde_json::to_vec(profile).expect("json"));
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("resolve")
        .arg("--profile")
        .arg(&path)
        .arg("--root")
        .arg(root)
        .output()
        .expect("run patch-cli resolve")
}

fn stdout(output: &Output) -> String {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    assert!(!output.status.success());
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn a_feature_is_on_with_its_recipe_and_limits() {
    let out = stdout(&run(
        &profile(&["beat-loop-1-to-32"], "experimental"),
        &repo_root(),
    ));

    for line in [
        "player: xdj700-v1.15",
        "accepts: experimental and stable",
        "screen main: stock",
        "screen perform: stock",
        "feature beat-loop-1-to-32: on (experimental, recipes/xdj700-v1.15/beat-loop-1-to-32.json)",
        "  limit: The lit pad follows the stock lengths",
        "fragments: 1",
        "fragment[0]: recipes/xdj700-v1.15/beat-loop-1-to-32.json (experimental; feature \
         beat-loop-1-to-32)",
        "tier: experimental",
    ] {
        assert!(out.contains(line), "{line}\n{out}");
    }
}

#[test]
fn conflicts_and_the_maturity_setting_switch_features_off() {
    let both = profile(
        &["beat-loop-1-to-32", "beat-loop-16-plays-32"],
        "experimental",
    );
    let out = stdout(&run(&both, &repo_root()));
    assert!(out.contains(
        "feature beat-loop-1-to-32: off: it conflicts with beat-loop-16-plays-32, also chosen; \
         choose one"
    ));
    assert!(out.contains("tier: none; nothing to build"), "{out}");

    let out = stdout(&run(
        &profile(&["beat-loop-16-plays-32"], "stable"),
        &repo_root(),
    ));
    assert!(out.contains(
        "feature beat-loop-16-plays-32: off: its implementation for xdj700-v1.15 is \
         experimental; the profile accepts stable only"
    ));

    let mut unknown = profile(&["no-such-feature"], "experimental");
    unknown["screens"] = json!({ "perform": "no-such-skin" });
    let out = stdout(&run(&unknown, &repo_root()));
    assert!(out.contains("feature no-such-feature: off: it is not in the catalog"));
    assert!(out.contains(
        "screen perform: skin no-such-skin off: skin no-such-skin is not in the catalog; it \
         keeps the stock skin"
    ));
}

#[test]
fn an_unusable_profile_or_catalog_is_refused() {
    let mut other = profile(&[], "experimental");
    other["player"] = json!("xdj1000mk2-v1.45");
    assert!(stderr(&run(&other, &repo_root())).contains("player xdj1000mk2-v1.45 is not in"));

    let dev = profile(&[], "dev");
    assert!(stderr(&run(&dev, &repo_root())).contains("the profile is invalid"));

    let mut unknown_field = profile(&[], "experimental");
    unknown_field["skins"] = json!({});
    assert!(stderr(&run(&unknown_field, &repo_root())).contains("failed to parse profile"));

    let empty = tempfile::tempdir().expect("tempdir");
    assert!(stderr(&run(&profile(&[], "stable"), empty.path())).contains("no such directory"));
}

#[test]
fn the_release_rules_for_the_label_and_reported_version_apply() {
    let mut not_lower = profile(&["beat-loop-1-to-32"], "experimental");
    not_lower["reported_version"] = json!("1.20");
    assert!(stderr(&run(&not_lower, &repo_root())).contains("is not lower than the official"));

    let mut not_higher = profile(&["beat-loop-1-to-32"], "experimental");
    not_higher["label"] = json!("Ver1.10");
    assert!(stderr(&run(&not_higher, &repo_root())).contains("is not higher"));

    // Another reported version than the recipe's own: a new update, experimental at most.
    let mut other = profile(&["beat-loop-1-to-32"], "experimental");
    other["reported_version"] = json!("0.13");
    let out = stdout(&run(&other, &repo_root()));
    assert!(
        out.contains("; under another label or reported version than its own)"),
        "{out}"
    );
}
