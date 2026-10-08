//! The application's reported version (`xdj700::app_version`), on synthetic applications.

use patch_core::RebuildError;
use patch_core::xdj700::{
    VERSION_STRING_OFFSET, reported_version, validate_reported_version, with_reported_version,
};

/// A synthetic decoded application with `field` at the version-string offset.
fn application(field: &[u8]) -> Vec<u8> {
    let mut decoded: Vec<u8> = (0..0x1000u32).map(|i| (i * 7) as u8).collect();
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
    for version in ["1.5", "0.100", "a.bc", "Ver0.10", "0,10", ""] {
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
