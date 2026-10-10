//! The last check of a composed build ([`check_composed`]): its application against each
//! fragment's own output. Outputs are reduced to the bytes they change, so a composition holds
//! one decoded output at a time.

use super::compose_error::ComposeError;
use std::collections::BTreeMap;
use std::ops::Range;

/// Bytes compared at once before a chunk is scanned byte by byte.
const CHUNK: usize = 4096;

/// Checks that the decoded application `composed` equals, at every byte a fragment changes, that
/// fragment's own decoded output, and `stock` everywhere else, outside `version` (the version
/// string, which the composition sets). `outputs` pairs each fragment's name with its output.
///
/// # Errors
///
/// [`ComposeError::Disagree`] when two outputs change one byte differently, or
/// [`ComposeError::Mismatch`] for a wrong length, or at a byte `composed` gets wrong (outputs in
/// order, then the bytes no output changes).
pub fn check_composed(
    stock: &[u8],
    outputs: &[(&str, &[u8])],
    composed: &[u8],
    version: Range<usize>,
) -> Result<(), ComposeError> {
    let mut changes = Vec::with_capacity(outputs.len());
    for &(name, output) in outputs {
        changes.push((name, changed_bytes(stock, output, &version)?));
    }
    let named: Vec<(&str, &[(usize, u8)])> = changes
        .iter()
        .map(|(name, changed)| (*name, changed.as_slice()))
        .collect();
    check_changes(stock, &named, composed, &version)
}

/// The offset and new value of every byte `output` changes from `stock`, outside `version`.
pub(super) fn changed_bytes(
    stock: &[u8],
    output: &[u8],
    version: &Range<usize>,
) -> Result<Vec<(usize, u8)>, ComposeError> {
    same_length(stock, output)?;
    Ok(differing(stock, output)
        .filter(|offset| !version.contains(offset))
        .map(|offset| (offset, output[offset]))
        .collect())
}

/// [`check_composed`] with each fragment's output reduced to [`changed_bytes`].
pub(super) fn check_changes(
    stock: &[u8],
    changes: &[(&str, &[(usize, u8)])],
    composed: &[u8],
    version: &Range<usize>,
) -> Result<(), ComposeError> {
    same_length(stock, composed)?;
    // Each changed byte, with the first fragment that changes it and its value there.
    let mut changed: BTreeMap<usize, (&str, u8)> = BTreeMap::new();
    for &(name, bytes) in changes {
        for &(offset, byte) in bytes {
            if let Some(&(first, first_byte)) = changed.get(&offset)
                && first_byte != byte
            {
                return Err(ComposeError::Disagree {
                    offset,
                    first: first.to_owned(),
                    second: name.to_owned(),
                });
            }
            if composed[offset] != byte {
                return Err(ComposeError::Mismatch {
                    offset,
                    expected: format!("{name}'s output"),
                });
            }
            changed.entry(offset).or_insert((name, byte));
        }
    }
    let unexplained = differing(stock, composed)
        .filter(|offset| !version.contains(offset))
        .find(|offset| !changed.contains_key(offset));
    match unexplained {
        Some(offset) => Err(ComposeError::Mismatch {
            offset,
            expected: "stock".to_owned(),
        }),
        None => Ok(()),
    }
}

fn same_length(stock: &[u8], other: &[u8]) -> Result<(), ComposeError> {
    if other.len() == stock.len() {
        Ok(())
    } else {
        Err(ComposeError::Mismatch {
            offset: other.len().min(stock.len()),
            expected: "stock (the application's length)".to_owned(),
        })
    }
}

/// The offsets at which `a` and `b` (of one length) differ, in order. Equal chunks are skipped
/// whole.
fn differing<'a>(a: &'a [u8], b: &'a [u8]) -> impl Iterator<Item = usize> + 'a {
    a.chunks(CHUNK)
        .zip(b.chunks(CHUNK))
        .enumerate()
        .filter(|(_, (x, y))| x != y)
        .flat_map(|(chunk, (x, y))| {
            (0..x.len())
                .filter(move |&at| x[at] != y[at])
                .map(move |at| chunk * CHUNK + at)
        })
}
