//! The application's reported version (`xdj700::app_version`), on synthetic applications.

mod common;

use common::xdj700::{rebuild, release, stock_update};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, VERSION_STRING_OFFSET, decode_section, main_document,
    rebuild_with_stock_application_reporting, reported_version, validate_reported_version,
    with_reported_version,
};
use patch_core::{RebuildError, parse_upd, sha256_hex};

/// A synthetic decoded application (the 19-byte zero seed, then a pattern) with `field` at the
/// version-string offset.
fn application(field: &[u8]) -> Vec<u8> {
    let mut decoded: Vec<u8> = (0..0x1000u32).map(|i| (i * 7) as u8).collect();
    decoded[..19].fill(0);
    decoded[VERSION_STRING_OFFSET..VERSION_STRING_OFFSET + field.len()].copy_from_slice(field);
    decoded
}

#[test]
fn reads_the_version_string() {
    assert_eq!(reported_version(&application(b"1.15\0")), Some("1.15"));
    assert_eq!(reported_version(&application(b"0.10\0")), Some("0.10"));
}

#[test]
fn reads_nothing_without_a_well_formed_version_string() {
    assert_eq!(reported_version(&application(b"1.15x")), None, "no NUL");
    assert_eq!(reported_version(&application(b"1x15\0")), None);
    assert_eq!(
        reported_version(&application(b"x.15\0")),
        None,
        "first digit"
    );
    assert_eq!(
        reported_version(&application(b"\xff.15\0")),
        None,
        "non-ASCII"
    );
    assert_eq!(reported_version(&application(b"11.5\0")), None);
    assert_eq!(
        reported_version(&[0; VERSION_STRING_OFFSET + 4]),
        None,
        "too short"
    );
}

#[test]
fn sets_a_lower_version_and_changes_nothing_outside_the_field() {
    let stock = application(b"1.15\0");

    let modified = with_reported_version(&stock, "0.10").expect("lower version");

    assert_eq!(reported_version(&modified), Some("0.10"));
    assert_eq!(modified.len(), stock.len());
    let changed: Vec<usize> = (0..stock.len())
        .filter(|&i| stock[i] != modified[i])
        .collect();
    assert_eq!(
        changed,
        [VERSION_STRING_OFFSET, VERSION_STRING_OFFSET + 3],
        "1.15 -> 0.10 differs only in its first and last digits; nothing outside the field"
    );
    assert_eq!(
        with_reported_version(&stock, "1.14").map(|m| reported_version(&m).map(str::to_owned)),
        Ok(Some("1.14".to_owned()))
    );
}

#[test]
fn refuses_a_version_not_lower_than_the_official_one() {
    for version in ["1.15", "1.16", "9.99"] {
        let expected = Err(RebuildError::ReportedVersionNotLower {
            version: version.to_owned(),
            official: "1.15".to_owned(),
        });
        assert_eq!(validate_reported_version(version), expected);
        assert_eq!(
            with_reported_version(&application(b"1.15\0"), version),
            expected.map(|()| Vec::new())
        );
    }
}

#[test]
fn refuses_a_malformed_version() {
    for version in ["1.5", "0.100", "a.bc", "x.10", "Ver0.10", "0,10", ""] {
        assert_eq!(
            validate_reported_version(version),
            Err(RebuildError::InvalidReportedVersion {
                version: version.to_owned()
            }),
            "{version:?}"
        );
    }
}

#[test]
fn refuses_an_application_without_a_version_string() {
    assert_eq!(
        with_reported_version(&application(b"1x15\0"), "0.10"),
        Err(RebuildError::MissingVersionString {
            offset: VERSION_STRING_OFFSET
        })
    );
}

#[test]
fn rebuilds_the_stock_application_reporting_a_lower_version() {
    let decoded = application(b"1.15\0");
    let stock = stock_update(&decoded);
    let sha256 = sha256_hex(&stock);

    let rebuilt = rebuild_with_stock_application_reporting(
        &stock,
        &release(stock.len(), &sha256),
        "0.10",
        "Ver1.16",
    )
    .expect("rebuild");

    let expected = with_reported_version(&decoded, "0.10").expect("lower version");
    assert_eq!(rebuilt.application_reported_version(), Some("0.10"));
    assert_eq!(rebuilt.application_sha256(), sha256_hex(&expected));
    // The synthetic loader is not a verified one, so decode the section directly.
    let parsed = parse_upd(rebuilt.bytes()).expect("parse");
    let image = main_document(&parsed)
        .expect("main")
        .image()
        .expect("image");
    let output = decode_section(image.bytes(), APPLICATION_SECTION_OFFSET).expect("decode");
    assert_eq!(output.decoded(), expected.as_slice());
    let plain = rebuild(&stock, &decoded, "Ver1.16").expect("no-op rebuild");
    assert_eq!(plain.application_reported_version(), Some("1.15"));
    assert_ne!(plain.sha256(), rebuilt.sha256());
}

#[test]
fn reporting_rebuild_validates_the_version_then_pins_the_input_before_decoding() {
    let not_a_update = b"not an update".to_vec();
    let pinned_elsewhere = release(not_a_update.len(), "00");

    assert_eq!(
        rebuild_with_stock_application_reporting(
            &not_a_update,
            &pinned_elsewhere,
            "1.15",
            "Ver1.16"
        ),
        Err(RebuildError::ReportedVersionNotLower {
            version: "1.15".to_owned(),
            official: "1.15".to_owned()
        })
    );
    assert_eq!(
        rebuild_with_stock_application_reporting(
            &not_a_update,
            &pinned_elsewhere,
            "0.10",
            "Ver1.16"
        ),
        Err(RebuildError::UnpinnedInput {
            sha256: sha256_hex(&not_a_update)
        })
    );
}

#[test]
fn reporting_rebuild_refuses_an_application_without_a_version_string() {
    let stock = stock_update(&application(b"1x15\0"));
    let sha256 = sha256_hex(&stock);

    assert_eq!(
        rebuild_with_stock_application_reporting(
            &stock,
            &release(stock.len(), &sha256),
            "0.10",
            "Ver1.16"
        ),
        Err(RebuildError::MissingVersionString {
            offset: VERSION_STRING_OFFSET
        })
    );
}
