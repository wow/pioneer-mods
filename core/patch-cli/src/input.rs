//! Reading an owner-supplied official update without trusting its size.

use anyhow::{Context, Result, bail};
use patch_core::open_regular_file;
use std::io::Read;
use std::path::Path;

/// Reads `path` only if its length is `expected_len`, checked on the open handle, so an arbitrary
/// large file is refused without being read. The caller (or the library) checks the hash.
/// `what` names the expected file and `verb` the refused action, for the refusal message.
pub fn read_pinned_input(
    path: &Path,
    expected_len: usize,
    what: &str,
    verb: &str,
) -> Result<Vec<u8>> {
    let read_failed = || format!("failed to read input update '{}'", path.display());
    let file = open_regular_file(path).with_context(read_failed)?;
    let len = file.metadata().with_context(read_failed)?.len();
    if len != expected_len as u64 {
        bail!(
            "refusing to {verb} '{}': it is not {what} ({len} bytes, expected {expected_len}); \
             only that exact file is accepted",
            path.display(),
        );
    }
    let mut bytes = Vec::with_capacity(expected_len);
    // One byte more than expected, so a file that grew after the check is still caught.
    file.take(len + 1)
        .read_to_end(&mut bytes)
        .with_context(read_failed)?;
    Ok(bytes)
}
