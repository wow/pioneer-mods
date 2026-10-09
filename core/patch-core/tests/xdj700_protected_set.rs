//! The protected set's file format: `ProtectedSet::parse` accepts the measured layout and refuses
//! every malformed line, a missing or wrong release, and ranges outside the application. Synthetic
//! ranges only; the measured set stays outside the repository. The checks against recipes are in
//! `xdj700_protected_set_check.rs`.

use patch_core::xdj700::{ProtectedSet, ProtectedSetError, RecipeTarget, recipe_target};

const RELEASE: &str = "xdj700-v1.15";
const LAST: u64 = 0x0800_0000 + 18_601_864 - 1;

fn target() -> &'static RecipeTarget<'static> {
    recipe_target(RELEASE).expect("known release")
}

/// Parses `ranges` (lines after the release line) for v1.15.
fn parse(ranges: &str) -> Result<ProtectedSet, ProtectedSetError> {
    ProtectedSet::parse(&format!("release {RELEASE}\n{ranges}"), target())
}

#[test]
fn parses_the_measured_format_with_header_comments_and_prefixes() {
    let set = ProtectedSet::parse(
        "\u{feff}# measured locally\n\nrelease xdj700-v1.15\nstart\tend\tbytes\n\
         08000800\t08000805\t6\n0x08000810 0X08000849\r\n",
        target(),
    )
    .expect("valid");

    assert_eq!(set.release(), RELEASE);
    assert_eq!(set.len(), 2);
    assert!(!set.is_empty());
    assert_eq!(set.ranges(), &[0x800..0x806, 0x810..0x84a]);
    assert_eq!(
        parse("start end\n08000800 08000805\n")
            .expect("valid")
            .len(),
        1
    );
}

#[test]
fn accepts_the_first_and_last_addresses_of_the_application() {
    let set = parse(&format!("08000000 08000000\n{LAST:x} {LAST:x}\n")).expect("valid");

    assert_eq!(set.ranges(), &[0..1, 18_601_863..18_601_864]);
}

#[test]
fn requires_the_release_first_and_refuses_another_release() {
    let no_release = |line, found: &str| {
        Err(ProtectedSetError::NoRelease {
            line,
            found: found.to_owned(),
        })
    };
    assert_eq!(
        ProtectedSet::parse("08000800 08000805\n", target()),
        no_release(1, "08000800 08000805")
    );
    assert_eq!(
        ProtectedSet::parse("# set\nstart end bytes\n", target()),
        no_release(2, "start end bytes")
    );
    assert_eq!(
        ProtectedSet::parse("release\n", target()),
        no_release(1, "release")
    );
    assert_eq!(
        ProtectedSet::parse("version xdj700-v1.15\n", target()),
        no_release(1, "version xdj700-v1.15")
    );
    assert_eq!(
        ProtectedSet::parse("release xdj700-v1.16\n08000800 08000805\n", target()),
        Err(ProtectedSetError::WrongRelease {
            set: "xdj700-v1.16".to_owned(),
            target: RELEASE.to_owned()
        })
    );
}

#[test]
fn refuses_an_empty_set() {
    assert_eq!(
        ProtectedSet::parse("", target()),
        Err(ProtectedSetError::Empty)
    );
    assert_eq!(parse(""), Err(ProtectedSetError::Empty));
    assert_eq!(
        parse("# only a comment\nstart end bytes\n"),
        Err(ProtectedSetError::Empty)
    );
}

#[test]
fn accepts_only_an_exact_header_and_only_before_the_first_range() {
    // A range line that begins with the word `start` is not taken for a header.
    let not_hex = |line, value: &str| {
        Err(ProtectedSetError::NotHex {
            line,
            field: "start",
            value: value.to_owned(),
        })
    };
    assert_eq!(parse("start 080d6300 080d63ff\n"), not_hex(2, "start"));
    assert_eq!(parse("start_addr end bytes\n"), not_hex(2, "start_addr"));
    assert_eq!(parse("start,end,bytes x\n"), not_hex(2, "start,end,bytes"));
    assert_eq!(
        parse("08000800 08000805\nstart end bytes\n"),
        not_hex(3, "start")
    );
    assert_eq!(
        parse("start end\nstart end\n08000800 08000805\n"),
        not_hex(3, "start")
    );
}

#[test]
fn refuses_malformed_lines_with_their_line_number() {
    assert_eq!(
        parse("08000800\n"),
        Err(ProtectedSetError::Fields { line: 2, fields: 1 })
    );
    assert_eq!(
        parse("start,end,bytes\n"),
        Err(ProtectedSetError::Fields { line: 2, fields: 1 })
    );
    assert_eq!(
        parse("08000800 08000805 6 extra\n"),
        Err(ProtectedSetError::Fields { line: 2, fields: 4 })
    );
    assert_eq!(
        parse("\n08000800 0800080g\n"),
        Err(ProtectedSetError::NotHex {
            line: 3,
            field: "end",
            value: "0800080g".to_owned()
        })
    );
    for value in ["+8000800", "0x"] {
        assert_eq!(
            parse(&format!("{value} 08000805\n")),
            Err(ProtectedSetError::NotHex {
                line: 2,
                field: "start",
                value: value.to_owned()
            })
        );
    }
    // Decimal digits only: no sign, as for the addresses.
    for value in ["six", "+6", "-6", "0x6"] {
        assert_eq!(
            parse(&format!("08000800 08000805 {value}\n")),
            Err(ProtectedSetError::NotCount {
                line: 2,
                value: value.to_owned()
            })
        );
    }
    assert_eq!(
        parse("08000805 08000800\n"),
        Err(ProtectedSetError::Reversed {
            line: 2,
            start: 0x0800_0805,
            end: 0x0800_0800
        })
    );
}

#[test]
fn refuses_a_byte_count_that_does_not_match_inclusive_ends() {
    // A file written with exclusive ends would carry 5 here.
    assert_eq!(
        parse("08000800 08000805 5\n"),
        Err(ProtectedSetError::ByteCount {
            line: 2,
            start: 0x0800_0800,
            end: 0x0800_0805,
            count: 5,
            expected: 6
        })
    );
}

#[test]
fn refuses_decoded_offsets_and_ranges_past_the_application() {
    let outside = |start, end| {
        Err(ProtectedSetError::OutsideApplication {
            line: 2,
            start,
            end,
            app_start: 0x0800_0000,
            app_end: LAST,
        })
    };
    // Decoded offsets instead of run-time addresses.
    assert_eq!(parse("00000800 00000805\n"), outside(0x800, 0x805));
    assert_eq!(
        parse(&format!("{:x} {:x}\n", LAST + 1, LAST + 9)),
        outside(LAST + 1, LAST + 9)
    );
    assert_eq!(
        parse("ffffffffffffffff ffffffffffffffff\n"),
        outside(u64::MAX, u64::MAX)
    );
    // The widest range: refused by its bounds before its byte count is computed.
    assert_eq!(parse("0 ffffffffffffffff\n"), outside(0, u64::MAX));
    assert_eq!(parse("0 ffffffffffffffff 0\n"), outside(0, u64::MAX));
    assert_eq!(
        parse("123456789abcdef01 123456789abcdef02\n"),
        Err(ProtectedSetError::NotHex {
            line: 2,
            field: "start",
            value: "123456789abcdef01".to_owned()
        })
    );
}

#[test]
fn refuses_a_target_without_an_application() {
    let empty = RecipeTarget {
        application_len: 0,
        ..*target()
    };

    assert_eq!(
        ProtectedSet::parse("release xdj700-v1.15\n08000000 08000000\n", &empty),
        Err(ProtectedSetError::NoApplication {
            target: RELEASE.to_owned()
        })
    );
}
