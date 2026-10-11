//! Ctrl-C before the emulator starts (`patch_cli::accept::catch_interrupts`): the emulator is
//! never started, and a second Ctrl-C would end the process. Its own test binary, since the
//! interrupt is process-wide. Needs no firmware and no emulator.
#![cfg(unix)]

use patch_cli::accept::{
    Qemu, QemuSetup, catch_interrupts, interrupted, private_work_dir, profile,
};
use std::os::unix::fs::PermissionsExt;

#[test]
fn an_interrupt_before_the_spawn_starts_no_emulator_and_a_second_one_ends_the_process() {
    let base = private_work_dir().expect("dir");
    let started = base.path().join("started");
    let emulator = base.path().join("fake-qemu");
    let script = format!("#!/bin/sh\ntouch '{}'\n", started.display());
    std::fs::write(&emulator, script).expect("script");
    std::fs::set_permissions(&emulator, std::fs::Permissions::from_mode(0o700)).expect("chmod");

    catch_interrupts();
    assert!(!interrupted());
    // SAFETY: raising a signal the handler above catches.
    unsafe { libc::raise(libc::SIGINT) };
    assert!(interrupted());
    // The handler gave SIGINT back to its default: a second Ctrl-C ends the process.
    // SAFETY: querying and restoring the handler, nothing else.
    let after = unsafe { libc::signal(libc::SIGINT, libc::SIG_DFL) };
    assert_eq!(after, libc::SIG_DFL);

    let setup = QemuSetup {
        emulator,
        profile: profile("xdj700-v1.15").expect("a profile"),
        work: base.path().to_path_buf(),
        flash: Vec::new(),
        stick: None,
        time_factor: 1,
    };
    let error = Qemu::start(setup).err().expect("refused");
    assert_eq!(
        error.to_string(),
        "interrupted; the emulator is stopped and the run's directory removed"
    );
    assert!(!started.exists());
    assert!(!base.path().join("flash.bin").exists());
}
