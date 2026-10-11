//! Starting the emulator (`patch_cli::accept::Qemu`) with a fake one: a shell script that records
//! its arguments and environment, then exits. Needs no firmware and no emulator.
#![cfg(unix)]

use patch_cli::accept::{Qemu, QemuSetup, option_value, private_work_dir, profile};
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

fn fake_emulator(dir: &Path, record: &Path) -> std::path::PathBuf {
    let script = dir.join("fake-qemu");
    let body = format!(
        "#!/bin/sh\nprintf '%s\\n' \"$@\" > '{}'\nprintf 'ATA=%s KEYSOCK=%s FILL=%s\\n' \
         \"$CDJ_ATA\" \"$CDJ_PANEL_KEYSOCK\" \"${{CDJ_PANEL_FILL-unset}}\" >> '{}'\nexit 3\n",
        record.display(),
        record.display()
    );
    std::fs::write(&script, body).expect("script");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    script
}

#[test]
fn the_emulator_is_started_with_quoted_paths_and_an_early_exit_is_reported() {
    let base = private_work_dir().expect("dir");
    // A run directory and a stick image whose paths hold commas.
    let work = base.path().join("run,1");
    std::fs::create_dir(&work).expect("dir");
    std::fs::set_permissions(&work, std::fs::Permissions::from_mode(0o700)).expect("chmod");
    let stick = base.path().join("music,snapshot=off.img");
    std::fs::write(&stick, b"").expect("stick");
    let record = base.path().join("args.txt");
    let setup = QemuSetup {
        emulator: fake_emulator(base.path(), &record),
        profile: profile("xdj700-v1.15").expect("a profile"),
        work: work.clone(),
        flash: vec![0xFF; 16],
        stick: Some(stick.clone()),
        time_factor: 1,
    };
    // An owner's leftover emulator setting (update mode) must not reach the run.
    // SAFETY: no other thread of this test binary reads the environment meanwhile.
    unsafe { std::env::set_var("CDJ_PANEL_FILL", "0x14:16:17") };
    let error = Qemu::start(setup).err().expect("refused");
    unsafe { std::env::remove_var("CDJ_PANEL_FILL") };
    assert_eq!(
        error.to_string(),
        "the emulator exited before its monitor came up (exit status: 3)"
    );

    let args = std::fs::read_to_string(&record).expect("recorded");
    let lines: Vec<&str> = args.lines().collect();
    let quoted = |path: &Path| option_value(path).expect("utf-8");
    assert!(quoted(&work).contains("run,,1"));
    for expected in [
        "-M".to_owned(),
        "xdj700".to_owned(),
        format!("unix:{},server,nowait", quoted(&work.join("monitor.sock"))),
        format!(
            "if=pflash,format=raw,file={}",
            quoted(&work.join("flash.bin"))
        ),
        format!(
            "if=none,id=usbstick,format=raw,file={},snapshot=on",
            quoted(&stick)
        ),
        "usb-storage,drive=usbstick,port=1".to_owned(),
    ] {
        assert!(lines.contains(&expected.as_str()), "{expected}\n{args}");
    }
    // The panel keys go to a port the runner picked; the owner's leftover setting is removed.
    let env = lines.last().expect("env line");
    let port = env
        .strip_prefix("ATA=1 KEYSOCK=")
        .and_then(|rest| rest.strip_suffix(" FILL=unset"));
    // One the system handed out (ephemeral), not a fixed one.
    let port = port.and_then(|port| port.parse::<u16>().ok());
    assert!(port.is_some_and(|port| port > 1023), "{env}");
    // The flash image was written into the run directory.
    assert_eq!(
        std::fs::read(work.join("flash.bin")).expect("flash"),
        [0xFF; 16]
    );
}

#[test]
fn a_path_that_is_not_utf8_is_refused_as_an_option_value() {
    use std::os::unix::ffi::OsStrExt;
    let path = Path::new(std::ffi::OsStr::from_bytes(b"/tmp/\xff.img"));
    assert!(option_value(path).is_err());
    assert_eq!(option_value(Path::new("/a,b/c")).expect("utf-8"), "/a,,b/c");
}
