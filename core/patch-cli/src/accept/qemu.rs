//! The local emulator (QEMU, run from the owner's own build; never part of this repository),
//! started headless for one test. Panel controls and touches go to its panel key socket (UDP on
//! 127.0.0.1); memory reads (`xp`) and frames (`screendump`) go through its monitor, a Unix socket
//! in the run's private directory, so no other user can reach it (the monitor can read memory and
//! run programs as the owner). The flash image, frames and the emulator's log stay in
//! the run's directory. Unix hosts only.
//!
//! The emulator binds its key socket on its first panel frame, well into boot, and logs it; the
//! runner waits for that line before the first press or touch, and refuses a run whose emulator
//! could not bind it. Holds are timed on the emulator's virtual clock, which runs slower than the
//! host's, so the runner waits a hold out for `time_factor` times its length. The emulator runs
//! with none of the owner's `CDJ_*` variables but the profile's.
//!
//! The monitor is driven as text (HMP): the prompt, the echo and the `xp` format are QEMU's. QMP
//! would give structured replies; it is a possible later step.

use super::host::{
    check_interrupted, free_udp_port, hmp_string, hold_host_ms, is_timeout, keys_bound,
    option_value,
};
use super::{Emulator, Frame, parse_ppm, parse_xp};
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::net::UdpSocket;
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

/// How a player's update runs in the emulator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Profile {
    /// QEMU's machine (`-M`).
    pub machine: &'static str,
    /// The NOR flash size, in bytes.
    pub flash_size: usize,
    /// Environment the machine needs.
    pub env: &'static [(&'static str, &'static str)],
}

/// How long the emulator may take to bind its key socket (its first panel frame).
const KEYS_TIMEOUT: Duration = Duration::from_secs(300);

/// Waited after a hold has been waited out, for the emulator to act on the release.
pub const SETTLE_MS: u32 = 100;

/// The emulator profile of a player, if the runner knows one.
pub fn profile(player: &str) -> Option<Profile> {
    match player {
        "xdj700-v1.15" => Some(Profile {
            machine: "xdj700",
            flash_size: 16 * 1024 * 1024,
            env: &[("CDJ_ATA", "1"), ("CDJ_REPORT", "0")],
        }),
        _ => None,
    }
}

/// What to start.
#[derive(Debug, Clone)]
pub struct QemuSetup {
    /// The emulator binary (`qemu-system-sh4` of the owner's build).
    pub emulator: PathBuf,
    pub profile: Profile,
    /// The run's directory: the flash image, frames and the emulator's log go here.
    pub work: PathBuf,
    /// The flash image ([`flash_image`](super::flash_image)).
    pub flash: Vec<u8>,
    /// The owner's USB stick image, plugged in from the start. The emulator's writes go to a
    /// throwaway overlay (`snapshot=on`); the image itself is not changed.
    pub stick: Option<PathBuf>,
    /// How many times slower than the host the emulator runs at most: a hold of `ms` is waited
    /// out for `ms * time_factor` host milliseconds, then [`SETTLE_MS`].
    pub time_factor: u32,
}

/// A running emulator. Dropping it quits QEMU, or kills it.
pub struct Qemu {
    child: Child,
    monitor: UnixStream,
    keys: UdpSocket,
    key_port: u16,
    keys_live: bool,
    time_factor: u32,
    work: PathBuf,
}

/// Longest Unix socket path every supported host binds (macOS allows 103 bytes, Linux 107).
pub const MAX_SOCKET_PATH: usize = 100;

/// How long the monitor may take to come up, or to answer one command.
const MONITOR_TIMEOUT: Duration = Duration::from_secs(60);

impl Qemu {
    /// Starts the emulator and connects to its monitor.
    ///
    /// # Errors
    ///
    /// A run directory others may enter, a flash image that cannot be written, an emulator that
    /// does not start, or a monitor that does not answer in time.
    pub fn start(setup: QemuSetup) -> Result<Self> {
        use std::os::unix::fs::PermissionsExt;
        if setup.time_factor == 0 {
            bail!("the time factor must be at least 1");
        }
        // A Ctrl-C while the update was read and checked: write nothing, start nothing.
        check_interrupted()?;
        // The monitor socket goes here: no other user may enter the directory.
        let mode = std::fs::metadata(&setup.work)
            .with_context(|| format!("failed to read '{}'", setup.work.display()))?
            .permissions()
            .mode();
        if mode & 0o077 != 0 {
            bail!(
                "refusing run directory '{}': others may enter it (mode {:o}); it holds the \
                 emulator's monitor",
                setup.work.display(),
                mode & 0o777
            );
        }
        let flash = setup.work.join("flash.bin");
        std::fs::write(&flash, &setup.flash)
            .with_context(|| format!("failed to write '{}'", flash.display()))?;
        // Everything that can fail goes before the spawn, so a started emulator is always reaped.
        // The runner's own socket first, so that it cannot be given the emulator's port.
        let keys = UdpSocket::bind("127.0.0.1:0").context("failed to open the key socket")?;
        let key_port = free_udp_port()?;
        // In the run's directory, which only the owner can enter (created mode 0700).
        let monitor_path = setup.work.join("monitor.sock");
        if monitor_path.as_os_str().len() > MAX_SOCKET_PATH {
            bail!(
                "refusing run directory '{}': the monitor's socket path would be longer than {} \
                 bytes, which Unix sockets cannot bind; set TMPDIR to a shorter directory",
                setup.work.display(),
                MAX_SOCKET_PATH
            );
        }
        let log = std::fs::File::create(setup.work.join("emulator.log"))
            .context("failed to create the emulator's log")?;
        let mut command = Command::new(&setup.emulator);
        command
            .args([
                "-M",
                setup.profile.machine,
                "-display",
                "none",
                "-serial",
                "none",
            ])
            .arg("-monitor")
            .arg(format!(
                "unix:{},server,nowait",
                option_value(&monitor_path)?
            ))
            .arg("-drive")
            .arg(format!(
                "if=pflash,format=raw,file={}",
                option_value(&flash)?
            ));
        if let Some(stick) = &setup.stick {
            command
                .arg("-drive")
                .arg(format!(
                    "if=none,id=usbstick,format=raw,file={},snapshot=on",
                    option_value(stick)?
                ))
                .args(["-device", "usb-storage,drive=usbstick,port=1"]);
        }
        // None of the owner's emulator settings: they change what a run does.
        for (name, _) in std::env::vars_os() {
            if name.to_string_lossy().starts_with("CDJ_") {
                command.env_remove(name);
            }
        }
        command
            .envs(setup.profile.env.iter().copied())
            .env("CDJ_PANEL_KEYSOCK", key_port.to_string())
            .stdin(Stdio::null())
            .stdout(
                log.try_clone()
                    .context("failed to share the emulator's log")?,
            )
            .stderr(log);
        let mut child = command
            .spawn()
            .with_context(|| format!("failed to start '{}'", setup.emulator.display()))?;
        let monitor = match connect_monitor(&mut child, &monitor_path) {
            Ok(monitor) => monitor,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let mut qemu = Qemu {
            child,
            monitor,
            keys,
            key_port,
            keys_live: false,
            time_factor: setup.time_factor,
            work: setup.work,
        };
        qemu.until_prompt()?;
        Ok(qemu)
    }

    /// Sends one monitor command and returns its output, up to the next prompt.
    fn command(&mut self, line: &str) -> Result<String> {
        self.monitor
            .write_all(format!("{line}\n").as_bytes())
            .context("the emulator's monitor closed")?;
        self.until_prompt()
    }

    fn until_prompt(&mut self) -> Result<String> {
        let deadline = Instant::now() + MONITOR_TIMEOUT;
        let mut output = Vec::new();
        let mut chunk = [0u8; 4096];
        while !output.ends_with(b"(qemu) ") {
            check_interrupted()?;
            if Instant::now() > deadline {
                bail!("the emulator's monitor did not answer in time");
            }
            match self.monitor.read(&mut chunk) {
                Ok(0) => {
                    // Ctrl-C reaches the emulator too, which may quit first: say why.
                    check_interrupted()?;
                    bail!("the emulator's monitor closed");
                }
                Ok(n) => output.extend_from_slice(&chunk[..n]),
                Err(error) if is_timeout(&error) => {}
                Err(error) => return Err(error).context("failed to read the emulator's monitor"),
            }
        }
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    /// Waits until the emulator has bound its key socket (it logs the line), once.
    fn await_keys(&mut self) -> Result<()> {
        if self.keys_live {
            return Ok(());
        }
        let deadline = Instant::now() + KEYS_TIMEOUT;
        loop {
            let log = std::fs::read_to_string(self.work.join("emulator.log")).unwrap_or_default();
            if keys_bound(&log, self.key_port)? {
                self.keys_live = true;
                return Ok(());
            }
            if Instant::now() > deadline {
                bail!("the emulator did not bind its key socket in time (its first panel frame)");
            }
            self.pause(200)?;
        }
    }

    /// Sleeps `ms` host milliseconds in short slices, ending early on an interrupt or if the
    /// emulator exits.
    fn pause(&mut self, ms: u64) -> Result<()> {
        let deadline = Instant::now() + Duration::from_millis(ms);
        loop {
            check_interrupted()?;
            if let Some(status) = self
                .child
                .try_wait()
                .context("failed to poll the emulator")?
            {
                bail!("the emulator exited ({status})");
            }
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return Ok(());
            }
            std::thread::sleep(left.min(Duration::from_millis(100)));
        }
    }

    /// Waits out a hold of `ms` emulated milliseconds.
    fn hold(&mut self, ms: u32) -> Result<()> {
        self.pause(hold_host_ms(ms, self.time_factor, SETTLE_MS))
    }

    fn send_key(&mut self, message: &str) -> Result<()> {
        self.await_keys()?;
        self.keys
            .send_to(message.as_bytes(), ("127.0.0.1", self.key_port))
            .context("failed to send to the emulator's panel")?;
        Ok(())
    }
}

impl Emulator for Qemu {
    fn press(&mut self, code: u8, value: u8, ms: u32) -> Result<()> {
        self.send_key(&format!("{code:#04x}:{value:#04x}:{ms}"))?;
        self.hold(ms)
    }

    fn touch(&mut self, x: u32, y: u32, ms: u32) -> Result<()> {
        self.send_key(&format!("{x}:{y}:{ms}:tap"))?;
        self.hold(ms)
    }

    fn wait(&mut self, ms: u32) -> Result<()> {
        self.pause(u64::from(ms))
    }

    fn read(&mut self, address: u32, bytes: u8) -> Result<u32> {
        let unit = match bytes {
            1 => 'b',
            2 => 'h',
            4 => 'w',
            _ => bail!("cannot read {bytes} bytes"),
        };
        let output = self.command(&format!("xp /1{unit}x {address:#x}"))?;
        parse_xp(&output, address)
    }

    fn frame(&mut self) -> Result<Frame> {
        let path = self.work.join("frame.ppm");
        let quoted = hmp_string(&path)?;
        let _ = std::fs::remove_file(&path);
        let reply = self.command(&format!("screendump {quoted}"))?;
        let bytes = std::fs::read(&path).with_context(|| {
            let reply = reply.replace("(qemu) ", "");
            format!("the emulator wrote no frame; it replied: {}", reply.trim())
        })?;
        std::fs::remove_file(&path).context("failed to remove the frame")?;
        parse_ppm(&bytes)
    }
}

impl Drop for Qemu {
    fn drop(&mut self) {
        let _ = self.monitor.write_all(b"quit\n");
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if matches!(self.child.try_wait(), Ok(Some(_))) {
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn connect_monitor(child: &mut Child, path: &std::path::Path) -> Result<UnixStream> {
    let deadline = Instant::now() + MONITOR_TIMEOUT;
    loop {
        check_interrupted()?;
        if let Some(status) = child.try_wait().context("failed to poll the emulator")? {
            // Ctrl-C reaches the emulator too, which may exit first: say why.
            check_interrupted()?;
            bail!("the emulator exited before its monitor came up ({status})");
        }
        if let Ok(stream) = UnixStream::connect(path) {
            stream
                .set_read_timeout(Some(Duration::from_millis(200)))
                .context("failed to set the monitor's timeout")?;
            return Ok(stream);
        }
        if Instant::now() > deadline {
            bail!("the emulator's monitor did not come up in time");
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
