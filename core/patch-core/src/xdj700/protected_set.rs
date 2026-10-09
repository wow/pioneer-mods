//! The protected set: code that runs at start-up or in the update path, measured in emulation and
//! kept outside this repository (`docs/xdj700-flashing.md`, section 5). A recipe whose span,
//! precondition window or edited image overlaps it is refused, so the rule is checked by the tool
//! and not only by hand.
//!
//! The set is a text file for one release, one range per line:
//!
//! ```text
//! # comments and blank lines are ignored
//! release xdj700-v1.15
//! start   end     bytes
//! 08000600        08000605        6
//! 0x08000610      0x08000649
//! ```
//!
//! The first line that is not blank or a comment names the release the set was measured on; a set
//! for another release is refused, since code moves between releases. The next may be a header,
//! exactly `start end` or `start end bytes`. Every other line is a range: `start` and `end` are
//! hexadecimal (an `0x` prefix is optional) and `end` is the last address of the range, inclusive.
//! An optional third column counts the bytes in decimal digits; it must equal `end - start + 1`,
//! which catches a file written with exclusive ends. A run-time address is the target's
//! [`RecipeTarget::load_address`] plus a decoded-application offset, and every range must lie
//! inside the application. Ranges may overlap and need not be sorted. A leading byte-order mark is
//! ignored. A set without ranges is refused, so a wrong file cannot pass silently.

use super::recipe::{RecipeError, RecipeTarget};
use patch_schema::{RecipeV2, WindowOwner};
use std::ops::Range;
use thiserror::Error;

/// Why a protected-set file was refused. Line numbers count from 1.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ProtectedSetError {
    #[error(
        "line {line}: expected `release <id>` naming the release the set was measured on, found \
         {found:?}"
    )]
    NoRelease { line: usize, found: String },

    #[error("the set was measured on release {set}, not {target}; code moves between releases")]
    WrongRelease { set: String, target: String },

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

    #[error("line {line}: end {end:#010x} is below start {start:#010x}")]
    Reversed { line: usize, start: u64, end: u64 },

    #[error(
        "line {line}: bytes is {count}, but {start:#010x}..={end:#010x} holds {expected} \
         (`end` is the last address, inclusive)"
    )]
    ByteCount {
        line: usize,
        start: u64,
        end: u64,
        count: u64,
        expected: u64,
    },

    #[error(
        "line {line}: {start:#010x}..={end:#010x} is not inside the application, run-time \
         {app_start:#010x}..={app_end:#010x} (the set uses run-time addresses: the load address \
         plus a decoded offset)"
    )]
    OutsideApplication {
        line: usize,
        start: u64,
        end: u64,
        app_start: u64,
        app_end: u64,
    },

    #[error("release {target} has an empty application, so no range can lie inside it")]
    NoApplication { target: String },

    #[error("the protected set holds no ranges")]
    Empty,
}

/// A parsed protected set for one release, as half-open decoded-application ranges.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProtectedSet {
    release: String,
    ranges: Vec<Range<usize>>,
}

impl ProtectedSet {
    /// Parses `text` (the format in the module docs) for `target`.
    ///
    /// # Errors
    ///
    /// A [`ProtectedSetError`] naming the first bad line, [`ProtectedSetError::WrongRelease`],
    /// [`ProtectedSetError::NoApplication`], or [`ProtectedSetError::Empty`].
    pub fn parse(text: &str, target: &RecipeTarget<'_>) -> Result<Self, ProtectedSetError> {
        if target.application_len == 0 {
            return Err(ProtectedSetError::NoApplication {
                target: target.id.to_owned(),
            });
        }
        let app_start = target.load_address;
        let app_end = app_start.saturating_add(target.application_len as u64 - 1);
        let text = text.strip_prefix('\u{feff}').unwrap_or(text);
        let mut release = None;
        let mut header_allowed = false;
        let mut ranges = Vec::new();
        for (index, raw_line) in text.lines().enumerate() {
            let line = index + 1;
            let content = raw_line.trim();
            if content.is_empty() || content.starts_with('#') {
                continue;
            }
            let fields: Vec<&str> = content.split_whitespace().collect();
            if release.is_none() {
                let [keyword, id] = fields[..] else {
                    return Err(no_release(line, content));
                };
                if keyword != "release" {
                    return Err(no_release(line, content));
                }
                if id != target.id {
                    return Err(ProtectedSetError::WrongRelease {
                        set: id.to_owned(),
                        target: target.id.to_owned(),
                    });
                }
                release = Some(id.to_owned());
                header_allowed = true;
                continue;
            }
            let is_header = matches!(fields[..], ["start", "end"] | ["start", "end", "bytes"]);
            let header_here = header_allowed;
            header_allowed = false;
            if header_here && is_header {
                continue;
            }
            ranges.push(parse_range(line, &fields, app_start, app_end)?);
        }
        let Some(release) = release else {
            return Err(ProtectedSetError::Empty);
        };
        if ranges.is_empty() {
            return Err(ProtectedSetError::Empty);
        }
        Ok(Self { release, ranges })
    }

    /// The release the set was measured on.
    pub fn release(&self) -> &str {
        &self.release
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

fn no_release(line: usize, content: &str) -> ProtectedSetError {
    ProtectedSetError::NoRelease {
        line,
        found: content.to_owned(),
    }
}

/// One range line, as a half-open decoded-application range.
fn parse_range(
    line: usize,
    fields: &[&str],
    app_start: u64,
    app_end: u64,
) -> Result<Range<usize>, ProtectedSetError> {
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
    // Bounds first: inside the application, the count below cannot overflow.
    if start < app_start || end > app_end {
        return Err(ProtectedSetError::OutsideApplication {
            line,
            start,
            end,
            app_start,
            app_end,
        });
    }
    let expected = end - start + 1;
    if let Some(value) = fields.get(2) {
        let not_count = || ProtectedSetError::NotCount {
            line,
            value: (*value).to_owned(),
        };
        // Digits only: `str::parse` would also accept a leading `+`.
        if !value.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(not_count());
        }
        let count = value.parse::<u64>().map_err(|_| not_count())?;
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
    // Inside the application, so both fit in usize.
    let offset = |address: u64| (address - app_start) as usize;
    Ok(offset(start)..offset(end) + 1)
}

fn parse_hex(line: usize, field: &'static str, value: &str) -> Result<u64, ProtectedSetError> {
    let not_hex = || ProtectedSetError::NotHex {
        line,
        field,
        value: value.to_owned(),
    };
    let digits = value
        .strip_prefix("0x")
        .or_else(|| value.strip_prefix("0X"))
        .unwrap_or(value);
    // `from_str_radix` accepts a leading `+`, which is not an address.
    if digits.is_empty() || !digits.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(not_hex());
    }
    u64::from_str_radix(digits, 16).map_err(|_| not_hex())
}

/// Refuses `recipe` if `set` was measured on another release, if the version string every rebuild
/// writes lies in `set`, or if a replacement's span or precondition window, or an edited image,
/// overlaps it. Run it
/// after [`check_recipe_v2`](super::check_recipe_v2), which bounds every window; like it, it needs
/// no firmware. The recipe entry points run it when their [`RecipeChecks`](super::RecipeChecks)
/// carry a set.
///
/// # Errors
///
/// [`RecipeError::ProtectedSetRelease`], [`RecipeError::ProtectedSetVersion`],
/// [`RecipeError::ProtectedSet`] or [`RecipeError::ImageProtectedSet`] for the first overlap, or a
/// window error from a recipe that has not passed `check_recipe_v2`.
pub fn check_recipe_against_protected_set(
    recipe: &RecipeV2,
    target: &RecipeTarget<'_>,
    set: &ProtectedSet,
) -> Result<(), RecipeError> {
    if set.release() != target.id {
        return Err(RecipeError::ProtectedSetRelease {
            set: set.release().to_owned(),
            target: target.id.to_owned(),
        });
    }
    let runtime = |offset: u64| target.load_address.saturating_add(offset);
    if let Some(block) = target.release.version_block {
        let text = block.text_range();
        let overlap = |range: &&Range<usize>| text.start < range.end && range.start < text.end;
        if let Some(range) = set.ranges().iter().find(overlap) {
            return Err(RecipeError::ProtectedSetVersion {
                start: runtime(text.start as u64),
                last: runtime(text.end as u64 - 1),
                set_start: runtime(range.start as u64),
                set_last: runtime(range.end as u64 - 1),
            });
        }
    }
    super::recipe_checks::check_ranges(recipe, target, set.ranges(), |found| {
        let (start, last) = (runtime(found.start), runtime(found.end - 1));
        let set_start = runtime(found.protected.start as u64);
        let set_last = runtime(found.protected.end as u64 - 1);
        match found.owner {
            WindowOwner::Replacement(index) => RecipeError::ProtectedSet {
                index,
                what: found.what,
                start,
                last,
                set_start,
                set_last,
            },
            WindowOwner::ImageEdit(index) => RecipeError::ImageProtectedSet {
                index,
                start,
                last,
                set_start,
                set_last,
            },
        }
    })
}
