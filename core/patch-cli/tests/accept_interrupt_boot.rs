//! Ctrl-C while the emulator boots: the terminal sends it to the emulator too, which may exit
//! first; the run still says it was interrupted. Its own test binary, since the interrupt is
//! process-wide. Needs no firmware and no emulator.
#![cfg(unix)]

use patch_cli::accept::{Qemu, QemuSetup, catch_interrupts, private_work_dir, profile};
use std::os::unix::fs::PermissionsExt;

#[test]
fn an_interrupt_that_also_ends_the_emulator_is_reported_as_an_interrupt() {
    let base = private_work_dir().expect("dir");
    // A fake emulator that interrupts the runner, as a Ctrl-C would, and exits once the signal
    // has surely been handled (it is process-wide: any thread may take it).
    let emulator = base.path().join("fake-qemu");
    let script = "#!/bin/sh\nkill -INT $PPID\nsleep 1\nexit 130\n";
    std::fs::write(&emulator, script).expect("script");
    std::fs::set_permissions(&emulator, std::fs::Permissions::from_mode(0o700)).expect("chmod");

    catch_interrupts();
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
}
