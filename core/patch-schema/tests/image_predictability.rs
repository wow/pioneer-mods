//! The leak rule for image edits: how predictable a stock image is (distinct values and pixels
//! the median edge predictor misses), and its exact thresholds. Synthetic images only.

use patch_schema::{
    MIN_IMAGE_DISTINCT_PIXELS, MIN_IMAGE_UNPREDICTED_PIXELS, predictability, unpredicted_pixels,
};

#[test]
fn fills_and_gradients_are_predicted_and_detail_is_not() {
    let rgb = |r: u16, g: u16, b: u16| (r << 11) | (g << 5) | b;
    let (w, h) = (20u16, 10u16);
    let image = |f: &dyn Fn(u16, u16) -> u16| -> Vec<u16> {
        (0..h)
            .flat_map(|y| (0..w).map(move |x| (x, y)))
            .map(|(x, y)| f(x, y))
            .collect()
    };

    assert_eq!(unpredicted_pixels(&image(&|_, _| rgb(31, 40, 0)), 20), 0);
    assert_eq!(
        unpredicted_pixels(&image(&|x, _| rgb(x, 2 * x, 31 - x)), 20),
        0
    );
    assert_eq!(unpredicted_pixels(&image(&|_, y| rgb(3 * y, y, 0)), 20), 0);
    assert_eq!(
        unpredicted_pixels(&image(&|x, y| rgb(x + y, x + y, 0)), 20),
        0
    );
    // A checkerboard of black and white: every counted pixel is missed.
    let checker = image(&|x, y| if (x + y) % 2 == 0 { 0 } else { 0xffff });
    assert_eq!(unpredicted_pixels(&checker, 20), 19 * 9);
    // One white dot on black is missed where it is, to its right and below it.
    let mut dot = image(&|_, _| 0);
    dot[5 * 20 + 7] = 0xffff;
    assert_eq!(unpredicted_pixels(&dot, 20), 3);
    assert_eq!(unpredicted_pixels(&[], 0), 0);
}

/// A 30x30 black image with `interior` isolated dots (each missed three times: itself, to its right
/// and below it) and, if `corner`, one in the bottom-right corner (missed once), coloured from a
/// palette of `colours` values.
fn dots(interior: usize, corner: bool, colours: u16) -> Vec<u16> {
    let width = 30;
    let mut image = vec![0u16; width * width];
    let colour = |i: usize| 0x8000 | ((i as u16 % colours) * 0x0841);
    let positions = (0..3).flat_map(|row| (0..7).map(move |col| (2 + 3 * col, 2 + 3 * row)));
    for (i, (x, y)) in positions.take(interior).enumerate() {
        image[y * width + x] = colour(i);
    }
    if corner {
        image[width * width - 1] = colour(interior);
    }
    image
}

#[test]
fn the_unpredicted_threshold_is_inclusive() {
    let at = predictability(&dots(21, true, 64), 30);
    let below = predictability(&dots(21, false, 64), 30);

    assert_eq!(at.unpredicted, MIN_IMAGE_UNPREDICTED_PIXELS);
    assert!(at.distinct >= MIN_IMAGE_DISTINCT_PIXELS);
    assert!(at.passes());
    assert_eq!(below.unpredicted, MIN_IMAGE_UNPREDICTED_PIXELS - 1);
    assert!(!below.passes());
}

#[test]
fn the_distinct_threshold_is_inclusive() {
    // Black plus 15 dot colours, then black plus 14.
    let at = predictability(&dots(21, true, 15), 30);
    let below = predictability(&dots(21, true, 14), 30);

    assert_eq!(at.distinct, MIN_IMAGE_DISTINCT_PIXELS);
    assert_eq!(at.unpredicted, MIN_IMAGE_UNPREDICTED_PIXELS);
    assert!(at.passes());
    assert_eq!(below.distinct, MIN_IMAGE_DISTINCT_PIXELS - 1);
    assert!(!below.passes());
}
