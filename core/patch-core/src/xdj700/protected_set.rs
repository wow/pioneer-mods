//! The protected set: code that runs at start-up or in the update path, measured in emulation and
//! kept outside this repository (`docs/xdj700-flashing.md`, section 5). A recipe whose span or
//! precondition window overlaps it is refused, so the rule is checked by the tool and not only by
//! hand.
//!
//! The set is a text file of run-time address ranges, one per line:
//!
//! ```text
//! # comments and blank lines are ignored; the first other line may be a header
//! start   end     bytes
//! 08000600        08000605        6
//! 0x08000610      0x08000649
//! ```
//!
//! `start` and `end` are hexadecimal (an `0x` prefix is optional) and `end` is the last address of
//! the range, inclusive. An optional third column counts the bytes; it must equal
//! `end - start + 1`, which catches a file written with exclusive ends. A run-time address is the
//! target's [`RecipeTarget::load_address`] plus a decoded-application offset, and every range must
//! lie inside the application. Ranges may overlap and need not be sorted. An empty set is refused,
//! so a wrong file cannot pass silently.

use super::recipe::{RecipeError, RecipeTarget};
use patch_schema::RecipeV2;
use std::ops::Range;
use thiserror::Error;

/// Why a protected-set file was refused. Line numbers count from 1.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtectedSetError {
    #[error("line {line}: expected `start end [bytes]`, found {fields} fields")]
    Fields { line: usize, fields: usize },

    #[error("line {line}: {field} {value:?} is not a hexadecimal address")]
    NotHex {
        line: usize,
        field: &'static str,
        value: String,
    },

    #[error("line {line}: bytes {value:?} is not a decimal count")]
    NotCount { line: usize, value: String },

    #[error("line {line}: end {end:#x} is below start {start:#x}")]
    Reversed { line: usize, start: u64, end: u64 },

    #[error(
        "line {line}: bytes is {count}, but {start:#x}..={end:#x} holds {expected} (`end` is the \
         last address, inclusive)"
    )]
    ByteCount {
        line: usize,
        start: u64,
        end: u64,
        count: u64,
        expected: u64,
    },

    #[error(
        "line {line}: {start:#x}..={end:#x} is not inside the application, run-time \
         {app_start:#x}..={app_end:#x} (the set uses run-time addresses: the load address plus \
         a decoded offset)"
    )]
    OutsideApplication {
        line: usize,
        start: u64,
        end: u64,
        app_start: u64,
        app_end: u64,
    },

    #[error("the protected set holds no ranges")]
    Empty,
}

/// A parsed protected set, as half-open decoded-application ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedSet {
    ranges: Vec<Range<usize>>,
}

impl ProtectedSet {
    /// Parses `text` (the format in the module docs) for `target`.
    ///
    /// # Errors
    ///
    /// A [`ProtectedSetError`] naming the first bad line, or [`ProtectedSetError::Empty`].
    pub fn parse(text: &str, target: &RecipeTarget<'_>) -> Result<Self, ProtectedSetError> {
        let app_start = target.load_address;
        // Targets are pinned constants with a non-empty application; saturate anyway.
        let app_end = app_start.saturating_add((target.application_len as u64).saturating_sub(1));
        let mut ranges = Vec::new();
        let mut header_allowed = true;
        for (index, raw_line) in text.lines().enumerate() {
            let line = index + 1;
            let content = raw_line.trim();
            if content.is_empty() || content.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = content.split_whitespace().collect();
            if std::mem::take(&mut header_allowed) && fields.first() == Some(&"start") {
                continue;
            }
            if !(2..=3).contains(&fields.len()) {
                return Err(ProtectedSetError::Fields {
                    line,
                    fields: fields.len(),
                });
            }
            let start = parse_hex(line, "start", fields[0])?;
            let end = parse_hex(line, "end", fields[1])?;
            if end < start {
                return Err(ProtectedSetError::Reversed { line, start, end });
            }
            let expected = end - start + 1;
            if let Some(value) = fields.get(2) {
                let count = value
                    .parse::<u64>()
                    .map_err(|_| ProtectedSetError::NotCount {
                        line,
                        value: (*value).to_owned(),
                    })?;
                if count != expected {
                    return Err(ProtectedSetError::ByteCount {
                        line,
                        start,
                        end,
                        count,
                        expected,
                    });
                }
            }
            if start < app_start || end > app_end {
                return Err(ProtectedSetError::OutsideApplication {
                    line,
                    start,
                    end,
                    app_start,
                    app_end,
                });
            }
            // Inside the application, so both fit in usize.
            let offset = |address: u64| (address - app_start) as usize;
            ranges.push(offset(start)..offset(end) + 1);
        }
        if ranges.is_empty() {
            return Err(ProtectedSetError::Empty);
        }
        Ok(Self { ranges })
    }

    /// The number of ranges.
    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    /// Always false: an empty set is refused when it is parsed.
    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    /// The ranges as half-open decoded-application offsets, in file order.
    pub fn ranges(&self) -> &[Range<usize>] {
        &self.ranges
    }
}

fn parse_hex(line: usize, field: &'static str, value: &str) -> Result<u64, ProtectedSetError> {
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    // `from_str_radix` accepts a leading `+`, which is not an address.
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(ProtectedSetError::NotHex {
            line,
            field,
            value: value.to_owned(),
        });
    }
    u64::from_str_radix(digits, 16).map_err(|_| ProtectedSetError::NotHex {
        line,
        field,
        value: value.to_owned(),
    })
}

/// Refuses `recipe` if a replacement's span or precondition window overlaps `set`. Run it after
/// [`check_recipe_v2`](super::check_recipe_v2), which bounds every window; like it, it needs no
/// firmware.
///
/// # Errors
///
/// [`RecipeError::ProtectedSet`] for the first overlap, or a window error from a recipe that has
/// not passed `check_recipe_v2`.
pub fn check_recipe_against_protected_set(
    recipe: &RecipeV2,
    target: &RecipeTarget<'_>,
    set: &ProtectedSet,
) -> Result<(), RecipeError> {
    super::recipe::check_ranges(recipe, target, set.ranges(), |found| {
        let runtime = |offset: u64| target.load_address + offset;
        RecipeError::ProtectedSet {
            index: found.index,
            what: found.what,
            start: runtime(found.start),
            last: runtime(found.end - 1),
            set_start: runtime(found.protected.start as u64),
            set_last: runtime(found.protected.end as u64 - 1),
        }
    })
}
