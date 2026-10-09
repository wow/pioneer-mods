//! The pixel arithmetic of image edits that every front end shares: blending, the erase's
//! interpolation, and box ranges.

use patch_schema::{ImageEdit, PixelBox, blend};

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
fn box_ranges_saturate_instead_of_overflowing() {
    let far = PixelBox {
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
