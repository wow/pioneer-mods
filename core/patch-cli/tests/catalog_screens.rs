//! The committed screens' slot images (`catalog/screens/`): they agree with the committed recipes
//! that edit them, lie on the player's screen and in its application. Needs no firmware.

mod common;

use common::{copy_catalog, repo_root, write_bytes};
use patch_cli::catalog::load_catalog;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read")).expect("json")
}

#[test]
fn the_perform_pads_are_the_images_stage_7_relabels() {
    let loaded = load_catalog(&repo_root()).expect("committed catalog");
    let screen = loaded
        .catalog()
        .screen("xdj700-v1.15", "perform")
        .expect("perform");
    let images = screen.slots["beat_loop.pad"]
        .images
        .as_ref()
        .expect("pad images");
    assert_eq!(
        (images.width, images.height, images.states.len()),
        (80, 53, 6)
    );

    // Stage 7 relabels each pad's six images: pad N's are element N-1's, and no others.
    let recipe = read_json(&repo_root().join("recipes/xdj700-v1.15/beat-loop-1-to-32.json"));
    let edits = recipe["image_edits"].as_array().expect("edits");
    assert_eq!(edits.len(), 36);
    for edit in edits {
        let offset = u32::try_from(edit["offset"].as_u64().expect("offset")).expect("u32");
        let purpose = edit["purpose"].as_str().expect("purpose");
        let pad: usize = purpose
            .split("(pad ")
            .nth(1)
            .and_then(|rest| rest.split(' ').next())
            .and_then(|n| n.parse().ok())
            .expect("pad number");
        let element = &images.elements[pad - 1];
        assert!(
            element.images.values().any(|image| image.0 == offset),
            "{offset:#x} is not pad {pad}'s"
        );
    }
    let all: BTreeSet<u32> = images.offsets().collect();
    assert_eq!(all.len(), 36);

    // Left to right, one row, each box the image's size.
    let xs: Vec<u32> = images.elements.iter().map(|e| e.area.x).collect();
    assert!(
        xs.windows(2).all(|pair| pair[0] + images.width <= pair[1]),
        "{xs:?}"
    );
}

fn refused_with(edit: impl Fn(&mut Value), problem: &str) {
    let root = copy_catalog();
    let path = root
        .path()
        .join("catalog/screens/xdj700-v1.15/perform.json");
    let mut screen = read_json(&path);
    edit(&mut screen["slots"]["beat_loop.pad"]["images"]);
    write_bytes(&path, &serde_json::to_vec(&screen).expect("json"));
    let message = format!("{:#}", load_catalog(root.path()).expect_err("refused"));
    assert!(message.contains(problem), "{problem}: {message}");
}

#[test]
fn a_box_off_the_screen_or_an_image_past_the_application_is_refused() {
    // The screen is 800x480: the last pad moved to x 721 ends at 801.
    refused_with(
        |images| images["elements"][5]["box"]["x"] = 721.into(),
        "slot beat_loop.pad: element 5's box is not on the 800x480 screen",
    );
    // The application is 18,601,864 bytes: an image at its last 8480 bytes ends at the end; two
    // bytes later it ends past it.
    let last = 18_601_864 - 8480;
    let images_at = |offset: u32| {
        move |images: &mut Value| {
            images["elements"][5]["images"]["normal"] = format!("{offset:#x}").into()
        }
    };
    let root = copy_catalog();
    let path = root
        .path()
        .join("catalog/screens/xdj700-v1.15/perform.json");
    let mut screen = read_json(&path);
    images_at(last)(&mut screen["slots"]["beat_loop.pad"]["images"]);
    write_bytes(&path, &serde_json::to_vec(&screen).expect("json"));
    load_catalog(root.path()).expect("an image ending at the application's end loads");
    refused_with(
        images_at(last + 2),
        "slot beat_loop.pad's image at 0x11bb66a ends past the 18601864-byte application",
    );
}
