//! Painting the author's own art in the schema-v2 engine ([`patch_schema::Paint`]), on the
//! synthetic release: each palette entry (kept, the author's colour, a stock pixel's colour read
//! before anything changes), the order erase, paint, glyph, and an edit that only paints.
//! Synthetic images only.

mod common;

use common::recipe::{Fixture, stock_application};
use patch_core::xdj700::{RecipeChecks, VERSION_STRING_OFFSET, precondition_hashes};
use patch_schema::{
    ExpectedV2, Glyph, ImageEdit, Paint, PaletteEntry, Pixel, PixelBox, RecipeV2, Rgb565, blend,
};

/// A 17x8 image of the synthetic application's byte pattern, at 0xd00.
const IMAGE: usize = 0xd00;
const WIDTH: u32 = 17;
const HEIGHT: u32 = 8;

fn pixel(bytes: &[u8], x: u32, y: u32) -> u16 {
    let i = IMAGE + ((y * WIDTH + x) * 2) as usize;
    u16::from_le_bytes([bytes[i], bytes[i + 1]])
}

/// A 5x3 box at (2, 1): keep, the author's 0x1234, the stock (0, 0), and the stock (3, 2), which
/// lies inside the box and is painted over.
fn paint() -> Paint {
    Paint {
        at: PixelBox {
            x: 2,
            y: 1,
            width: 5,
            height: 3,
        },
        palette: vec![
            PaletteEntry::Keep,
            PaletteEntry::Colour(Rgb565(0x1234)),
            PaletteEntry::From(Pixel { x: 0, y: 0 }),
            PaletteEntry::From(Pixel { x: 3, y: 2 }),
        ],
        pixels_hex: "01230123012301230123".chars().take(15).collect(),
    }
}

fn edit(paint: Option<Paint>, glyph: Option<Glyph>) -> ImageEdit {
    ImageEdit {
        offset: IMAGE as u64,
        width: WIDTH,
        height: HEIGHT,
        erase: None,
        paint,
        glyph,
        purpose: "test".to_owned(),
    }
}

/// `edits` in a recipe with both output identities pinned, as its author completes it.
fn pinned(fixture: &Fixture, edits: Vec<ImageEdit>) -> RecipeV2 {
    let mut recipe = fixture.recipe(Vec::new());
    recipe.image_edits = edits;
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some("00".repeat(32)),
        upd_sha256: None,
    });
    let output = precondition_hashes(
        &recipe,
        &fixture.target(),
        &fixture.update,
        RecipeChecks::NONE,
    )
    .expect("draft")
    .output;
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some(output.application_sha256),
        upd_sha256: Some(output.upd_sha256),
    });
    recipe
}

#[test]
fn paints_each_palette_entry_and_nothing_else() {
    let fixture = Fixture::new();
    let stock = stock_application();
    let output = fixture
        .apply(&pinned(&fixture, vec![edit(Some(paint()), None)]))
        .expect("apply");

    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let inside = (2..7).contains(&x) && (1..4).contains(&y);
            let expected = if inside {
                match (((y - 1) * 5 + (x - 2)) % 4) as u8 {
                    0 => pixel(&stock, x, y),
                    1 => 0x1234,
                    2 => pixel(&stock, 0, 0),
                    _ => pixel(&stock, 3, 2),
                }
            } else {
                pixel(&stock, x, y)
            };
            assert_eq!(pixel(&output, x, y), expected, "pixel ({x}, {y})");
        }
    }
    // (3, 2) is painted over (its digit is 2, the stock (0, 0)), yet entry 3 paints its stock
    // colour: palette colours are read before anything changes.
    assert_eq!(pixel(&output, 3, 2), pixel(&stock, 0, 0));
    assert_ne!(pixel(&stock, 3, 2), pixel(&stock, 0, 0));
    assert_eq!(pixel(&output, 5, 1), pixel(&stock, 3, 2));
    // Outside the image, only the version string changed.
    let image = IMAGE..IMAGE + (WIDTH * HEIGHT * 2) as usize;
    let version = VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + 4;
    for (i, (a, b)) in stock.iter().zip(&output).enumerate() {
        if !image.contains(&i) && !version.contains(&i) {
            assert_eq!(a, b, "byte {i:#x}");
        }
    }
}

#[test]
fn erases_then_paints_then_draws_the_glyph_in_the_authors_colour() {
    let fixture = Fixture::new();
    let stock = stock_application();
    // The erase box (4..10, 2..5) starts inside the paint box (2..7, 1..4); the glyph (two pixels
    // at (3, 1) and (4, 1)) lies on painted pixels.
    let mut erasing = edit(
        Some(paint()),
        Some(Glyph {
            at: PixelBox {
                x: 3,
                y: 1,
                width: 2,
                height: 1,
            },
            alpha_hex: "f8".to_owned(),
            colour_from: None,
            colour: Some(Rgb565(0xffff)),
        }),
    );
    erasing.erase = Some(PixelBox {
        x: 4,
        y: 2,
        width: 6,
        height: 3,
    });
    let output = fixture
        .apply(&pinned(&fixture, vec![erasing]))
        .expect("apply");

    // Digits run 0, 1, 2, 3 repeating, row by row over the 5x3 box: pixel (x, y) has index
    // (y - 1) * 5 + (x - 2). (4, 2) is erased, then painted: index 7, digit 3, the stock (3, 2).
    assert_eq!(pixel(&output, 4, 2), pixel(&stock, 3, 2));
    // (6, 2) is erased, then painted: its digit (index 9) is 1, the author's colour.
    assert_eq!(pixel(&output, 6, 2), 0x1234);
    // (5, 2) has digit 0 (index 8): kept, so it shows the erase's interpolation, not stock.
    let erased = ImageEdit::erase_row(pixel(&stock, 3, 2), pixel(&stock, 10, 2), 6);
    assert_eq!(pixel(&output, 5, 2), erased[1]);
    // (8, 3), outside the paint box, is erased only.
    let erased = ImageEdit::erase_row(pixel(&stock, 3, 3), pixel(&stock, 10, 3), 6);
    assert_eq!(pixel(&output, 8, 3), erased[4]);
    // The glyph: full coverage paints the author's colour, half blends over the painted pixel.
    assert_eq!(pixel(&output, 3, 1), 0xffff);
    assert_eq!(pixel(&output, 4, 1), blend(pixel(&stock, 0, 0), 0xffff, 8));
}
