//! Kept bytes (`--` in `bytes_hex`) on the synthetic release: the stock byte stays, it is not
//! published, so the leak rule counts only the bytes a span writes.

mod common;

use common::recipe::{Fixture, replacement, stock_application};
use patch_core::xdj700::RecipeError;
use patch_schema::Replacement;

/// A replacement of `bytes_hex` at `offset`, with the helper's window for a span of that length.
fn kept(offset: usize, bytes_hex: &str) -> Replacement {
    let span = vec![0; bytes_hex.len() / 2];
    Replacement {
        bytes_hex: bytes_hex.to_owned(),
        ..replacement(offset, &span)
    }
}

/// `bytes_hex` for the stock bytes at `range`, written explicitly.
fn stock_hex(range: std::ops::Range<usize>) -> String {
    stock_application()[range]
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[test]
fn keeps_the_stock_byte_and_writes_the_others() {
    let fixture = Fixture::new();
    let stock = stock_application();
    // Three table entries four bytes apart: only their first bytes change.
    let recipe = fixture.recipe(vec![kept(0x900, "aa------bb------cc")]);

    let output = fixture.apply(&recipe).expect("apply");

    assert_eq!(
        [output[0x900], output[0x904], output[0x908]],
        [0xaa, 0xbb, 0xcc]
    );
    for at in (0x901..0x908).filter(|at| at % 4 != 0) {
        assert_eq!(output[at], stock[at], "{at:#x}");
    }
}

#[test]
fn counts_only_written_bytes_against_the_leak_rule() {
    let fixture = Fixture::new();
    // 20 stock bytes written explicitly would publish them...
    let explicit = format!("aa{}bb", stock_hex(0x901..0x915));
    assert_eq!(
        fixture.apply(&fixture.recipe(vec![kept(0x900, &explicit)])),
        Err(RecipeError::UnchangedSpanBytes {
            index: 0,
            unchanged: 20,
            longest_run: 20,
            len: 22
        })
    );
    // ...kept, they are not published.
    let masked = format!("aa{}bb", "--".repeat(20));
    assert!(
        fixture
            .apply(&fixture.recipe(vec![kept(0x900, &masked)]))
            .is_ok()
    );
}

#[test]
fn a_written_stock_byte_still_counts_and_edges_must_change() {
    let fixture = Fixture::new();
    // Three of five written bytes equal stock: refused. Keeping one instead leaves two of four.
    let three = format!("aa{}bb", stock_hex(0x901..0x904));
    assert_eq!(
        fixture.apply(&fixture.recipe(vec![kept(0x900, &three)])),
        Err(RecipeError::UnchangedSpanBytes {
            index: 0,
            unchanged: 3,
            longest_run: 3,
            len: 5
        })
    );
    let two = format!(
        "aa{}--{}bb",
        stock_hex(0x901..0x902),
        stock_hex(0x903..0x904)
    );
    assert!(
        fixture
            .apply(&fixture.recipe(vec![kept(0x900, &two)]))
            .is_ok()
    );

    // A written edge equal to stock is refused, with kept bytes inside.
    let edge = format!("{}--bb", stock_hex(0x900..0x901));
    assert_eq!(
        fixture.apply(&fixture.recipe(vec![kept(0x900, &edge)])),
        Err(RecipeError::UnchangedSpanEdge { index: 0 })
    );
}
