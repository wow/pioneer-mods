//! The local emulator (QEMU, run from the owner's own build; never part of this repository),
//! started headless for one test. Panel controls and touches go to its panel key socket (UDP on
//! 127.0.0.1); memory reads (`xp`) and frames (`screendump`) go through its monitor (TCP on
//! 127.0.0.1). The flash image, frames and the emulator's log stay in the run's directory.

use super::{Emulator, Frame, parse_ppm};
use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream, UdpSocket};
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
}

/// A running emulator. Dropping it quits QEMU, or kills it.
pub struct Qemu {
    child: Child,
    monitor: TcpStream,
    keys: UdpSocket,
    key_port: u16,
    work: PathBuf,
}

/// How long the monitor may take to come up, or to answer one command.
const MONITOR_TIMEOUT: Duration = Duration::from_secs(60);

impl Qemu {
    /// Starts the emulator and connects to its monitor.
    ///
    /// # Errors
    ///
    /// The flash image cannot be written, the emulator does not start, or its monitor does not
    /// answer in time.
    pub fn start(setup: QemuSetup) -> Result<Self> {
        let flash = setup.work.join("flash.bin");
        std::fs::write(&flash, &setup.flash)
            .with_context(|| format!("failed to write '{}'", flash.display()))?;
        let key_port = free_udp_port()?;
        let monitor_port = free_tcp_port()?;
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
            .arg(format!("tcp:127.0.0.1:{monitor_port},server,nowait"))
            .arg("-drive")
            .arg(format!("if=pflash,format=raw,file={}", flash.display()));
        if let Some(stick) = &setup.stick {
            command
                .arg("-drive")
                .arg(format!(
                    "if=none,id=usbstick,format=raw,file={},snapshot=on",
                    stick.display()
                ))
                .args(["-device", "usb-storage,drive=usbstick,port=1"]);
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
        let monitor = match connect_monitor(&mut child, monitor_port) {
            Ok(monitor) => monitor,
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error);
            }
        };
        let keys = UdpSocket::bind("127.0.0.1:0").context("failed to open the key socket")?;
        let mut qemu = Qemu {
            child,
            monitor,
            keys,
            key_port,
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
            if Instant::now() > deadline {
                bail!("the emulator's monitor did not answer in time");
            }
            match self.monitor.read(&mut chunk) {
                Ok(0) => bail!("the emulator's monitor closed"),
                Ok(n) => output.extend_from_slice(&chunk[..n]),
                Err(error) if is_timeout(&error) => {}
                Err(error) => return Err(error).context("failed to read the emulator's monitor"),
            }
        }
        Ok(String::from_utf8_lossy(&output).into_owned())
    }

    fn send_key(&mut self, message: &str) -> Result<()> {
        self.keys
            .send_to(message.as_bytes(), ("127.0.0.1", self.key_port))
            .context("failed to send to the emulator's panel")?;
        Ok(())
    }
}

impl Emulator for Qemu {
    fn press(&mut self, code: u8, value: u8, ms: u32) -> Result<()> {
        self.send_key(&format!("{code:#04x}:{value:#04x}:{ms}"))?;
        self.wait(ms)
    }

    fn touch(&mut self, x: u32, y: u32, ms: u32) -> Result<()> {
        self.send_key(&format!("{x}:{y}:{ms}:tap"))?;
        self.wait(ms)
    }

    fn wait(&mut self, ms: u32) -> Result<()> {
        std::thread::sleep(Duration::from_millis(u64::from(ms)));
        if let Some(status) = self
            .child
            .try_wait()
            .context("failed to poll the emulator")?
        {
            bail!("the emulator exited ({status})");
        }
        Ok(())
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
        let _ = std::fs::remove_file(&path);
        self.command(&format!("screendump {}", path.display()))?;
        let bytes = std::fs::read(&path).context("the emulator wrote no frame")?;
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

fn connect_monitor(child: &mut Child, port: u16) -> Result<TcpStream> {
    let deadline = Instant::now() + MONITOR_TIMEOUT;
    loop {
        if let Some(status) = child.try_wait().context("failed to poll the emulator")? {
            bail!("the emulator exited before its monitor came up ({status}); see its log");
        }
        if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
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

fn free_udp_port() -> Result<u16> {
    let socket = UdpSocket::bind("127.0.0.1:0").context("no free UDP port")?;
    Ok(socket.local_addr().context("no free UDP port")?.port())
}

fn free_tcp_port() -> Result<u16> {
    let listener = TcpListener::bind("127.0.0.1:0").context("no free TCP port")?;
    Ok(listener.local_addr().context("no free TCP port")?.port())
}

fn is_timeout(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
    )
}
