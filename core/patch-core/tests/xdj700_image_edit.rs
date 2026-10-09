//! Image edits in the schema-v2 engine, on the synthetic release: the edited pixels, the bounded
//! diff, the output pin that replaces a per-image hash, protected ranges and the protected set, and
//! the draft's output identities. Synthetic images only.

mod common;

use common::recipe::{Fixture, PADDING, replacement, stock_application};
use patch_core::xdj700::{
    OutputIdentities, ProtectedSet, RecipeChecks, RecipeError, VERSION_STRING_OFFSET,
    apply_recipe_v2_to, precondition_hashes,
};
use patch_schema::{ExpectedV2, Glyph, ImageEdit, Pixel, PixelBox, RecipeV2};

/// A 17x8 image of the synthetic application's byte pattern, at 0xd00.
const IMAGE: usize = 0xd00;
const WIDTH: u32 = 17;
const HEIGHT: u32 = 8;

fn pixel(bytes: &[u8], x: u32, y: u32) -> u16 {
    let i = IMAGE + ((y * WIDTH + x) * 2) as usize;
    u16::from_le_bytes([bytes[i], bytes[i + 1]])
}

/// Erases x 4..10, y 2..5 and draws a 3x2 glyph at (5, 3) in the colour of pixel (1, 1).
fn edit(offset: usize) -> ImageEdit {
    ImageEdit {
        offset: offset as u64,
        width: WIDTH,
        height: HEIGHT,
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

/// A recipe with `edits` and a placeholder output pin, as in a draft.
fn with_edits(fixture: &Fixture, edits: Vec<ImageEdit>) -> RecipeV2 {
    let mut recipe = fixture.recipe(Vec::new());
    recipe.image_edits = edits;
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some("00".repeat(32)),
        upd_sha256: None,
    });
    recipe
}

/// The output identities of `recipe` as a draft, from the engine's draft path.
fn draft_output(fixture: &Fixture, recipe: &RecipeV2) -> OutputIdentities {
    precondition_hashes(
        recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    )
    .expect("draft")
    .output
}

/// `recipe` with both output identities pinned, as its author completes it.
fn pinned(fixture: &Fixture, mut recipe: RecipeV2) -> RecipeV2 {
    let output = draft_output(fixture, &recipe);
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some(output.application_sha256),
        upd_sha256: Some(output.upd_sha256),
    });
    recipe
}

#[test]
fn edits_exactly_the_documented_pixels_and_nothing_else() {
    let fixture = Fixture::new();
    let stock = stock_application();

    let recipe = pinned(&fixture, with_edits(&fixture, vec![edit(IMAGE)]));

    let output = fixture.apply(&recipe).expect("apply");

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
    let recipe = pinned(&fixture, recipe);

    let first = fixture.apply(&recipe).expect("first");
    let again = fixture.apply(&recipe).expect("again");

    assert_eq!(first, again);
    assert_eq!(&first[0x900..0x902], &[0xde, 0xad]);
    assert_eq!(pixel(&first, 5, 3), pixel(&stock_application(), 1, 1));
}

#[test]
fn refuses_an_output_that_does_not_match_its_pin() {
    let fixture = Fixture::new();
    let recipe = with_edits(&fixture, vec![edit(IMAGE)]);
    let output = draft_output(&fixture, &recipe);

    assert_eq!(
        fixture.apply(&recipe),
        Err(RecipeError::UnexpectedOutput {
            field: "application_sha256",
            expected: "00".repeat(32),
            actual: output.application_sha256
        })
    );
}

/// What a per-image hash used to catch: an offset changed after the recipe was completed.
#[test]
fn the_output_pin_catches_a_moved_image() {
    let fixture = Fixture::new();
    let mut recipe = pinned(&fixture, with_edits(&fixture, vec![edit(IMAGE)]));
    recipe.image_edits[0].offset += 2;

    let result = fixture.apply(&recipe);

    assert!(
        matches!(
            result,
            Err(RecipeError::UnexpectedOutput {
                field: "application_sha256",
                ..
            })
        ),
        "{result:?}"
    );
}

/// Without a published hash there is nothing to invert, so a mostly flat image is edited like any
/// other: four rows of zero padding over one row of the pattern, whose first pixel gives the glyph
/// colour.
#[test]
fn edits_a_flat_image() {
    let fixture = Fixture::new();
    let stock = stock_application();
    let base = PADDING.start;
    let at = |bytes: &[u8], x: usize, y: usize| {
        let i = base + (y * 8 + x) * 2;
        u16::from_le_bytes([bytes[i], bytes[i + 1]])
    };
    let mut flat = edit(base);
    flat.width = 8;
    flat.height = 5;
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
    flat.glyph.colour_from = Pixel { x: 0, y: 4 };
    let recipe = pinned(&fixture, with_edits(&fixture, vec![flat]));

    let output = fixture.apply(&recipe).expect("apply");

    let colour = at(&stock, 0, 4);
    assert_ne!(colour, 0);
    for x in [1, 2] {
        assert_eq!(at(&stock, x, 1), 0);
        assert_eq!(at(&output, x, 1), colour, "glyph pixel ({x}, 1)");
    }
    // The erase interpolates between zeros, so only the glyph's 4 bytes and the version string
    // change.
    let glyph = base + 18..base + 22;
    let version = VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + 4;
    for (i, (a, b)) in stock.iter().zip(&output).enumerate() {
        if !glyph.contains(&i) && !version.contains(&i) {
            assert_eq!(a, b, "byte {i:#x}");
        }
    }
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

    assert_eq!(
        fixture.apply(&with_edits(&fixture, vec![edit(0x700)])),
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

/// A draft whose window hashes and output pin are placeholders: the draft path computes them, and
/// the completed recipe applies.
#[test]
fn draft_output_identities_complete_the_recipe() {
    let fixture = Fixture::new();
    let mut recipe = with_edits(&fixture, vec![edit(IMAGE)]);
    recipe.replacements = vec![replacement(0x900, &[0xde, 0xad])];
    recipe.replacements[0].precondition.sha256 = "00".repeat(32);

    let hashes = precondition_hashes(
        &recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    )
    .expect("hashes");

    let output = hashes.output;
    recipe.replacements[0].precondition.sha256 = hashes.replacements[0].clone();
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some(output.application_sha256.clone()),
        upd_sha256: Some(output.upd_sha256.clone()),
    });
    let rebuilt = apply_recipe_v2_to(
        &recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    )
    .expect("apply");
    assert_eq!(rebuilt.application_sha256(), output.application_sha256);
    assert_eq!(rebuilt.sha256(), output.upd_sha256);
}

/// As for a relabelled button: the colour pixel lies inside the erase box (a stroke of the old
/// label), so it must be read before the erase removes it.
#[test]
fn reads_the_glyph_colour_before_the_erase() {
    let fixture = Fixture::new();
    let stock = stock_application();
    let mut inside = edit(IMAGE);
    inside.glyph.colour_from = Pixel { x: 6, y: 3 };
    let recipe = pinned(&fixture, with_edits(&fixture, vec![inside]));

    let output = fixture.apply(&recipe).expect("apply");

    // The erase changes that pixel, so reading it afterwards would give another colour.
    let erased_row = ImageEdit::erase_row(pixel(&stock, 3, 3), pixel(&stock, 10, 3), 6);
    assert_ne!(erased_row[2], pixel(&stock, 6, 3));
    // Full-coverage glyph pixels carry the stock colour.
    assert_eq!(pixel(&output, 5, 3), pixel(&stock, 6, 3));
    assert_eq!(pixel(&output, 7, 4), pixel(&stock, 6, 3));
}
