//! The acceptance runner's parts that need no emulator: the flash image of an update (synthetic,
//! no vendor bytes), frames as the emulator writes them, and its memory replies.
#![cfg(unix)]

mod common;

use common::synthetic_upd::update;
use patch_cli::accept::{
    Frame, Qemu, QemuSetup, check_release, flash_image, hmp_string, hold_host_ms, keys_bound,
    parse_ppm, parse_xp, private_work_dir, profile,
};
use patch_core::xdj700::{APPLICATION_SECTION_OFFSET, encode_section, recipe_target};
use patch_schema::catalog::Region;
use std::path::Path;

#[test]
fn the_flash_holds_the_main_records_at_their_addresses_and_is_erased_elsewhere() {
    let flash =
        flash_image(&update("XDJ-700", &[(0x10, &[1, 2]), (0x40, &[3])]), 0x100).expect("flash");
    assert_eq!(flash.len(), 0x100);
    assert_eq!(&flash[0x10..0x12], [1, 2]);
    assert_eq!(flash[0x40], 3);
    let written = [0x10, 0x11, 0x40];
    assert!(
        (0..0x100)
            .filter(|i| !written.contains(i))
            .all(|i| flash[i] == 0xFF)
    );

    let error = flash_image(&update("XDJ-700", &[(0xFF, &[1, 2])]), 0x100).expect_err("refused");
    assert_eq!(
        error.to_string(),
        "the update's MAIN image ends past the 256-byte flash (at 0x101)"
    );
    // An update whose application does not decode cannot be held to a release.
    let error =
        check_release(&update("XDJ-700", &[(0, &[1])]), "xdj700-v1.15").expect_err("refused");
    assert_eq!(
        error.to_string(),
        "the update's application does not decode"
    );
    let error =
        check_release(&update("XDJ-700", &[(0, &[1])]), "xdj999-v9.99").expect_err("refused");
    assert_eq!(error.to_string(), "unknown release xdj999-v9.99");
    let error = flash_image(&update("CDJ-900", &[(0, &[1])]), 0x100).expect_err("refused");
    assert_eq!(error.to_string(), "the update has no XDJ-700 MAIN document");
    let error = flash_image(b"not an update", 0x100).expect_err("refused");
    assert_eq!(error.to_string(), "the update does not parse");

    let xdj700 = profile("xdj700-v1.15").expect("a profile");
    assert_eq!(
        (xdj700.machine, xdj700.flash_size),
        ("xdj700", 16 * 1024 * 1024)
    );
    assert!(profile("xdj1000mk2-v1.45").is_none());
}

#[test]
fn an_update_whose_application_is_not_the_releases_length_is_refused() {
    // Our own encoding of a small synthetic application, at the application's offset.
    let mut decoded = vec![0u8; 19];
    decoded.extend((0..200u8).map(|i| i / 3));
    let section = encode_section(&decoded).expect("encode");
    let offset = u32::try_from(APPLICATION_SECTION_OFFSET).expect("offset");
    let mut records: Vec<(u32, &[u8])> = vec![(0, &[0xFF])];
    let mut at = offset;
    for chunk in section.chunks(32) {
        records.push((at, chunk));
        at += 32;
    }
    let error = check_release(&update("XDJ-700", &records), "xdj700-v1.15").expect_err("refused");
    let expected = recipe_target("xdj700-v1.15")
        .expect("pinned")
        .application_len;
    assert_eq!(
        error.to_string(),
        format!(
            "the update is not of release xdj700-v1.15: its application is {} bytes, not \
             {expected}",
            decoded.len()
        )
    );
}

#[test]
fn presses_wait_for_the_key_socket_and_holds_for_the_time_factor() {
    assert!(!keys_bound("", 4000).expect("not yet"));
    // Another port's line (an earlier run's log) is not this run's socket.
    assert!(!keys_bound("panel: live keys on 4001 (udp 4001)\n", 4000).expect("not yet"));
    assert!(keys_bound("boot\npanel: live keys on 4000 (udp 4000)\n", 4000).expect("bound"));
    let error = keys_bound("panel: could NOT bind 127.0.0.1:4000\n", 4000).expect_err("refused");
    assert_eq!(
        error.to_string(),
        "the emulator could not bind its key socket (udp 4000): no press or touch would reach it"
    );
    assert_eq!(hold_host_ms(300, 4, 100), 1300);
    assert_eq!(hold_host_ms(10_000, 20, 100), 200_100);
    assert_eq!(
        hold_host_ms(u32::MAX, 20, 100),
        u64::from(u32::MAX) * 20 + 100
    );
}

#[test]
fn frames_are_binary_ppm_and_regions_are_measured_in_them() {
    let mut ppm = b"P6\n# a comment\n3 2\n255\n".to_vec();
    for grey in [0u8, 50, 100, 150, 200, 250] {
        ppm.extend_from_slice(&[grey, grey, grey]);
    }
    let frame = parse_ppm(&ppm).expect("frame");
    assert_eq!((frame.width, frame.height, frame.pixels.len()), (3, 2, 6));
    let region = |x, y, w, h| Region { x, y, w, h };
    assert_eq!(frame.brightness(&region(0, 0, 3, 2)).expect("in"), 125);
    assert_eq!(frame.brightness(&region(1, 1, 2, 1)).expect("in"), 225);
    assert_eq!(frame.brightness(&region(0, 0, 1, 2)).expect("in"), 75);
    let error = frame.brightness(&region(2, 0, 2, 1)).expect_err("refused");
    assert_eq!(
        error.to_string(),
        "region (2, 0, 2x1) is not in the 3x2 frame"
    );

    for (bytes, message) in [
        (b"P5\n1 1\n255\n\0".to_vec(), "not a binary PPM (P6)"),
        (
            b"P6\n1 1\n65535\n\0\0\0\0\0\0".to_vec(),
            "maximum value of 65535",
        ),
        (
            b"P6\n2 1\n255\n\0\0\0".to_vec(),
            "holds 3 bytes of pixels for 2x1",
        ),
        (
            b"P6\n1 1\n255\n\0\0\0\0".to_vec(),
            "holds 4 bytes of pixels for 1x1",
        ),
        (b"P6\n1".to_vec(), "its header ends early"),
        (b"P6\nx 1\n255\n\0\0\0".to_vec(), "bad number"),
    ] {
        let error = parse_ppm(&bytes).expect_err("refused").to_string();
        assert!(error.contains(message), "{message}: {error}");
    }
    let empty = Frame {
        width: 0,
        height: 0,
        pixels: Vec::new(),
    };
    assert!(empty.brightness(&region(0, 0, 1, 1)).is_err());
    // A frame from another emulator whose pixels do not match its size: an error, not a panic.
    let short = Frame {
        width: 4,
        height: 4,
        pixels: vec![(0, 0, 0); 3],
    };
    let error = short.brightness(&region(0, 0, 4, 4)).expect_err("refused");
    assert_eq!(error.to_string(), "the frame holds 3 pixels, not 4x4");
}

#[test]
fn memory_replies_are_read_for_the_address_asked() {
    let reply = "xp /1wx 0xf00000\r\n00f00000: 0xffffffff\r\n(qemu) ";
    assert_eq!(parse_xp(reply, 0x00f0_0000).expect("value"), 0xffff_ffff);
    // The monitor's echo may hold line-editing escapes before the reply.
    let reply = "\u{1b}[K\u{1b}[Dxp /1bx 0x10\r\n0000000000000010: 0x2a\r\n(qemu) ";
    assert_eq!(parse_xp(reply, 0x10).expect("value"), 42);
    let reply = "xp /1hx 0x20\r\n0000000000000020: 0x00ff\r\n(qemu) ";
    assert_eq!(parse_xp(reply, 0x20).expect("value"), 255);

    let error = parse_xp("0000000000000024: 0x01\r\n(qemu) ", 0x20).expect_err("refused");
    assert_eq!(
        error.to_string(),
        "the emulator did not report the value at 0x20"
    );
    let error = parse_xp("Cannot access memory\r\n(qemu) ", 0x20).expect_err("refused");
    assert_eq!(
        error.to_string(),
        "the emulator did not report the value at 0x20"
    );
    let error = parse_xp("00000020: 0x\r\n(qemu) ", 0x20).expect_err("refused");
    assert_eq!(
        error.to_string(),
        "the emulator's reply for 0x20 is not a value"
    );
}

#[test]
fn the_emulator_runs_only_in_a_directory_no_other_user_may_enter() {
    use std::os::unix::fs::PermissionsExt;
    let work = tempfile::tempdir().expect("tempdir");
    std::fs::set_permissions(work.path(), std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let setup = QemuSetup {
        emulator: work.path().join("never-run"),
        profile: profile("xdj700-v1.15").expect("a profile"),
        work: work.path().to_path_buf(),
        flash: Vec::new(),
        stick: None,
        time_factor: 1,
    };
    let error = Qemu::start(setup).err().expect("refused");
    assert!(
        error.to_string().contains("others may enter it (mode 755)"),
        "{error}"
    );
    assert!(!work.path().join("flash.bin").exists());

    // A hold cannot be waited out for no time at all.
    let private = private_work_dir().expect("dir");
    let setup = QemuSetup {
        emulator: private.path().join("never-run"),
        profile: profile("xdj700-v1.15").expect("a profile"),
        work: private.path().to_path_buf(),
        flash: Vec::new(),
        stick: None,
        time_factor: 0,
    };
    let error = Qemu::start(setup).err().expect("refused");
    assert_eq!(error.to_string(), "the time factor must be at least 1");

    // The command's own run directory is the owner's alone.
    let private = private_work_dir().expect("dir");
    let mode = std::fs::metadata(private.path())
        .expect("meta")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}

#[test]
fn a_run_directory_too_deep_for_the_monitor_socket_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let base = private_work_dir().expect("dir");
    let work = base.path().join("d".repeat(120));
    std::fs::create_dir(&work).expect("dir");
    std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let setup = QemuSetup {
        emulator: work.join("never-run"),
        profile: profile("xdj700-v1.15").expect("a profile"),
        work: work.clone(),
        flash: Vec::new(),
        stick: None,
        time_factor: 1,
    };
    let error = Qemu::start(setup).err().expect("refused");
    assert!(
        error
            .to_string()
            .contains("longer than 100 bytes, which Unix sockets cannot bind"),
        "{error}"
    );
}

#[test]
fn monitor_arguments_are_quoted_and_refused_when_they_cannot_be() {
    let quoted = hmp_string(Path::new("/tmp/a b/frame.ppm")).expect("quoted");
    assert_eq!(quoted, "\"/tmp/a b/frame.ppm\"");
    for path in ["/tmp/a\"b", "/tmp/a\\b"] {
        let error = hmp_string(Path::new(path)).expect_err("refused");
        assert!(
            error.to_string().contains("a quote or a backslash"),
            "{error}"
        );
    }
}
