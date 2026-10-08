//! The application's reported version (`xdj700::app_version`) and the release rule for modified
//! applications, on synthetic applications.

mod common;

use common::xdj700::{release, stock_update};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, OFFICIAL_V115_VERSION_BLOCK, StockRelease, VERSION_STRING_OFFSET,
    VersionBlock, decode_section, main_document, rebuild_with_application,
    rebuild_with_stock_application_reporting, reported_version, reported_version_at,
    verify_rebuild,
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

/// A synthetic version block whose stock application has SHA-256 `stock_sha256`.
fn block(stock_sha256: &str) -> VersionBlock<'_> {
    VersionBlock {
        offset: VERSION_STRING_OFFSET,
        stock_version: "1.15",
        stock_application_sha256: stock_sha256,
    }
}

/// The synthetic release of `update`, with `block`.
fn release_with<'a>(update: &[u8], sha256: &'a str, block: VersionBlock<'a>) -> StockRelease<'a> {
    StockRelease {
        version_block: Some(block),
        ..release(update.len(), sha256)
    }
}

fn not_lower(version: &str) -> RebuildError {
    RebuildError::ReportedVersionNotLower {
        version: version.to_owned(),
        official: "1.15".to_owned(),
    }
}

#[test]
fn reads_the_version_string() {
    assert_eq!(reported_version(&application(b"1.15\0")), Some("1.15"));
    assert_eq!(reported_version(&application(b"0.10\0")), Some("0.10"));
    assert_eq!(reported_version_at(b"xx0.10\0", 2), Some("0.10"));
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
    assert_eq!(
        reported_version_at(b"0.10\0", usize::MAX),
        None,
        "offset overflow"
    );
}

#[test]
fn sets_a_lower_version_and_changes_nothing_outside_the_field() {
    let stock = application(b"1.15\0");
    let mut modified = stock.clone();

    OFFICIAL_V115_VERSION_BLOCK
        .set_reported_version(&mut modified, "0.10")
        .expect("lower version");

    assert_eq!(reported_version(&modified), Some("0.10"));
    let changed: Vec<usize> = (0..stock.len())
        .filter(|&i| stock[i] != modified[i])
        .collect();
    assert_eq!(
        changed,
        [VERSION_STRING_OFFSET, VERSION_STRING_OFFSET + 3],
        "1.15 -> 0.10 differs only in its first and last digits; nothing outside the field"
    );
}

#[test]
fn refuses_a_version_not_lower_than_the_stock_one() {
    for version in ["1.15", "1.16", "9.99"] {
        let mut decoded = application(b"1.15\0");
        assert_eq!(
            OFFICIAL_V115_VERSION_BLOCK.validate_reported_version(version),
            Err(not_lower(version))
        );
        assert_eq!(
            OFFICIAL_V115_VERSION_BLOCK.set_reported_version(&mut decoded, version),
            Err(not_lower(version))
        );
        assert_eq!(reported_version(&decoded), Some("1.15"), "unchanged");
    }
    assert_eq!(
        OFFICIAL_V115_VERSION_BLOCK.validate_reported_version("1.14"),
        Ok(())
    );
}

#[test]
fn refuses_a_malformed_version() {
    for version in ["1.5", "0.100", "a.bc", "x.10", "Ver0.10", "0,10", ""] {
        assert_eq!(
            OFFICIAL_V115_VERSION_BLOCK.validate_reported_version(version),
            Err(RebuildError::InvalidReportedVersion {
                version: version.to_owned()
            }),
            "{version:?}"
        );
    }
}

#[test]
fn refuses_to_set_a_version_without_a_version_string() {
    assert_eq!(
        OFFICIAL_V115_VERSION_BLOCK.set_reported_version(&mut application(b"1x15\0"), "0.10"),
        Err(RebuildError::MissingVersionString {
            offset: VERSION_STRING_OFFSET
        })
    );
}

#[test]
fn the_rule_accepts_the_stock_application_and_lower_reporting_modifications() {
    let stock = application(b"1.15\0");
    let stock_sha256 = sha256_hex(&stock);
    let block = block(&stock_sha256);

    assert_eq!(block.check_application(&stock), Ok(()));
    assert_eq!(block.check_application(&application(b"0.10\0")), Ok(()));
    assert_eq!(block.check_application(&application(b"1.14\0")), Ok(()));
}

#[test]
fn the_rule_refuses_a_modification_reporting_no_lower_version() {
    let stock_sha256 = sha256_hex(&application(b"1.15\0"));
    let block = block(&stock_sha256);
    let mut modified_code = application(b"1.15\0");
    modified_code[0x800] ^= 1;

    for (decoded, reported) in [
        (modified_code, Some("1.15")),
        (application(b"1.16\0"), Some("1.16")),
        (application(b"1x15\0"), None),
    ] {
        assert_eq!(
            block.check_application(&decoded),
            Err(RebuildError::ModifiedApplicationVersion {
                reported: reported.map(str::to_owned),
                official: "1.15".to_owned()
            }),
            "{reported:?}"
        );
    }
}

#[test]
fn rebuild_and_verify_enforce_the_rule() {
    let stock_application = application(b"1.15\0");
    let stock = stock_update(&stock_application);
    let (upd_sha256, stock_sha256) = (sha256_hex(&stock), sha256_hex(&stock_application));
    let with_rule = release_with(&stock, &upd_sha256, block(&stock_sha256));
    let without_rule = release(stock.len(), &upd_sha256);
    let higher = application(b"1.16\0");
    let refused = Err(RebuildError::ModifiedApplicationVersion {
        reported: Some("1.16".to_owned()),
        official: "1.15".to_owned(),
    });

    assert_eq!(
        rebuild_with_application(&stock, &with_rule, &higher, "Ver1.17").map(|_| ()),
        refused
    );
    let output = rebuild_with_application(&stock, &without_rule, &higher, "Ver1.17")
        .expect("a synthetic release without a version block has no rule");
    assert_eq!(
        verify_rebuild(&stock, &with_rule, output.bytes(), &higher, "Ver1.17"),
        refused
    );
    assert_eq!(
        verify_rebuild(&stock, &without_rule, output.bytes(), &higher, "Ver1.17"),
        Ok(())
    );
}

#[test]
fn rebuilds_the_stock_application_reporting_a_lower_version() {
    let decoded = application(b"1.15\0");
    let stock = stock_update(&decoded);
    let (upd_sha256, stock_sha256) = (sha256_hex(&stock), sha256_hex(&decoded));
    let with_rule = release_with(&stock, &upd_sha256, block(&stock_sha256));

    let rebuilt = rebuild_with_stock_application_reporting(&stock, &with_rule, "0.10", "Ver1.16")
        .expect("rebuild");

    let mut expected = decoded.clone();
    block(&stock_sha256)
        .set_reported_version(&mut expected, "0.10")
        .expect("lower version");
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
    let same = rebuild_with_application(&stock, &with_rule, &expected, "Ver1.16").expect("same");
    assert_eq!(same, rebuilt);
}

#[test]
fn reporting_rebuild_checks_its_arguments_then_pins_the_input_before_decoding() {
    let not_an_update = b"not an update".to_vec();
    let stock_sha256 = sha256_hex(b"stock");
    let pinned_elsewhere = release_with(&not_an_update, "00", block(&stock_sha256));
    let reporting = |release: &StockRelease<'_>, version: &str, label: &str| {
        rebuild_with_stock_application_reporting(&not_an_update, release, version, label)
            .map(|_| ())
    };

    assert_eq!(
        reporting(&pinned_elsewhere, "0.10", "1.16"),
        Err(RebuildError::InvalidVersionLabel {
            label: "1.16".to_owned()
        })
    );
    assert_eq!(
        reporting(&release(not_an_update.len(), "00"), "0.10", "Ver1.16"),
        Err(RebuildError::NoVersionBlock)
    );
    assert_eq!(
        reporting(&pinned_elsewhere, "1.15", "Ver1.16"),
        Err(not_lower("1.15"))
    );
    assert_eq!(
        reporting(&pinned_elsewhere, "0.10", "Ver1.16"),
        Err(RebuildError::UnpinnedInput {
            sha256: sha256_hex(&not_an_update)
        })
    );
}

#[test]
fn reporting_rebuild_refuses_an_application_without_a_version_string() {
    let decoded = application(b"1x15\0");
    let stock = stock_update(&decoded);
    let (upd_sha256, stock_sha256) = (sha256_hex(&stock), sha256_hex(&decoded));

    assert_eq!(
        rebuild_with_stock_application_reporting(
            &stock,
            &release_with(&stock, &upd_sha256, block(&stock_sha256)),
            "0.10",
            "Ver1.16"
        )
        .map(|_| ()),
        Err(RebuildError::MissingVersionString {
            offset: VERSION_STRING_OFFSET
        })
    );
}

#[test]
fn the_reported_version_is_read_at_the_release_block_offset() {
    // A release whose block sits at 0x100, with an unrelated well-formed string at 0x740.
    let mut decoded = application(b"9.99\0");
    decoded[0x100..0x105].copy_from_slice(b"1.15\0");
    let stock = stock_update(&decoded);
    let (upd_sha256, stock_sha256) = (sha256_hex(&stock), sha256_hex(&decoded));
    let at_0x100 = VersionBlock {
        offset: 0x100,
        ..block(&stock_sha256)
    };

    let rebuilt = rebuild_with_stock_application_reporting(
        &stock,
        &release_with(&stock, &upd_sha256, at_0x100),
        "0.10",
        "Ver1.16",
    )
    .expect("rebuild");

    assert_eq!(rebuilt.application_reported_version(), Some("0.10"));
}

#[test]
fn the_refusal_names_what_the_application_reports() {
    let message = |reported: Option<&str>| {
        RebuildError::ModifiedApplicationVersion {
            reported: reported.map(str::to_owned),
            official: "1.15".to_owned(),
        }
        .to_string()
    };

    assert!(
        message(Some("1.16")).contains("(it reports 1.16)"),
        "{}",
        message(Some("1.16"))
    );
    assert!(
        message(None).contains("(it reports no version string of the form X.YY)"),
        "{}",
        message(None)
    );
}
