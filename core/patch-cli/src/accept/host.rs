//! What the runner needs from the host around the emulator: a private run directory, QEMU option
//! values, a free port, the monitor's replies, and interruption (Ctrl-C or SIGTERM), which ends a
//! run between steps so that the emulator is stopped and the run directory removed.

use anyhow::{Context, Result, bail};
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};

static INTERRUPTED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_signal(signal: libc::c_int) {
    INTERRUPTED.store(true, Ordering::SeqCst);
    // A second Ctrl-C ends the process, should the run be stuck where no check runs.
    // SAFETY: `signal` is async-signal-safe.
    unsafe {
        libc::signal(signal, libc::SIG_DFL);
    }
}

/// Makes SIGINT and SIGTERM set a flag ([`interrupted`]) instead of ending the process, so that a
/// run stops between steps and cleans up; a second one ends the process. The runner's command
/// calls it once, first.
pub fn catch_interrupts() {
    let handler = on_signal as extern "C" fn(libc::c_int) as libc::sighandler_t;
    // SAFETY: the handler only stores to an atomic and resets its signal, both
    // async-signal-safe.
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGTERM, handler);
    }
}

/// Whether SIGINT or SIGTERM arrived since [`catch_interrupts`].
pub fn interrupted() -> bool {
    INTERRUPTED.load(Ordering::SeqCst)
}

/// An error if the run was interrupted.
///
/// # Errors
///
/// The run was interrupted.
pub fn check_interrupted() -> Result<()> {
    if interrupted() {
        bail!("interrupted; the emulator is stopped and the run's directory removed");
    }
    Ok(())
}

/// The value of an `xp` reply: the line `<address>: 0x<value>`, the address being `address`.
pub fn parse_xp(output: &str, address: u32) -> Result<u32> {
    for line in output.lines() {
        let Some((at, value)) = line.trim().split_once(": 0x") else {
            continue;
        };
        let at = at
            .rsplit(|c: char| !c.is_ascii_hexdigit())
            .next()
            .unwrap_or("");
        let value: String = value.chars().take_while(char::is_ascii_hexdigit).collect();
        if u64::from_str_radix(at, 16).ok() == Some(u64::from(address)) {
            return u32::from_str_radix(&value, 16)
                .with_context(|| format!("the emulator's reply for {address:#x} is not a value"));
        }
    }
    bail!("the emulator did not report the value at {address:#x}")
}

/// `path` as a QEMU option value: UTF-8, each comma doubled (QEMU separates options with commas).
///
/// # Errors
///
/// A path that is not UTF-8.
pub fn option_value(path: &std::path::Path) -> Result<String> {
    let text = path
        .to_str()
        .with_context(|| format!("the path '{}' is not UTF-8", path.display()))?;
    Ok(text.replace(',', ",,"))
}

/// `path` as a quoted monitor (HMP) argument, which the monitor would otherwise split at spaces.
///
/// # Errors
///
/// A path that is not UTF-8, or holds a `"` or a `\`.
pub fn hmp_string(path: &std::path::Path) -> Result<String> {
    let text = path
        .to_str()
        .with_context(|| format!("the path '{}' is not UTF-8", path.display()))?;
    if text.contains(['"', '\\']) {
        bail!("the path '{text}' holds a quote or a backslash, which the monitor cannot take");
    }
    Ok(format!("\"{text}\""))
}

/// A new run directory only the owner may enter (mode 0700 from creation, whatever the umask),
/// removed when dropped.
///
/// # Errors
///
/// The directory cannot be created.
pub fn private_work_dir() -> Result<tempfile::TempDir> {
    use std::os::unix::fs::PermissionsExt;
    tempfile::Builder::new()
        .permissions(std::fs::Permissions::from_mode(0o700))
        .tempdir()
        .context("failed to create the run's directory")
}

/// The line the emulator logs once its key socket is bound, and the one it logs if it cannot be.
const KEYS_LIVE: &str = "panel: live keys on ";
const KEYS_FAILED: &str = "panel: could NOT bind ";

/// Whether the emulator's log says its key socket on `port` is bound (`false`: not yet).
///
/// # Errors
///
/// The log says the emulator could not bind it.
pub fn keys_bound(log: &str, port: u16) -> Result<bool> {
    if log.contains(&format!("{KEYS_LIVE}{port} (udp {port})")) {
        return Ok(true);
    }
    if log.contains(KEYS_FAILED) {
        bail!(
            "the emulator could not bind its key socket (udp {port}): no press or touch would \
             reach it"
        );
    }
    Ok(false)
}

/// The host milliseconds a hold of `ms` emulated milliseconds is waited out for: `ms` times
/// `time_factor` (the emulator's clock runs slower than the host's), then `settle_ms` for the
/// emulator to act on the release.
pub fn hold_host_ms(ms: u32, time_factor: u32, settle_ms: u32) -> u64 {
    u64::from(ms) * u64::from(time_factor) + u64::from(settle_ms)
}

pub(super) fn free_udp_port() -> Result<u16> {
    let socket = UdpSocket::bind("127.0.0.1:0").context("no free UDP port")?;
    Ok(socket.local_addr().context("no free UDP port")?.port())
}

pub(super) fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}
