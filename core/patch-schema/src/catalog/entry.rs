//! Rules every catalog file shares: the schema version, ids, dotted names, slot references,
//! recipe paths, labels and text.

use crate::is_valid_sha256_hex;
use thiserror::Error;

/// The only `schema_version` catalog files accept.
pub const CATALOG_SCHEMA_VERSION: u32 = 1;

/// Longest id or name.
pub const MAX_ID_LEN: usize = 64;

/// Longest label text a feature gives a slot.
pub const MAX_LABEL_LEN: usize = 16;

/// Why one catalog file is invalid on its own.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EntryError {
    #[error("unsupported schema_version {0}; catalog files are schema_version 1")]
    SchemaVersion(u32),

    #[error(
        "{field} {value:?} is not an id: lowercase letters, digits and '-' (a player id may also \
         hold '.'), starting with a letter or digit, at most 64 bytes"
    )]
    Id { field: String, value: String },

    #[error(
        "{field} {value:?} is not a name: lowercase letters, digits and '_' in segments joined by \
         '.', at most 64 bytes"
    )]
    Name { field: String, value: String },

    #[error("{field} {value:?} is not a slot reference of the form <screen>.<slot name>")]
    SlotRef { field: String, value: String },

    #[error(
        "{field} {value:?} is not a recipe path: relative, under recipes/, ending in .json, with \
         '/' separators and no empty, '.' or '..' components"
    )]
    RecipePath { field: String, value: String },

    #[error("{field} {value:?} is not a label: 1 to 16 printable ASCII characters, trimmed")]
    Label { field: String, value: String },

    #[error("{field} must be 64 hexadecimal characters")]
    Sha256 { field: String },

    #[error("{field} must not be empty")]
    Empty { field: String },

    #[error("{field} lists {value:?} twice")]
    Repeated { field: String, value: String },

    #[error("{field}: {problem}")]
    Rule { field: String, problem: String },
}

/// How settled an implementation is: `stable` (passed a hardware stage, and the maintainer
/// offers it by default), `experimental` (emulator rehearsals at least), `dev` (never offered).
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, serde::Serialize, serde::Deserialize,
)]
#[serde(rename_all = "snake_case")]
pub enum Maturity {
    Dev,
    Experimental,
    Stable,
}

pub(crate) fn check_schema_version(version: u32) -> Result<(), EntryError> {
    if version == CATALOG_SCHEMA_VERSION {
        Ok(())
    } else {
        Err(EntryError::SchemaVersion(version))
    }
}

/// A screen, feature or skin id: `[a-z0-9][a-z0-9-]*`, at most [`MAX_ID_LEN`] bytes.
pub fn is_id(value: &str) -> bool {
    id_chars(value, |c| c == '-')
}

/// A player id: as [`is_id`], and `.` too (`xdj700-v1.15`).
pub fn is_player_id(value: &str) -> bool {
    id_chars(value, |c| c == '-' || c == '.')
}

fn id_chars(value: &str, extra: impl Fn(char) -> bool) -> bool {
    let lower = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    value.len() <= MAX_ID_LEN
        && value.chars().next().is_some_and(lower)
        && value.chars().all(|c| lower(c) || extra(c))
}

/// A capability or slot name: segments of `[a-z0-9_]` joined by `.`, at most [`MAX_ID_LEN`]
/// bytes (`beat_loop.pad`).
pub fn is_name(value: &str) -> bool {
    value.len() <= MAX_ID_LEN
        && value.split('.').all(|segment| {
            !segment.is_empty()
                && segment
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        })
}

/// `<screen>.<slot name>` split at its first `.` (`perform.beat_loop.pad`), or `None`.
pub fn split_slot_ref(value: &str) -> Option<(&str, &str)> {
    let (screen, slot) = value.split_once('.')?;
    (is_id(screen) && is_name(slot)).then_some((screen, slot))
}

pub(crate) fn check_id(field: &str, value: &str) -> Result<(), EntryError> {
    checked(is_id(value), || EntryError::Id {
        field: field.to_owned(),
        value: value.to_owned(),
    })
}

pub(crate) fn check_player_id(field: &str, value: &str) -> Result<(), EntryError> {
    checked(is_player_id(value), || EntryError::Id {
        field: field.to_owned(),
        value: value.to_owned(),
    })
}

pub(crate) fn check_name(field: &str, value: &str) -> Result<(), EntryError> {
    checked(is_name(value), || EntryError::Name {
        field: field.to_owned(),
        value: value.to_owned(),
    })
}

pub(crate) fn check_slot_ref(field: &str, value: &str) -> Result<(), EntryError> {
    checked(split_slot_ref(value).is_some(), || EntryError::SlotRef {
        field: field.to_owned(),
        value: value.to_owned(),
    })
}

pub(crate) fn check_text(field: &str, value: &str) -> Result<(), EntryError> {
    checked(!value.trim().is_empty(), || EntryError::Empty {
        field: field.to_owned(),
    })
}

pub(crate) fn check_sha256(field: &str, value: &str) -> Result<(), EntryError> {
    checked(is_valid_sha256_hex(value), || EntryError::Sha256 {
        field: field.to_owned(),
    })
}

/// A recipe path relative to the repository root: `recipes/…/<file>.json`.
pub(crate) fn check_recipe_path(field: &str, value: &str) -> Result<(), EntryError> {
    let components_ok = value
        .split('/')
        .all(|part| !part.is_empty() && part != "." && part != "..");
    let chars_ok = value
        .chars()
        .all(|c| c.is_ascii_graphic() && c != '\\' && c != ':');
    let ok = value.starts_with("recipes/") && value.ends_with(".json") && components_ok && chars_ok;
    checked(ok, || EntryError::RecipePath {
        field: field.to_owned(),
        value: value.to_owned(),
    })
}

/// The labels a feature gives a slot, or a skin draws: at least one, each a short printable
/// ASCII text.
pub(crate) fn check_labels(field: &str, labels: &[String]) -> Result<(), EntryError> {
    if labels.is_empty() {
        return Err(EntryError::Empty {
            field: field.to_owned(),
        });
    }
    for label in labels {
        let ok = (1..=MAX_LABEL_LEN).contains(&label.len())
            && label.trim() == label
            && label.chars().all(|c| c == ' ' || c.is_ascii_graphic());
        checked(ok, || EntryError::Label {
            field: field.to_owned(),
            value: label.clone(),
        })?;
    }
    Ok(())
}

/// Refuses a value listed twice in `values`.
pub(crate) fn check_unique<'a>(
    field: &str,
    values: impl IntoIterator<Item = &'a String>,
) -> Result<(), EntryError> {
    let mut seen = std::collections::BTreeSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(EntryError::Repeated {
                field: field.to_owned(),
                value: value.clone(),
            });
        }
    }
    Ok(())
}

/// The fields every implementation has: its recipe, maturity evidence and known limits.
pub(crate) fn check_implementation(
    field: &str,
    recipe: &str,
    evidence: &str,
    limits: &[String],
) -> Result<(), EntryError> {
    check_recipe_path(&format!("{field}.recipe"), recipe)?;
    check_text(&format!("{field}.evidence"), evidence)?;
    for (index, limit) in limits.iter().enumerate() {
        check_text(&format!("{field}.limits[{index}]"), limit)?;
    }
    Ok(())
}

fn checked(ok: bool, error: impl FnOnce() -> EntryError) -> Result<(), EntryError> {
    if ok { Ok(()) } else { Err(error()) }
}
