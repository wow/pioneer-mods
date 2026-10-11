//! Running an acceptance test (`docs/acceptance.md`): its steps, in order, against an emulator
//! ([`Emulator`]), each check reported PASS or FAIL with what was read. [`Qemu`] runs the local
//! emulator; tests drive [`run_test`] with a scripted one.

mod flash;
mod frame;
mod qemu;

pub use flash::flash_image;
pub use frame::{Frame, parse_ppm};
pub use qemu::{Profile, Qemu, QemuSetup, parse_xp, profile};

use anyhow::{Result, bail};
use patch_schema::catalog::{AcceptanceTest, Check, Player, Step};

/// How often a `wait_until` check is read again, in milliseconds.
pub const POLL_MS: u32 = 500;

/// Largest update the runner reads (official updates are about 17 MB).
pub const MAX_UPDATE_LEN: u64 = 64 * 1024 * 1024;

/// Reads the update to run, size-capped; any update of the player (the official file, a stage
/// file, a build) may be run.
///
/// # Errors
///
/// A file that cannot be read, is not a regular file, or is larger than [`MAX_UPDATE_LEN`].
pub fn read_update(path: &std::path::Path) -> Result<Vec<u8>> {
    crate::recipe::read_capped(path, MAX_UPDATE_LEN, "update", "update", "any update")
}

/// What a test needs from an emulator: the player's panel and screen, and its memory.
pub trait Emulator {
    /// Presses the control with this code and value, released after `ms` milliseconds; returns
    /// once it is released.
    fn press(&mut self, code: u8, value: u8, ms: u32) -> Result<()>;
    /// Touches the screen at (`x`, `y`) for `ms` milliseconds; returns once it is released.
    fn touch(&mut self, x: u32, y: u32, ms: u32) -> Result<()>;
    /// Lets the emulated player run for `ms` milliseconds.
    fn wait(&mut self, ms: u32) -> Result<()>;
    /// The `bytes`-byte value (1, 2 or 4) at the guest physical `address`.
    fn read(&mut self, address: u32, bytes: u8) -> Result<u32>;
    /// The screen as it is now.
    fn frame(&mut self) -> Result<Frame>;
}

/// How a run went: one line per check, and how many failed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub lines: Vec<String>,
    pub checks: usize,
    pub failed: usize,
}

/// Runs `test` on `emulator` for `player`, whose controls and screen the catalog check has held
/// the test to. A failed check is recorded and the run goes on; an emulator error ends it.
///
/// # Errors
///
/// An emulator error, or a frame that is not the player's screen size.
pub fn run_test(
    test: &AcceptanceTest,
    player: &Player,
    emulator: &mut dyn Emulator,
) -> Result<Outcome> {
    let mut outcome = Outcome::default();
    for (index, step) in test.steps.iter().enumerate() {
        match step {
            Step::Wait { ms } => emulator.wait(*ms)?,
            Step::Press { control, ms } => {
                let Some((code, value)) = player.controls.get(control).and_then(|c| c.bytes())
                else {
                    bail!(
                        "steps[{index}]: player {} has no control {control}",
                        player.id
                    );
                };
                emulator.press(code, value, *ms)?;
            }
            Step::Touch { x, y, ms } => emulator.touch(*x, *y, *ms)?,
            Step::Expect(check) => {
                let (passed, seen) = evaluate(check, player, emulator)?;
                outcome.record(index, "expect", check, passed, &seen);
            }
            Step::WaitUntil { check, timeout_ms } => {
                let mut waited = 0;
                let (passed, seen) = loop {
                    let (passed, seen) = evaluate(check, player, emulator)?;
                    if passed || waited >= *timeout_ms {
                        break (passed, seen);
                    }
                    let pause = POLL_MS.min(timeout_ms - waited);
                    emulator.wait(pause)?;
                    waited += pause;
                };
                let seen = format!("{seen}, after {waited} ms of {timeout_ms}");
                outcome.record(index, "wait_until", check, passed, &seen);
            }
        }
    }
    Ok(outcome)
}

impl Outcome {
    fn record(&mut self, index: usize, kind: &str, check: &Check, passed: bool, seen: &str) {
        self.checks += 1;
        self.failed += usize::from(!passed);
        let verdict = if passed { "PASS" } else { "FAIL" };
        self.lines.push(format!(
            "steps[{index}] {kind} {}: {verdict} ({seen})",
            describe(check)
        ));
    }
}

/// Whether `check` holds now, and what was read.
fn evaluate(check: &Check, player: &Player, emulator: &mut dyn Emulator) -> Result<(bool, String)> {
    match check {
        Check::Memory {
            address,
            bytes,
            equals,
        } => {
            let value = emulator.read(address.0, *bytes)?;
            Ok((value == *equals, format!("read {value}")))
        }
        Check::Brightest {
            region,
            others,
            margin,
        } => {
            let frame = emulator.frame()?;
            let screen = &player.screen_class;
            if (frame.width, frame.height) != (screen.width, screen.height) {
                bail!(
                    "the emulator's frame is {}x{}, not {}'s {}x{} screen",
                    frame.width,
                    frame.height,
                    player.id,
                    screen.width,
                    screen.height
                );
            }
            let bright = frame.brightness(region)?;
            let mut means = Vec::with_capacity(others.len());
            for other in others {
                means.push(frame.brightness(other)?);
            }
            let passed = means
                .iter()
                .all(|&other| u16::from(bright) >= u16::from(other) + u16::from(*margin));
            let means: Vec<String> = means.iter().map(u8::to_string).collect();
            Ok((
                passed,
                format!("brightness {bright}; others {}", means.join(", ")),
            ))
        }
    }
}

fn describe(check: &Check) -> String {
    match check {
        Check::Memory {
            address,
            bytes,
            equals,
        } => format!(
            "memory {:#010x} ({bytes} byte{}) == {equals}",
            address.0,
            if *bytes == 1 { "" } else { "s" }
        ),
        Check::Brightest {
            region,
            others,
            margin,
        } => format!(
            "brightest ({}, {}, {}x{}) over {} regions by {margin}",
            region.x,
            region.y,
            region.w,
            region.h,
            others.len()
        ),
    }
}
