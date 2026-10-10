//! Owner-input composition of the committed recipes on the official XDJ-700 v1.15 update. Prints
//! no bytes. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-core --test official_compose -- --ignored
//! ```

#[path = "common/official_pins.rs"]
mod official_pins;
#[path = "common/recipe_files.rs"]
mod recipe_files;

use official_pins::{
    STAGE3_RECIPE, STAGE5_APPLICATION_SHA256, STAGE5_RECIPE, STAGE5_REPORTED_VERSION,
    STAGE5_UPD_SHA256, STAGE7_APPLICATION_SHA256, STAGE7_RECIPE, STAGE7_REPORTED_VERSION,
    STAGE7_UPD_SHA256, UPD_ENV, UPD_SHA256,
};
use patch_core::read_firmware;
use patch_core::xdj700::{Composition, RecipeChecks, compose_recipes};
use patch_schema::RecipeV2;
use recipe_files::committed_recipes;
use std::path::PathBuf;

fn official_upd() -> Vec<u8> {
    let path = std::env::var_os(UPD_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {UPD_ENV} to an owner-supplied official XDJ700.UPD"));
    let (identity, bytes) = read_firmware(&path).expect("read official UPD");
    assert_eq!(
        identity.sha256_hex, UPD_SHA256,
        "not the official v1.15 file"
    );
    bytes
}

fn committed(name: &str) -> RecipeV2 {
    committed_recipes()
        .into_iter()
        .find(|(path, _)| path.ends_with(name))
        .unwrap_or_else(|| panic!("{name} is committed"))
        .1
}

/// Stage 7 repeats stage 5's table entry exactly, so composing the two under stage 7's label and
/// version reproduces the stage-7 file byte for byte; likewise the version marker (stage 3) with
/// stage 5 reproduces the stage-5 file. Each recipe is also checked against its own pins.
#[test]
#[ignore = "needs owner-supplied firmware; see module docs"]
fn composing_committed_recipes_reproduces_their_stage_files() {
    let official = official_upd();
    let cases = [
        (
            [STAGE5_RECIPE, STAGE7_RECIPE],
            STAGE7_REPORTED_VERSION,
            STAGE7_APPLICATION_SHA256,
            STAGE7_UPD_SHA256,
        ),
        (
            [STAGE3_RECIPE, STAGE5_RECIPE],
            STAGE5_REPORTED_VERSION,
            STAGE5_APPLICATION_SHA256,
            STAGE5_UPD_SHA256,
        ),
    ];

    for (names, reported_version, application_sha256, upd_sha256) in cases {
        let fragments = names.map(committed);
        let composition = Composition {
            label: "Ver1.16",
            reported_version,
        };

        let composed = compose_recipes(&fragments, composition, &official, RecipeChecks::NONE)
            .unwrap_or_else(|error| panic!("{names:?}: {error}"));

        assert_eq!(
            composed.rebuilt.application_sha256(),
            application_sha256,
            "{names:?}"
        );
        assert_eq!(composed.rebuilt.sha256(), upd_sha256, "{names:?}");
        assert_eq!(composed.fragments.len(), 2);
    }
}
