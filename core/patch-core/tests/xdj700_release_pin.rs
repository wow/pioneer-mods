//! Release pinning: a rebuild starts only from the pinned official file, so chained rebuilds
//! cannot reset the size bound or carry a non-official loader forward.

mod common;

use common::xdj700::{application, padded_application, rebuild, release, stock_update};
use patch_core::xdj700::{
    MAX_MAIN_GROWTH, OFFICIAL_V115, rebuild_with_application, rebuild_with_stock_application,
};
use patch_core::{RebuildError, sha256_hex};

#[test]
fn rebuild_refuses_an_input_other_than_the_pinned_release() {
    let stock = stock_update(&padded_application(7));
    let other = stock_update(&application(300, 9));
    let other_sha256 = sha256_hex(&other);

    assert_eq!(
        rebuild_with_application(
            &stock,
            &release(other.len(), &other_sha256),
            &application(300, 9),
            "Ver1.22"
        ),
        Err(RebuildError::UnpinnedInput {
            sha256: sha256_hex(&stock)
        })
    );
}

#[test]
fn rebuild_refuses_its_own_output_as_the_next_input() {
    // Chaining rebuilds must not reset the size bound or carry a non-official loader forward.
    let stock = stock_update(&padded_application(7));
    let sha256 = sha256_hex(&stock);
    let first = rebuild(&stock, &application(3000, 11), "Ver1.15").expect("first rebuild");

    let second = rebuild_with_application(
        first.bytes(),
        &release(stock.len(), &sha256),
        &application(3000, 13),
        "Ver1.15",
    );

    assert_eq!(
        second,
        Err(RebuildError::UnpinnedInput {
            sha256: first.sha256().to_owned()
        })
    );
}

#[test]
fn official_v115_release_pins_the_official_file_and_bound() {
    assert_eq!(
        OFFICIAL_V115.upd_sha256,
        "73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c"
    );
    assert_eq!(OFFICIAL_V115.max_main_image_len, 7_251_904 + 256 * 1024);
    assert_eq!(MAX_MAIN_GROWTH, 256 * 1024);
}

#[test]
fn rebuild_refuses_the_right_hash_with_the_wrong_length() {
    let stock = stock_update(&padded_application(7));
    let sha256 = sha256_hex(&stock);

    assert_eq!(
        rebuild_with_application(
            &stock,
            &release(stock.len() + 1, &sha256),
            &application(300, 9),
            "Ver1.22"
        ),
        Err(RebuildError::UnpinnedInput { sha256 })
    );
}

#[test]
fn stock_rebuild_equals_a_rebuild_with_the_decoded_stock_application() {
    let decoded = padded_application(7);
    let stock = stock_update(&decoded);
    let sha256 = sha256_hex(&stock);
    let pinned = release(stock.len(), &sha256);

    let from_stock = rebuild_with_stock_application(&stock, &pinned, "Ver1.15").expect("stock");
    let explicit =
        rebuild_with_application(&stock, &pinned, &decoded, "Ver1.15").expect("explicit");

    assert_eq!(from_stock, explicit);
    assert_eq!(from_stock.application_sha256(), sha256_hex(&decoded));
}

#[test]
fn stock_rebuild_refuses_an_unpinned_input_and_a_bad_label() {
    let stock = stock_update(&padded_application(7));
    let sha256 = sha256_hex(&stock);

    assert_eq!(
        rebuild_with_stock_application(&stock, &release(1, &sha256), "Ver1.15"),
        Err(RebuildError::UnpinnedInput {
            sha256: sha256.clone()
        })
    );
    assert_eq!(
        rebuild_with_stock_application(&stock, &release(stock.len(), &sha256), "1.15"),
        Err(RebuildError::InvalidVersionLabel {
            label: "1.15".to_owned()
        })
    );
}
