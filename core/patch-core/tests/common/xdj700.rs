//! Synthetic XDJ-700 updates for rebuild tests: a sparse loader region, an application section
//! at `0x40000` produced by our own encoder, and a PANL document. No vendor bytes.

use super::{container, document_with_descriptor, panel_lines, record};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, RebuiltUpdate, StockRelease, encode_section,
    rebuild_with_application, verify_rebuild,
};
use patch_core::{RebuildError, sha256_hex};

/// Loader extents written by the synthetic MAIN document. The last extent (the application)
/// starts at the section offset and runs to the image end.
pub const LOADER_EXTENTS: [(usize, usize); 2] = [(0x000, 0x040), (0x100, 0x200)];

/// A decoded application: the 19-byte zero seed followed by `len` pattern bytes.
pub fn application(len: usize, salt: u8) -> Vec<u8> {
    let mut decoded = vec![0; 19];
    decoded.extend((0..len).map(|i| (i as u8 / 3).wrapping_mul(salt) ^ (i >> 7) as u8));
    decoded
}

/// The loader region `[0, 0x40000)`: pattern bytes inside the extents, `0xFF` elsewhere.
pub fn loader() -> Vec<u8> {
    let mut image = vec![0xFF; APPLICATION_SECTION_OFFSET];
    for (start, end) in LOADER_EXTENTS {
        for (offset, byte) in image[start..end].iter_mut().enumerate() {
            *byte = (start + offset) as u8 ^ 0x5A;
        }
    }
    image
}

/// Loader plus the encoded section, optionally padded with `0xFF` to a 32-byte boundary as in
/// the official update.
pub fn main_image(decoded: &[u8], pad: bool) -> Vec<u8> {
    let mut image = loader();
    image.extend(encode_section(decoded).expect("encode"));
    if pad {
        image.resize(image.len().next_multiple_of(32), 0xFF);
    }
    image
}

pub struct Main<'a> {
    pub image: &'a [u8],
    pub version: &'a str,
    pub record_len: usize,
    pub header: &'a [u8],
    pub entry: u32,
}

impl<'a> Main<'a> {
    pub fn new(image: &'a [u8], version: &'a str) -> Self {
        Self {
            image,
            version,
            record_len: 32,
            header: b"romobj  mot",
            entry: 0xA000_0000,
        }
    }

    pub fn document(&self) -> Vec<u8> {
        let mut lines = vec![record(b'0', 2, 0, self.header)];
        let extents = LOADER_EXTENTS
            .into_iter()
            .chain([(APPLICATION_SECTION_OFFSET, self.image.len())]);
        for (start, end) in extents {
            for address in (start..end).step_by(self.record_len) {
                let stop = (address + self.record_len).min(end);
                lines.push(record(b'2', 3, address as u32, &self.image[address..stop]));
            }
        }
        lines.push(record(b'7', 4, self.entry, &[]));
        document_with_descriptor(&descriptor("MAIN", self.version), &lines)
    }

    /// `[MAIN, PANL]`, as in the official update.
    pub fn update(&self) -> Vec<u8> {
        container(&[self.document(), panel()])
    }
}

pub fn descriptor(kind: &str, version: &str) -> Vec<u8> {
    let mut bytes = format!("XDJ-700     {kind}{version}").into_bytes();
    bytes.extend_from_slice(if kind == "MAIN" {
        b"\0       0"
    } else {
        b"         "
    });
    bytes
}

pub fn panel() -> Vec<u8> {
    document_with_descriptor(&descriptor("PANL", "Ver1.00"), &panel_lines())
}

/// A stock update (`Ver1.15`, padded) around `decoded`.
pub fn stock_update(decoded: &[u8]) -> Vec<u8> {
    Main::new(&main_image(decoded, true), "Ver1.15").update()
}

/// An application whose section does not end on a 32-byte boundary, so its stock image needs
/// padding (as the official v1.15 image does).
pub fn padded_application(salt: u8) -> Vec<u8> {
    (3000..3100)
        .map(|len| application(len, salt))
        .find(|decoded| {
            !encode_section(decoded)
                .expect("encode")
                .len()
                .is_multiple_of(32)
        })
        .expect("some length needs padding")
}

/// The record lines of a document built by these helpers (descriptor and CRC removed).
pub fn lines(document: &[u8]) -> Vec<Vec<u8>> {
    document[32..document.len() - 2]
        .split_inclusive(|&byte| byte == b'\n')
        .map(<[u8]>::to_vec)
        .collect()
}

/// A decoded application whose `len` bytes after the zero seed do not compress, so its section
/// grows by about 1/8.
pub fn incompressible_application(len: usize) -> Vec<u8> {
    let mut state = 0x2545_F491_4F6C_DD1D_u64;
    let mut decoded = vec![0; 19];
    decoded.extend((0..len).map(|_| {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        state as u8
    }));
    decoded
}

/// A second panel-like document, so tests can tell documents apart.
pub fn other_document() -> Vec<u8> {
    let lines = [
        record(b'0', 2, 0, &[]),
        record(b'2', 3, 0x0D0000, &[0x46; 5]),
        record(b'8', 3, 0, &[]),
    ];
    document_with_descriptor(&descriptor("PANL", "Ver1.01"), &lines)
}

/// Size bound for synthetic releases: the loader region plus 64 KiB.
pub const SYNTHETIC_MAX_MAIN_IMAGE_LEN: usize = APPLICATION_SECTION_OFFSET + 64 * 1024;

/// A synthetic release pinned to the stock file whose SHA-256 is `sha256`.
pub fn release(sha256: &str) -> StockRelease<'_> {
    StockRelease {
        upd_sha256: sha256,
        max_main_image_len: SYNTHETIC_MAX_MAIN_IMAGE_LEN,
    }
}

/// Rebuilds `stock`, pinned as its own synthetic release.
pub fn rebuild(stock: &[u8], decoded: &[u8], label: &str) -> Result<RebuiltUpdate, RebuildError> {
    let sha256 = sha256_hex(stock);
    rebuild_with_application(stock, &release(&sha256), decoded, label)
}

/// Verifies `output` against `stock`, pinned as its own synthetic release.
pub fn verify(
    stock: &[u8],
    output: &[u8],
    decoded: &[u8],
    label: &str,
) -> Result<(), RebuildError> {
    let sha256 = sha256_hex(stock);
    verify_rebuild(stock, &release(&sha256), output, decoded, label)
}
