//! A replacement's `bytes_hex` byte by byte: the bytes it writes and the stock bytes it keeps
//! (`--`), which are not published. Kept bytes are for changes a few bytes apart, such as the
//! fields of one table. A span writes its first and last bytes (a kept byte at an edge belongs
//! outside the span, in its window) and keeps fewer than [`MIN_PRECONDITION_LEN`] bytes in a row,
//! the limit for written stock bytes too: a longer stretch belongs between two replacements.
//! [`RecipeV2::validate`](crate::RecipeV2::validate) checks both; [`Replacement::pattern`] only
//! parses.

use crate::v2::{MIN_PRECONDITION_LEN, RecipeV2Error, Replacement};

impl Replacement {
    /// The span byte by byte: `Some(byte)` to write, `None` where `bytes_hex` holds `--` and the
    /// stock byte is kept. `None` unless `bytes_hex` is non-empty, has an even length and holds
    /// pairs of hex digits or `--`; where kept bytes may be is checked by `validate`, not here.
    pub fn pattern(&self) -> Option<Vec<Option<u8>>> {
        let hex = &self.bytes_hex;
        if hex.is_empty() || !hex.len().is_multiple_of(2) {
            return None;
        }
        hex.as_bytes()
            .chunks(2)
            .map(|pair| {
                if pair == b"--" {
                    return Some(None);
                }
                let digit = |byte: u8| (byte as char).to_digit(16);
                Some(Some((digit(pair[0])? * 16 + digit(pair[1])?) as u8))
            })
            .collect()
    }

    /// The runs of bytes the span writes, in order: each run's position in the span and its
    /// bytes. Kept bytes separate the runs. `None` if `bytes_hex` does not parse
    /// ([`Self::pattern`]).
    pub fn written_runs(&self) -> Option<Vec<(usize, Vec<u8>)>> {
        let pattern = self.pattern()?;
        let mut runs = Vec::new();
        let mut at = 0;
        for chunk in pattern.chunk_by(|a, b| a.is_some() == b.is_some()) {
            // A chunk is all written or all kept; a kept one collects to `None`.
            if let Some(bytes) = chunk.iter().copied().collect::<Option<Vec<u8>>>() {
                runs.push((at, bytes));
            }
            at += chunk.len();
        }
        Some(runs)
    }
}

/// Where replacement `index`, whose `bytes_hex` parses to `pattern`, may keep stock bytes: not at
/// its edges, and fewer than [`MIN_PRECONDITION_LEN`] in a row.
pub(crate) fn check_kept(index: usize, pattern: &[Option<u8>]) -> Result<(), RecipeV2Error> {
    let written = |byte: Option<&Option<u8>>| byte.is_some_and(Option::is_some);
    if !written(pattern.first()) || !written(pattern.last()) {
        return Err(RecipeV2Error::KeptSpanEdge { index });
    }
    let longest = pattern
        .chunk_by(|a, b| a.is_some() == b.is_some())
        .filter(|chunk| chunk[0].is_none())
        .map(<[_]>::len)
        .max()
        .unwrap_or(0);
    if longest >= MIN_PRECONDITION_LEN as usize {
        return Err(RecipeV2Error::LongKeptRun {
            index,
            len: longest,
        });
    }
    Ok(())
}
