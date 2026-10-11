//! The acceptance runner's parts that need no emulator: the flash image of an update (synthetic,
//! no vendor bytes), frames as the emulator writes them, and its memory replies.

use patch_cli::accept::{
    Frame, Qemu, QemuSetup, flash_image, parse_ppm, parse_xp, private_work_dir, profile,
};
use patch_core::upd::crc16_xmodem;
use patch_schema::catalog::Region;

/// One S-record line (with CRLF), its checksum computed.
fn record(record_type: u8, address_len: usize, address: u32, data: &[u8]) -> Vec<u8> {
    let mut raw = vec![u8::try_from(address_len + data.len() + 1).expect("short")];
    raw.extend_from_slice(&address.to_be_bytes()[4 - address_len..]);
    raw.extend_from_slice(data);
    raw.push(!raw.iter().fold(0u8, |sum, &byte| sum.wrapping_add(byte)));
    let hex: String = raw.iter().map(|byte| format!("{byte:02X}")).collect();
    format!("S{}{hex}\r\n", record_type as char).into_bytes()
}

fn document(model: &str, kind: &str, lines: &[Vec<u8>]) -> Vec<u8> {
    let mut doc = format!("{model:<12}{kind}Ver1.15").into_bytes();
    doc.extend_from_slice(b"\0       0");
    assert_eq!(doc.len(), 32);
    for line in lines {
        doc.extend_from_slice(line);
    }
    let crc = crc16_xmodem(&doc);
    doc.extend_from_slice(&crc.to_le_bytes());
    doc
}

/// A synthetic update whose MAIN document holds `records` (address, data).
fn update(model: &str, records: &[(u32, &[u8])]) -> Vec<u8> {
    let mut lines = vec![record(b'0', 2, 0, b"synthetic")];
    lines.extend(records.iter().map(|(at, data)| record(b'2', 3, *at, data)));
    lines.push(record(b'8', 3, 0, &[]));
    let panel = vec![
        record(b'0', 2, 0, &[]),
        record(b'2', 3, 0x0C_0000, &[0x44; 3]),
        record(b'8', 3, 0, &[]),
    ];
    let documents = [
        document(model, "MAIN", &lines),
        document(model, "PANL", &panel),
    ];
    let mut bytes = Vec::new();
    for doc in &documents {
        bytes.extend_from_slice(format!("{}\r\n", doc.len()).as_bytes());
    }
    for doc in &documents {
        bytes.extend_from_slice(doc);
    }
    bytes
}

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
        "the update's MAIN record at 0xff ends past the 256-byte flash (at 0x101)"
    );
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
    };
    let error = Qemu::start(setup).err().expect("refused");
    assert!(
        error.to_string().contains("others may enter it (mode 755)"),
        "{error}"
    );
    assert!(!work.path().join("flash.bin").exists());

    // The command's own run directory is the owner's alone.
    let private = private_work_dir().expect("dir");
    let mode = std::fs::metadata(private.path())
        .expect("meta")
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
}
