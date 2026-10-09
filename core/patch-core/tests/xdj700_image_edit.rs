//! Image edits in the schema-v2 engine, on the synthetic release: the edited pixels, the bounded
//! diff, the whole-image precondition, the flat-image rule, protected ranges and the protected
//! set, and the draft hashes. Synthetic images only.

mod common;

use common::recipe::{Fixture, PADDING, replacement, stock_application};
use patch_core::sha256_hex;
use patch_core::xdj700::{
    ProtectedSet, RecipeChecks, RecipeError, VERSION_STRING_OFFSET, apply_recipe_v2_to,
    precondition_hashes,
};
use patch_schema::{Glyph, ImageEdit, Pixel, PixelBox};

/// A 17x8 image of the synthetic application's byte pattern, at 0xd00 (17 pixels wide, its rows do
/// not line the pattern up, so its neighbours do not predict it).
const IMAGE: usize = 0xd00;
const WIDTH: u32 = 17;
const HEIGHT: u32 = 8;

fn pixel(bytes: &[u8], x: u32, y: u32) -> u16 {
    let i = IMAGE + ((y * WIDTH + x) * 2) as usize;
    u16::from_le_bytes([bytes[i], bytes[i + 1]])
}

/// Erases x 4..10, y 2..5 and draws a 3x2 glyph at (5, 3) in the colour of pixel (1, 1).
fn edit(offset: usize) -> ImageEdit {
    let stock = stock_application();
    let len = (WIDTH * HEIGHT * 2) as usize;
    ImageEdit {
        offset: offset as u64,
        width: WIDTH,
        height: HEIGHT,
        // An image placed past the end on purpose gets a placeholder hash.
        sha256: stock
            .get(offset..offset + len)
            .map_or_else(|| "00".repeat(32), sha256_hex),
        erase: Some(PixelBox {
            x: 4,
            y: 2,
            width: 6,
            height: 3,
        }),
        glyph: Glyph {
            at: PixelBox {
                x: 5,
                y: 3,
                width: 3,
                height: 2,
            },
            alpha_hex: "f8008f".to_owned(),
            colour_from: Pixel { x: 1, y: 1 },
        },
        purpose: "test".to_owned(),
    }
}

/// The documented arithmetic, restated per channel: linear interpolation between the pixels
/// beside the box, then coverage blending in 15ths.
fn expected_pixel(stock: &[u8], x: u32, y: u32) -> u16 {
    let channels = |p: u16| {
        [
            u32::from(p >> 11),
            u32::from((p >> 5) & 0x3f),
            u32::from(p & 0x1f),
        ]
    };
    let pack = |c: [u32; 3]| ((c[0] << 11) | (c[1] << 5) | c[2]) as u16;
    let mut value = pixel(stock, x, y);
    if (4..10).contains(&x) && (2..5).contains(&y) {
        let (l, r) = (channels(pixel(stock, 3, y)), channels(pixel(stock, 10, y)));
        let i = x - 4;
        value = pack([0, 1, 2].map(|c| (l[c] * (6 - i) + r[c] * (i + 1) + 7 / 2) / 7));
    }
    if (5..8).contains(&x) && (3..5).contains(&y) {
        let alpha = [15, 8, 0, 0, 8, 15][((y - 3) * 3 + (x - 5)) as usize];
        let (b, c) = (channels(value), channels(pixel(stock, 1, 1)));
        value = pack([0, 1, 2].map(|k| (b[k] * (15 - alpha) + c[k] * alpha + 7) / 15));
    }
    value
}

fn with_edits(fixture: &Fixture, edits: Vec<ImageEdit>) -> patch_schema::RecipeV2 {
    let mut recipe = fixture.recipe(Vec::new());
    recipe.image_edits = edits;
    recipe
}

#[test]
fn edits_exactly_the_documented_pixels_and_nothing_else() {
    let fixture = Fixture::new();
    let stock = stock_application();

    let output = fixture
        .apply(&with_edits(&fixture, vec![edit(IMAGE)]))
        .expect("apply");

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            assert_eq!(
                pixel(&output, x, y),
                expected_pixel(&stock, x, y),
                "pixel ({x}, {y})"
            );
        }
    }
    // Outside the image, only the version string changed.
    let image = IMAGE..IMAGE + (WIDTH * HEIGHT * 2) as usize;
    assert_eq!(image.end, 0xe10);
    let version = VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + 4;
    for (i, (a, b)) in stock.iter().zip(&output).enumerate() {
        if !image.contains(&i) && !version.contains(&i) {
            assert_eq!(a, b, "byte {i:#x}");
        }
    }
    // The glyph's full-coverage pixels carry the sampled colour.
    assert_eq!(pixel(&output, 5, 3), pixel(&stock, 1, 1));
}

#[test]
fn applies_the_same_bytes_every_time_and_alongside_replacements() {
    let fixture = Fixture::new();
    let mut recipe = with_edits(&fixture, vec![edit(IMAGE)]);
    recipe.replacements = vec![replacement(0x900, &[0xde, 0xad])];

    let first = fixture.apply(&recipe).expect("first");
    let again = fixture.apply(&recipe).expect("again");

    assert_eq!(first, again);
    assert_eq!(&first[0x900..0x902], &[0xde, 0xad]);
    assert_eq!(pixel(&first, 5, 3), pixel(&stock_application(), 1, 1));
}

#[test]
fn refuses_a_stock_image_that_does_not_match_its_hash() {
    let fixture = Fixture::new();
    let mut wrong = edit(IMAGE);
    wrong.sha256 = "00".repeat(32);

    let result = fixture.apply(&with_edits(&fixture, vec![wrong]));

    assert_eq!(
        result,
        Err(RecipeError::ImagePrecondition {
            index: 0,
            expected: "00".repeat(32)
        })
    );
    // The actual hash is not reported.
    let message = result.unwrap_err().to_string();
    assert!(!message.contains(&sha256_hex(&stock_application()[IMAGE..IMAGE + 256])));
}

#[test]
fn refuses_a_flat_image_whose_hash_could_be_inverted() {
    let fixture = Fixture::new();
    // 8x4 pixels of zero padding.
    let mut flat = edit(PADDING.start);
    flat.width = 8;
    flat.height = 4;
    flat.erase = Some(PixelBox {
        x: 1,
        y: 1,
        width: 2,
        height: 2,
    });
    flat.glyph.at = PixelBox {
        x: 1,
        y: 1,
        width: 2,
        height: 1,
    };
    flat.glyph.alpha_hex = "ff".to_owned();
    flat.sha256 = sha256_hex(&stock_application()[PADDING]);
    let recipe = with_edits(&fixture, vec![flat]);

    assert_eq!(
        fixture.apply(&recipe),
        Err(RecipeError::PredictableImage {
            index: 0,
            distinct: 1,
            unpredicted: 0
        })
    );
    // No hash is computed for it either.
    let hashes = precondition_hashes(
        &recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    );
    assert_eq!(
        hashes,
        Err(RecipeError::PredictableImage {
            index: 0,
            distinct: 1,
            unpredicted: 0
        })
    );
}

#[test]
fn refuses_an_image_past_the_end_or_in_the_protected_header() {
    let fixture = Fixture::new();
    let past = with_edits(&fixture, vec![edit(0xf80)]);
    assert_eq!(
        fixture.apply(&past),
        Err(RecipeError::ImageOutOfBounds {
            index: 0,
            end: 0x1090,
            len: 0x1000
        })
    );

    let mut header = edit(0x700);
    header.sha256 = "00".repeat(32);
    assert_eq!(
        fixture.apply(&with_edits(&fixture, vec![header])),
        Err(RecipeError::ImageProtected {
            index: 0,
            start: 0x700,
            end: 0x810,
            protected_start: 0,
            protected_end: 0x800
        })
    );
}

#[test]
fn refuses_an_image_in_the_protected_set() {
    let fixture = Fixture::new();
    let target = fixture.target();
    let set = ProtectedSet::parse("release synthetic\n08000d80 08000d81\n", &target)
        .expect("a synthetic set");
    let checks = RecipeChecks {
        protected_set: Some(&set),
    };
    let recipe = with_edits(&fixture, vec![edit(IMAGE)]);

    let result = apply_recipe_v2_to(&recipe, &target, b"never read", checks).map(|_| ());

    assert_eq!(
        result,
        Err(RecipeError::ImageProtectedSet {
            index: 0,
            start: 0x0800_0d00,
            last: 0x0800_0e0f,
            set_start: 0x0800_0d80,
            set_last: 0x0800_0d81
        })
    );
}

#[test]
fn draft_hashes_include_every_image_and_complete_the_recipe() {
    let fixture = Fixture::new();
    let complete = edit(IMAGE);
    let mut draft = complete.clone();
    draft.sha256 = "00".repeat(32);
    let mut recipe = with_edits(&fixture, vec![draft]);

    let hashes = precondition_hashes(
        &recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    )
    .expect("hashes");

    assert!(hashes.replacements.is_empty());
    assert_eq!(hashes.image_edits, vec![complete.sha256.clone()]);
    recipe.image_edits[0].sha256 = hashes.image_edits[0].clone();
    assert!(fixture.apply(&recipe).is_ok());
}

#[test]
fn refuses_an_image_its_neighbours_predict_despite_many_values() {
    let fixture = Fixture::new();
    // 16 pixels wide, the pattern repeats from row to row: 128 distinct values, but a smooth
    // structure whose hash could be inverted.
    let mut aligned = edit(IMAGE);
    aligned.width = 16;
    aligned.sha256 = sha256_hex(&stock_application()[IMAGE..IMAGE + 256]);

    assert_eq!(
        fixture.apply(&with_edits(&fixture, vec![aligned])),
        Err(RecipeError::PredictableImage {
            index: 0,
            distinct: 128,
            unpredicted: 24
        })
    );
}
