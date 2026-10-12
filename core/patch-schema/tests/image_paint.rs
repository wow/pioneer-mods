//! Painting the author's own art in an image edit (`paint`) and a glyph in the author's own colour
//! (`glyph.colour`): the JSON forms and the static checks. Needs no firmware.

use patch_schema::{ColourSource, ImageEdit, PaletteEntry, Pixel, Rgb565};
use serde_json::{Value, json};

fn edit() -> Value {
    json!({
        "offset": 4096, "width": 4, "height": 3,
        "paint": {
            "at": { "x": 1, "y": 0, "width": 3, "height": 2 },
            "palette": ["keep", { "colour": "0x1f00" }, { "from": { "x": 0, "y": 2 } }],
            "pixels_hex": "012210"
        },
        "glyph": {
            "at": { "x": 0, "y": 2, "width": 2, "height": 1 },
            "alpha_hex": "f8",
            "colour": "0xFFFF"
        },
        "purpose": "Our own pad."
    })
}

fn checked(value: Value) -> Result<ImageEdit, String> {
    let edit: ImageEdit = serde_json::from_value(value).map_err(|error| error.to_string())?;
    edit.validate(0).map_err(|error| error.to_string())?;
    Ok(edit)
}

#[test]
fn a_paint_names_its_palette_and_a_glyph_may_carry_the_authors_colour() {
    let edit = checked(edit()).expect("valid");
    let paint = edit.paint.as_ref().expect("paint");
    assert_eq!(
        paint.palette,
        [
            PaletteEntry::Keep,
            PaletteEntry::Colour(Rgb565(0x1f00)),
            PaletteEntry::From(Pixel { x: 0, y: 2 })
        ]
    );
    assert_eq!(
        paint.entries().expect("entries")[..3],
        [paint.palette[0], paint.palette[1], paint.palette[2]]
    );
    let glyph = edit.glyph.as_ref().expect("glyph");
    assert_eq!(glyph.colour, ColourSource::Colour(Rgb565(0xffff)));
    // The rows it may change: the paint box (rows 0 and 1) and the glyph box (row 2).
    let rows: Vec<(u32, std::ops::Range<u32>)> = edit.changed_rows();
    assert_eq!(rows, [(0, 1..4), (1, 1..4), (2, 0..2)]);
    // Written back, colours are lower-case, four digits.
    let written = serde_json::to_value(&edit).expect("json");
    assert_eq!(written["glyph"]["colour"], "0xffff");
    assert_eq!(written["paint"]["palette"][0], "keep");

    // A paint alone is an edit; so is a glyph alone (with a stock colour, as stage 7's).
    let mut paint_only = self::edit();
    paint_only.as_object_mut().expect("edit").remove("glyph");
    checked(paint_only).expect("valid");
    let mut glyph_only = self::edit();
    glyph_only.as_object_mut().expect("edit").remove("paint");
    glyph_only["glyph"] = json!({ "at": { "x": 0, "y": 2, "width": 2, "height": 1 },
                                  "alpha_hex": "f8", "colour_from": { "x": 3, "y": 0 } });
    checked(glyph_only).expect("valid");

    // Two edits that differ only in the digits' letter case are the same edit.
    // An 11-entry palette, so that digit a is used: written in either case, the same edit.
    let eleven = |digits: &str| {
        let mut value = self::edit();
        let mut palette = vec![json!("keep")];
        palette.extend((1..=10).map(|i| json!({ "colour": format!("0x{i:04x}") })));
        value["paint"]["palette"] = Value::Array(palette);
        value["paint"]["pixels_hex"] = digits.into();
        checked(value).expect("valid")
    };
    assert!(eleven("01a2a0").same_edit(&eleven("01A2A0")));
    assert!(!eleven("01a2a0").same_edit(&eleven("01a2a1")));
    let mut other = self::edit();
    other["paint"]["pixels_hex"] = "012211".into();
    assert!(!checked(other).expect("valid").same_edit(&edit));
    // The same pixels from a reordered palette with an unused entry: the same paint.
    let mut reordered = self::edit();
    reordered["paint"]["palette"] = json!([{ "from": { "x": 0, "y": 2 } }, { "colour": "0x0001" },
                                          "keep", { "colour": "0x1f00" }]);
    reordered["paint"]["pixels_hex"] = "230032".into();
    assert!(checked(reordered).expect("valid").same_edit(&edit));

    // Erasing alone is an edit: it removes a stock label.
    let mut erase_only = self::edit();
    let object = erase_only.as_object_mut().expect("edit");
    object.remove("paint");
    object.remove("glyph");
    object.insert(
        "erase".into(),
        json!({ "x": 1, "y": 0, "width": 2, "height": 3 }),
    );
    let erase_only = checked(erase_only).expect("valid");
    assert_eq!(erase_only.changed_rows(), [(0, 1..3), (1, 1..3), (2, 1..3)]);
}

#[test]
fn malformed_paints_and_glyph_colours_are_refused() {
    let refused = |change: &dyn Fn(&mut Value), problem: &str| {
        let mut value = edit();
        change(&mut value);
        let error = checked(value).expect_err("refused");
        assert!(error.contains(problem), "{problem}: {error}");
    };
    refused(
        &|e| {
            let object = e.as_object_mut().expect("edit");
            object.remove("paint");
            object.remove("glyph");
        },
        "image_edits[0] neither erases, paints nor draws a glyph",
    );
    refused(
        &|e| e["glyph"]["colour_from"] = json!({ "x": 0, "y": 0 }),
        "needs exactly one of colour and colour_from",
    );
    refused(
        &|e| {
            e["glyph"].as_object_mut().expect("glyph").remove("colour");
        },
        "needs exactly one of colour and colour_from",
    );
    refused(
        &|e| e["paint"]["palette"] = json!([]),
        "1 to 16 entries, not 0",
    );
    refused(
        &|e| e["paint"]["palette"] = Value::Array(vec!["keep".into(); 17]),
        "1 to 16 entries, not 17",
    );
    refused(
        &|e| e["paint"]["pixels_hex"] = "01221".into(),
        "(6), found 5",
    );
    refused(
        &|e| e["paint"]["pixels_hex"] = "01221g".into(),
        "holds 'g', which names no entry of the 3-entry palette",
    );
    refused(
        &|e| e["paint"]["pixels_hex"] = "012213".into(),
        "holds '3', which names no entry of the 3-entry palette",
    );
    refused(
        &|e| e["paint"]["pixels_hex"] = "000000".into(),
        "paint keeps every pixel",
    );
    refused(
        &|e| e["paint"]["at"]["x"] = 2.into(),
        "image_edits[0].paint.at does not lie inside the 4x3 image",
    );
    refused(
        &|e| e["paint"]["palette"][2] = json!({ "from": { "x": 4, "y": 0 } }),
        "image_edits[0].paint.palette[2].from (4, 0) does not lie inside the 4x3 image",
    );
    refused(
        &|e| e["paint"]["palette"][1] = json!({ "colour": "0x1f0" }),
        "is not an RGB565",
    );
    refused(
        &|e| e["paint"]["palette"][1] = json!({ "colour": "1f00" }),
        "is not an RGB565",
    );
    refused(
        &|e| e["paint"]["palette"][0] = "transparent".into(),
        "unknown variant",
    );
    refused(&|e| e["paint"]["extra"] = 1.into(), "unknown field");
}
