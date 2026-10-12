//! A screen slot's images (`slots.<name>.images`): where each element is drawn, from which image
//! per state. Needs no firmware: offsets and positions only.

use patch_schema::catalog::{CatalogEntry, Screen};
use serde_json::{Value, json};

fn element(x: u32, first: u32) -> Value {
    json!({ "at": { "x": x, "y": 154 },
            "images": { "normal": format!("{first:#x}"), "lit": format!("{:#x}", first + 8480) } })
}

fn screen(images: Value) -> Value {
    json!({
        "schema_version": 1, "id": "perform", "player": "xdj700-v1.15", "title": "PERFORM",
        "slots": { "beat_loop.pad": { "count": 2, "description": "Two pads.", "images": images } },
        "maintainer": "m"
    })
}

fn images() -> Value {
    json!({
        "width": 80, "height": 53, "pixels": "rgb565", "states": ["normal", "lit"],
        "elements": [element(116, 0x1000), element(201, 0x1000 + 2 * 8480)],
        "evidence": "Measured."
    })
}

fn checked(value: Value) -> Result<Screen, String> {
    let screen: Screen = serde_json::from_value(value).map_err(|error| error.to_string())?;
    screen.validate().map_err(|error| error.to_string())?;
    Ok(screen)
}

#[test]
fn a_slot_names_each_elements_position_and_images_in_state_order() {
    let loaded = checked(screen(images())).expect("valid");
    let slot_images = loaded.slots["beat_loop.pad"]
        .images
        .as_ref()
        .expect("images");
    assert_eq!(slot_images.image_len(), 8480);
    // Element by element; within one, in the order of `states` (normal before lit), whatever
    // order the file gives them in.
    let listed: Vec<(usize, &str, u32)> = slot_images
        .images()
        .map(|image| (image.element, image.state, image.offset))
        .collect();
    assert_eq!(
        listed,
        [
            (0, "normal", 0x1000),
            (0, "lit", 0x1000 + 8480),
            (1, "normal", 0x1000 + 2 * 8480),
            (1, "lit", 0x1000 + 3 * 8480)
        ]
    );
    let area = slot_images.area(&slot_images.elements[1]);
    assert_eq!((area.x, area.y, area.w, area.h), (201, 154, 80, 53));

    // Without images, a slot is as before.
    let mut plain = screen(images());
    plain["slots"]["beat_loop.pad"]
        .as_object_mut()
        .expect("slot")
        .remove("images");
    assert!(
        checked(plain).expect("valid").slots["beat_loop.pad"]
            .images
            .is_none()
    );
}

#[test]
fn malformed_slot_images_are_refused() {
    let refused = |edit: &dyn Fn(&mut Value), problem: &str| {
        let mut images = images();
        edit(&mut images);
        let error = checked(screen(images)).expect_err("refused");
        assert!(error.contains(problem), "{problem}: {error}");
    };
    refused(
        &|i| i["elements"].as_array_mut().expect("list").truncate(1),
        "one element per slot element",
    );
    refused(&|i| i["states"] = json!(["normal"]), "one image per state");
    refused(
        &|i| i["states"] = json!(["normal", "lit", "lit"]),
        "none twice",
    );
    refused(&|i| i["states"] = json!([]), "at least one state");
    refused(&|i| i["states"] = json!(["Normal", "lit"]), "Normal");
    refused(
        &|i| i["pixels"] = "rgb888".into(),
        "unknown variant `rgb888`",
    );
    refused(
        &|i| i["width"] = 0.into(),
        "images.width: an image is not empty",
    );
    refused(
        &|i| i["height"] = 0.into(),
        "images.height: an image is not empty",
    );
    refused(
        &|i| i["height"] = 8000.into(),
        "width x height is 1048576 bytes at most",
    );
    refused(
        &|i| i["elements"][0]["images"]["lit"] = "0x1001".into(),
        "elements[0].images: an image is 2-byte aligned",
    );
    // The second element's first image starts one byte pair inside the first element's last.
    refused(
        &|i| i["elements"][1] = element(201, 0x1000 + 2 * 8480 - 2),
        "no two images overlap",
    );
    // Two elements drawn over each other: the second starts one pixel inside the first.
    refused(
        &|i| i["elements"][1]["at"]["x"] = 195.into(),
        "elements[0] and elements[1] overlap on screen",
    );
    refused(
        &|i| i["elements"][1]["box"] = json!({}),
        "unknown field `box`",
    );
    refused(&|i| i["evidence"] = "".into(), "evidence");
    refused(&|i| i["extra"] = 1.into(), "unknown field");

    // Images and elements that only touch are fine.
    let mut touching = images();
    touching["elements"][1] = element(196, 0x1000 + 2 * 8480);
    checked(screen(touching)).expect("valid");
}
