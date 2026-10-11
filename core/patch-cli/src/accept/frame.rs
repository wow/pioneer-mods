//! A frame of the emulated screen, as the emulator's `screendump` writes it (binary PPM, 8 bits a
//! channel; QEMU expands the RGB565 framebuffer by bit replication, as
//! [`expand_rgb565`](patch_schema::catalog::expand_rgb565) does), and the brightness of its
//! regions. A frame stays in memory and in the run's temporary directory; only the brightness
//! numbers are reported.

use anyhow::{Result, bail};
use patch_schema::catalog::{Region, mean_brightness};

/// An RGB frame, 8 bits a channel, rows from the top.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    /// `width * height` pixels, row by row.
    pub pixels: Vec<(u8, u8, u8)>,
}

impl Frame {
    /// The mean brightness of `region`, which must lie in the frame.
    ///
    /// # Errors
    ///
    /// A region that is empty or not in the frame.
    pub fn brightness(&self, region: &Region) -> Result<u8> {
        if !region.fits(self.width, self.height) {
            bail!(
                "region ({}, {}, {}x{}) is not in the {}x{} frame",
                region.x,
                region.y,
                region.w,
                region.h,
                self.width,
                self.height
            );
        }
        let rows = region.y..region.y + region.h;
        let width = self.width as usize;
        Ok(mean_brightness(rows.flat_map(|y| {
            let start = y as usize * width + region.x as usize;
            self.pixels[start..start + region.w as usize]
                .iter()
                .copied()
        })))
    }
}

/// Parses a binary PPM (`P6`, maximum value 255), with the header's optional comments.
///
/// # Errors
///
/// Anything else, or a pixel count that does not match the header.
pub fn parse_ppm(bytes: &[u8]) -> Result<Frame> {
    let mut at = 0;
    let mut fields = Vec::new();
    while fields.len() < 4 {
        while at < bytes.len() && bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if at < bytes.len() && bytes[at] == b'#' {
            while at < bytes.len() && bytes[at] != b'\n' {
                at += 1;
            }
            continue;
        }
        let start = at;
        while at < bytes.len() && !bytes[at].is_ascii_whitespace() {
            at += 1;
        }
        if start == at {
            bail!("the frame is not a binary PPM: its header ends early");
        }
        fields.push(&bytes[start..at]);
    }
    // One whitespace byte separates the header from the pixels.
    at += 1;
    let number = |field: &[u8]| -> Result<u32> {
        std::str::from_utf8(field)
            .ok()
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| anyhow::anyhow!("the frame's PPM header holds a bad number"))
    };
    if fields[0] != b"P6" {
        bail!("the frame is not a binary PPM (P6)");
    }
    let (width, height, max) = (number(fields[1])?, number(fields[2])?, number(fields[3])?);
    if max != 255 {
        bail!("the frame's PPM has a maximum value of {max}, not 255");
    }
    let count = width as usize * height as usize;
    let data = bytes.get(at..).unwrap_or_default();
    if data.len() != count * 3 {
        bail!(
            "the frame's PPM holds {} bytes of pixels for {width}x{height}",
            data.len()
        );
    }
    let pixels = data
        .as_chunks::<3>()
        .0
        .iter()
        .map(|&[r, g, b]| (r, g, b))
        .collect();
    Ok(Frame {
        width,
        height,
        pixels,
    })
}
