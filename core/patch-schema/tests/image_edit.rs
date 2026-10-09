//! Image edits in schema-v2 recipes: the pixel arithmetic every front end shares, the rows an edit
//! may change, and the static checks.

use patch_schema::{
    ImageEdit, ImageEditError, RecipeV2, RecipeV2Error, blend, check_windows_across,
};
use serde_json::{Value, json};

/// A valid recipe with one replacement (window 0x900..0x940) and one image edit: a 16x8 image at
/// 0xd00, erasing x 4..10, y 2..5 and drawing a 3x2 glyph at (5, 3).
fn recipe_json() -> Value {
    json!({
        "schema_version": 2,
        "recipe_id": "image-test",
        "description": "test",
        "target": {
            "release": "xdj700-v1.15",
            "upd_sha256": "00".repeat(32),
            "application_sha256": "11".repeat(32)
        },
        "label": "Ver1.16",
        "reported_version": "0.12",
        "replacements": [{
            "offset": 0x910,
            "bytes_hex": "dead",
            "precondition": {"before": 16, "after": 46, "sha256": "ab".repeat(32)},
            "purpose": "test"
        }],
        "image_edits": [image_json(0xd00)]
    })
}

fn image_json(offset: u64) -> Value {
    json!({
        "offset": offset,
        "width": 16,
        "height": 8,
        "sha256": "cd".repeat(32),
        "erase": {"x": 4, "y": 2, "width": 6, "height": 3},
        "glyph": {
            "at": {"x": 5, "y": 3, "width": 3, "height": 2},
            "alpha_hex": "f8008f",
            "colour_from": {"x": 1, "y": 1}
        },
        "purpose": "relabel a button"
    })
}

fn recipe() -> RecipeV2 {
    serde_json::from_value(recipe_json()).expect("valid JSON")
}

fn with_image(change: impl FnOnce(&mut Value)) -> RecipeV2 {
    let mut json = recipe_json();
    change(&mut json["image_edits"][0]);
    serde_json::from_value(json).expect("valid JSON")
}

fn image_error(recipe: &RecipeV2) -> Option<ImageEditError> {
    match recipe.validate() {
        Err(RecipeV2Error::ImageEdit(error)) => Some(error),
        _ => None,
    }
}

#[test]
fn a_recipe_with_an_image_edit_validates() {
    let recipe = recipe();

    assert_eq!(recipe.validate(), Ok(()));
    let edit = &recipe.image_edits[0];
    assert_eq!(edit.len(), Some(256));
    assert_eq!(edit.window(), Some(0xd00..0xe00));
    assert_eq!(edit.alpha(), Some(vec![15, 8, 0, 0, 8, 15]));
}

#[test]
fn image_edits_are_optional() {
    let mut json = recipe_json();
    json.as_object_mut().expect("object").remove("image_edits");

    let recipe: RecipeV2 = serde_json::from_value(json).expect("valid JSON");

    assert!(recipe.image_edits.is_empty());
    assert_eq!(recipe.validate(), Ok(()));
}

#[test]
fn unknown_fields_in_an_edit_are_refused() {
    let mut json = recipe_json();
    json["image_edits"][0]["colour"] = json!("ffff");

    assert!(serde_json::from_value::<RecipeV2>(json).is_err());
}

#[test]
fn blend_keeps_the_background_at_zero_and_paints_the_colour_at_fifteen() {
    assert_eq!(blend(0x1234, 0xffff, 0), 0x1234);
    assert_eq!(blend(0x1234, 0xffff, 15), 0xffff);
    assert_eq!(blend(0x0000, 0xffff, 15), 0xffff);
    // Halfway between black and white, per channel, rounded: red 31 * 8 / 15 = 16.5 -> 17.
    assert_eq!(blend(0x0000, 0xffff, 8), (17 << 11) | (34 << 5) | 17);
    // Alpha above 15 counts as 15.
    assert_eq!(blend(0x1234, 0xabcd, 200), 0xabcd);
}

#[test]
fn erasing_interpolates_between_the_pixels_beside_the_box() {
    // Black to white over three pixels: a quarter, a half and three quarters of the way.
    let row = ImageEdit::erase_row(0x0000, 0xffff, 3);
    let red = |pixel: u16| pixel >> 11;
    let green = |pixel: u16| (pixel >> 5) & 0x3f;
    assert_eq!(row.iter().map(|&p| red(p)).collect::<Vec<_>>(), [8, 16, 23]);
    assert_eq!(
        row.iter().map(|&p| green(p)).collect::<Vec<_>>(),
        [16, 32, 47]
    );
    // A flat background stays flat.
    assert_eq!(ImageEdit::erase_row(0x4a69, 0x4a69, 5), vec![0x4a69; 5]);
}

#[test]
fn the_changed_rows_are_the_union_of_both_boxes() {
    let edit = &recipe().image_edits[0];

    assert_eq!(
        edit.changed_rows(),
        vec![(2, 4..10), (3, 4..10), (4, 4..10)]
    );

    // A glyph wider and lower than the erase box.
    let wide = with_image(|image| {
        image["glyph"]["at"] = json!({"x": 2, "y": 4, "width": 12, "height": 2});
        image["glyph"]["alpha_hex"] = json!("1".repeat(24));
    });
    assert_eq!(
        wide.image_edits[0].changed_rows(),
        vec![(2, 4..10), (3, 4..10), (4, 2..14), (5, 2..14)]
    );

    // Without an erase box, only the glyph's rows.
    let no_erase = with_image(|image| {
        image.as_object_mut().expect("object").remove("erase");
    });
    assert_eq!(
        no_erase.image_edits[0].changed_rows(),
        vec![(3, 5..8), (4, 5..8)]
    );
}

#[test]
fn refuses_images_that_are_empty_too_large_or_overflow() {
    let empty = with_image(|image| image["width"] = json!(0));
    assert_eq!(
        image_error(&empty),
        Some(ImageEditError::EmptyImage { index: 0 })
    );

    let large = with_image(|image| {
        image["width"] = json!(1024);
        image["height"] = json!(513);
    });
    assert_eq!(
        image_error(&large),
        Some(ImageEditError::ImageTooLarge { index: 0 })
    );

    let overflow = with_image(|image| image["offset"] = json!(u64::MAX - 10));
    assert_eq!(
        image_error(&overflow),
        Some(ImageEditError::OffsetOverflow { index: 0 })
    );
}

#[test]
fn refuses_boxes_and_pixels_outside_the_image() {
    let outside = |field: &str| {
        Some(ImageEditError::OutsideImage {
            index: 0,
            field: match field {
                "erase" => "erase",
                "glyph.at" => "glyph.at",
                _ => "glyph.colour_from",
            },
            width: 16,
            height: 8,
            needs: if field == "erase" {
                ", with a column on each side (the erase interpolates between them)"
            } else {
                ""
            },
        })
    };
    // The erase needs a stock pixel on each side.
    for erase in [
        json!({"x": 0, "y": 2, "width": 6, "height": 3}),
        json!({"x": 10, "y": 2, "width": 6, "height": 3}),
        json!({"x": 4, "y": 6, "width": 6, "height": 3}),
        json!({"x": 4, "y": 2, "width": 0, "height": 3}),
    ] {
        let recipe = with_image(|image| image["erase"] = erase.clone());
        assert_eq!(image_error(&recipe), outside("erase"), "{erase}");
    }
    let glyph = with_image(|image| {
        image["glyph"]["at"] = json!({"x": 14, "y": 3, "width": 3, "height": 2});
    });
    assert_eq!(image_error(&glyph), outside("glyph.at"));
    let colour = with_image(|image| image["glyph"]["colour_from"] = json!({"x": 16, "y": 0}));
    assert_eq!(image_error(&colour), outside("colour_from"));
}

#[test]
fn refuses_a_mask_of_the_wrong_length_non_hex_or_painting_nothing() {
    let short = with_image(|image| image["glyph"]["alpha_hex"] = json!("f80"));
    assert_eq!(
        image_error(&short),
        Some(ImageEditError::AlphaLength {
            index: 0,
            expected: 6,
            found: 3
        })
    );
    let not_hex = with_image(|image| image["glyph"]["alpha_hex"] = json!("f80g8f"));
    assert_eq!(
        image_error(&not_hex),
        Some(ImageEditError::AlphaNotHex { index: 0 })
    );
    let blank = with_image(|image| image["glyph"]["alpha_hex"] = json!("000000"));
    assert_eq!(
        image_error(&blank),
        Some(ImageEditError::EmptyGlyph { index: 0 })
    );
}

#[test]
fn refuses_an_empty_purpose_and_a_malformed_hash() {
    let purpose = with_image(|image| image["purpose"] = json!("  "));
    assert_eq!(
        image_error(&purpose),
        Some(ImageEditError::EmptyPurpose { index: 0 })
    );
    let hash = with_image(|image| image["sha256"] = json!("cd"));
    assert_eq!(
        hash.validate(),
        Err(RecipeV2Error::InvalidSha256 {
            field: "image_edits[0].sha256".to_owned()
        })
    );
}

#[test]
fn refuses_unordered_or_overlapping_images() {
    let mut json = recipe_json();
    json["image_edits"] = json!([image_json(0xd00), image_json(0xdf0)]);
    let overlapping: RecipeV2 = serde_json::from_value(json.clone()).expect("JSON");
    assert_eq!(
        image_error(&overlapping),
        Some(ImageEditError::UnorderedOrOverlapping {
            index: 1,
            previous: 0
        })
    );

    json["image_edits"] = json!([image_json(0xe00), image_json(0xd00)]);
    let unordered: RecipeV2 = serde_json::from_value(json.clone()).expect("JSON");
    assert_eq!(
        image_error(&unordered),
        Some(ImageEditError::UnorderedOrOverlapping {
            index: 1,
            previous: 0
        })
    );

    json["image_edits"] = json!([image_json(0xd00), image_json(0xe00)]);
    let adjacent: RecipeV2 = serde_json::from_value(json).expect("JSON");
    assert_eq!(adjacent.validate(), Ok(()));
}

#[test]
fn refuses_an_image_over_a_replacement_window() {
    // The replacement's window is 0x900..0x940.
    let recipe = with_image(|image| image["offset"] = json!(0x8c1));

    assert_eq!(
        image_error(&recipe),
        Some(ImageEditError::OverlapsReplacement {
            index: 0,
            replacement: 0
        })
    );
    let clear = with_image(|image| image["offset"] = json!(0x940));
    assert_eq!(clear.validate(), Ok(()));
}

#[test]
fn image_windows_count_across_recipes() {
    let first = recipe();
    let mut second_json = recipe_json();
    second_json["recipe_id"] = json!("other");
    second_json["replacements"] = json!([]);
    second_json["image_edits"] = json!([image_json(0xd80)]);
    let second: RecipeV2 = serde_json::from_value(second_json).expect("JSON");

    let error = check_windows_across([&first, &second]).expect_err("overlap");

    assert_eq!(error.first, "image-test image_edits[0]");
    assert_eq!(error.second, "other image_edits[0]");
    assert_eq!(error.second_window, 0xd80..0xe80);
}

#[test]
fn counts_mask_characters_not_bytes() {
    // Three characters, five bytes: refused for its length, reported in characters.
    let euro = with_image(|image| image["glyph"]["alpha_hex"] = json!("ff€"));

    assert_eq!(
        image_error(&euro),
        Some(ImageEditError::AlphaLength {
            index: 0,
            expected: 6,
            found: 3
        })
    );
}

#[test]
fn box_ranges_saturate_instead_of_overflowing() {
    let far = patch_schema::PixelBox {
        x: u32::MAX - 1,
        y: u32::MAX,
        width: 5,
        height: 5,
    };

    assert_eq!(far.columns(), u32::MAX - 1..u32::MAX);
    assert_eq!(far.rows(), u32::MAX..u32::MAX);
    // erase_row's arithmetic is 64-bit: the widest rows a validated edit can have stay exact.
    let row = ImageEdit::erase_row(0x0000, 0xffff, 1024);
    assert_eq!(row.len(), 1024);
    assert_eq!(row[1023] >> 11, (31 * 1024 + 512) / 1025);
}
