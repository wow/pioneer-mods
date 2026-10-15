//! An acceptance test: a scripted emulator run for one player that drives its panel and touch
//! screen and checks what the running application does (`catalog/acceptance/<id>.json`).
//!
//! A test holds no firmware and no pixels: it presses named controls, touches screen points and
//! checks memory values in the run-time state ranges its player names (never the loaded
//! application), or compares regions of the same screen with each other. Implementations name the
//! tests they must pass (`acceptance`); a runner executes them against an update in the emulator
//! (`docs/acceptance.md`).

use super::entry::{
    CatalogEntry, EntryError, check_id, check_name, check_player_id, check_schema_version,
    check_text,
};
use serde::{Deserialize, Serialize};

/// Most steps a test may have.
pub const MAX_STEPS: usize = 1000;
/// Longest single wait, or `wait_until` timeout, in milliseconds (10 minutes).
pub const MAX_WAIT_MS: u32 = 600_000;
/// Longest a test may wait in all, in milliseconds (one hour).
pub const MAX_TOTAL_WAIT_MS: u64 = 3_600_000;
/// Longest a control may be held or a touch last, in milliseconds.
pub const MAX_HOLD_MS: u32 = 10_000;
/// Most regions a `brightest` check compares against.
pub const MAX_OTHER_REGIONS: usize = 16;
/// Most `memory` checks a test may hold.
pub const MAX_MEMORY_CHECKS: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptanceTest {
    pub schema_version: u32,
    pub id: String,
    /// The player whose application the test runs (`xdj700-v1.15`).
    pub player: String,
    pub description: String,
    /// Whether the run needs the owner's USB stick image, with music, plugged in from the start.
    #[serde(default)]
    pub stick: bool,
    /// What the test does, in order. At least one step checks something.
    pub steps: Vec<Step>,
    pub maintainer: String,
}

/// One step of a test.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Step {
    /// Waits `ms` milliseconds of host time.
    Wait { ms: u32 },
    /// Presses one of the player's panel controls ([`Player::controls`](super::Player)) and
    /// releases it after `ms` milliseconds.
    Press {
        control: String,
        #[serde(default = "default_hold_ms")]
        ms: u32,
    },
    /// Touches the screen at (`x`, `y`), in screen pixels from the top left, for `ms`
    /// milliseconds.
    Touch {
        x: u32,
        y: u32,
        #[serde(default = "default_hold_ms")]
        ms: u32,
    },
    /// The check must hold now.
    Expect(Check),
    /// Polls the check until it holds; the test fails if it does not within `timeout_ms`.
    WaitUntil { check: Check, timeout_ms: u32 },
}

fn default_hold_ms() -> u32 {
    300
}

/// What a step checks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum Check {
    /// The `bytes`-byte value at the guest physical `address` (as the emulator's memory bus
    /// sees it), in the emulated processor's byte order, equals `equals`. The bytes must lie in
    /// one of the player's run-time state ranges ([`Player::state_memory`](super::Player)), so a
    /// test never holds the loaded application's contents.
    Memory {
        /// Aligned to `bytes`.
        address: Address,
        /// 1, 2 or 4.
        bytes: u8,
        equals: u32,
    },
    /// The mean brightness ([`mean_brightness`]) of `region` exceeds that of each of `others` by
    /// at least `margin` (1 to 255): for example, the pad that lights. Regions of the same frame
    /// are compared with each other; no reference image is stored.
    Brightest {
        region: Region,
        others: Vec<Region>,
        margin: u8,
    },
}

/// An address or offset, written `0x` and one to eight hexadecimal digits; anything else is
/// refused when the file is read. Two kinds, a release's load address apart: a decoded-application
/// offset (a slot or part image, a touch record, a part table) and a run-time address (a part's
/// touch handler, from the load address). A memory check's is a guest physical address.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Address(pub u32);

impl TryFrom<String> for Address {
    type Error = String;

    fn try_from(value: String) -> Result<Self, String> {
        parse_address(&value).map(Address).ok_or_else(|| {
            format!("{value:?}: an address is 0x and one to eight hexadecimal digits")
        })
    }
}

impl From<Address> for String {
    fn from(address: Address) -> String {
        format!("{:#010x}", address.0)
    }
}

/// A rectangle of the screen, in pixels from the top left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub w: u32,
    pub h: u32,
}

impl Region {
    /// Whether the region is not empty and lies within a `width` x `height` screen.
    pub fn fits(&self, width: u32, height: u32) -> bool {
        self.w > 0
            && self.h > 0
            && self
                .x
                .checked_add(self.w)
                .is_some_and(|right| right <= width)
            && self
                .y
                .checked_add(self.h)
                .is_some_and(|bottom| bottom <= height)
    }

    /// Whether the two regions share a pixel.
    pub fn overlaps(&self, other: &Region) -> bool {
        let span = |a: u32, a_len: u32, b: u32, b_len: u32| {
            u64::from(a) < u64::from(b) + u64::from(b_len)
                && u64::from(b) < u64::from(a) + u64::from(a_len)
        };
        span(self.x, self.w, other.x, other.w) && span(self.y, self.h, other.y, other.h)
    }
}

impl Check {
    /// The address a `memory` check reads (none for any other check).
    pub fn address(&self) -> Option<u32> {
        match self {
            Check::Memory { address, .. } => Some(address.0),
            Check::Brightest { .. } => None,
        }
    }

    fn validate(&self, field: &str) -> Result<(), EntryError> {
        let rule = |problem: &str| EntryError::Rule {
            field: field.to_owned(),
            problem: problem.to_owned(),
        };
        match self {
            Check::Memory {
                address,
                bytes,
                equals,
            } => {
                let address = address.0;
                if ![1, 2, 4].contains(bytes) {
                    return Err(rule("bytes: 1, 2 or 4"));
                }
                if address % u32::from(*bytes) != 0 {
                    return Err(rule("address: aligned to bytes"));
                }
                if *bytes < 4 && *equals >> (8 * u32::from(*bytes)) != 0 {
                    return Err(rule("equals: a value that fits in bytes"));
                }
            }
            Check::Brightest {
                region,
                others,
                margin,
            } => {
                if others.is_empty() || others.len() > MAX_OTHER_REGIONS {
                    return Err(rule("others: one to 16 regions"));
                }
                // An overlap shares pixels between the means it compares: the region could never
                // be brighter than itself, or by less than the margin meant.
                if others.iter().any(|other| region.overlaps(other)) {
                    return Err(rule("others: none may overlap region"));
                }
                if (others.iter().enumerate())
                    .any(|(index, a)| others[index + 1..].iter().any(|b| a.overlaps(b)))
                {
                    return Err(rule("others: no two may overlap"));
                }
                if *margin == 0 {
                    return Err(rule("margin: at least 1"));
                }
            }
        }
        Ok(())
    }

    /// How many bytes a `memory` check reads (0 for any other check).
    pub fn bytes(&self) -> u8 {
        match self {
            Check::Memory { bytes, .. } => *bytes,
            Check::Brightest { .. } => 0,
        }
    }

    /// The regions the check reads.
    pub fn regions(&self) -> Vec<Region> {
        match self {
            Check::Memory { .. } => Vec::new(),
            Check::Brightest { region, others, .. } => std::iter::once(*region)
                .chain(others.iter().copied())
                .collect(),
        }
    }
}

/// The brightness of an 8-bit RGB pixel, 0 to 255: Rec. 601 luma, `(299 R + 587 G + 114 B) /
/// 1000`, rounded down. A runner expands an RGB565 pixel to 8 bits a channel first
/// ([`expand_rgb565`]).
pub fn brightness(r: u8, g: u8, b: u8) -> u8 {
    let luma = (299 * u32::from(r) + 587 * u32::from(g) + 114 * u32::from(b)) / 1000;
    u8::try_from(luma).expect("at most 255")
}

/// An RGB565 pixel as 8-bit R, G and B, each channel's bits replicated into the low bits (31 and
/// 63 become 255).
pub fn expand_rgb565(pixel: u16) -> (u8, u8, u8) {
    let r = u8::try_from(pixel >> 11).expect("5 bits");
    let g = u8::try_from((pixel >> 5) & 0x3f).expect("6 bits");
    let b = u8::try_from(pixel & 0x1f).expect("5 bits");
    (
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    )
}

/// The mean brightness of `pixels` (8-bit RGB), rounded down; 0 for none. What a `brightest`
/// check compares.
pub fn mean_brightness(pixels: impl IntoIterator<Item = (u8, u8, u8)>) -> u8 {
    let (sum, count) = pixels
        .into_iter()
        .fold((0u64, 0u64), |(sum, count), (r, g, b)| {
            (sum + u64::from(brightness(r, g, b)), count + 1)
        });
    if count == 0 {
        return 0;
    }
    u8::try_from(sum / count).expect("a mean of bytes")
}

/// `0x` and one to eight hexadecimal digits.
pub fn parse_address(value: &str) -> Option<u32> {
    let digits = value.strip_prefix("0x")?;
    if digits.is_empty() || digits.len() > 8 || !digits.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    u32::from_str_radix(digits, 16).ok()
}

impl CatalogEntry for AcceptanceTest {
    const KIND: &'static str = "acceptance test";

    fn id(&self) -> &str {
        &self.id
    }

    fn validate(&self) -> Result<(), EntryError> {
        check_schema_version(self.schema_version)?;
        check_id("id", &self.id)?;
        check_player_id("player", &self.player)?;
        check_text("description", &self.description)?;
        if self.steps.is_empty() {
            return Err(EntryError::Empty {
                field: "steps".to_owned(),
            });
        }
        let rule = |field: String, problem: &str| EntryError::Rule {
            field,
            problem: problem.to_owned(),
        };
        if self.steps.len() > MAX_STEPS {
            return Err(rule("steps".to_owned(), "at most 1000 steps"));
        }
        let mut waited = 0u64;
        let mut checks = 0usize;
        let mut memory_checks = 0usize;
        for (index, step) in self.steps.iter().enumerate() {
            let field = format!("steps[{index}]");
            match step {
                Step::Wait { ms } => {
                    if *ms == 0 || *ms > MAX_WAIT_MS {
                        return Err(rule(field, "wait: 1 ms to 10 minutes"));
                    }
                    waited += u64::from(*ms);
                }
                Step::Press { control, ms } => {
                    check_name(&format!("{field}.control"), control)?;
                    if *ms == 0 || *ms > MAX_HOLD_MS {
                        return Err(rule(field, "press: held 1 ms to 10 s"));
                    }
                    waited += u64::from(*ms);
                }
                Step::Touch { ms, .. } => {
                    if *ms == 0 || *ms > MAX_HOLD_MS {
                        return Err(rule(field, "touch: held 1 ms to 10 s"));
                    }
                    waited += u64::from(*ms);
                }
                Step::Expect(check) => {
                    check.validate(&field)?;
                    checks += 1;
                    memory_checks += usize::from(check.address().is_some());
                }
                Step::WaitUntil { check, timeout_ms } => {
                    check.validate(&field)?;
                    memory_checks += usize::from(check.address().is_some());
                    if *timeout_ms == 0 || *timeout_ms > MAX_WAIT_MS {
                        return Err(rule(field, "wait_until: a timeout of 1 ms to 10 minutes"));
                    }
                    waited += u64::from(*timeout_ms);
                    checks += 1;
                }
            }
        }
        if checks == 0 {
            return Err(rule(
                "steps".to_owned(),
                "at least one step checks something",
            ));
        }
        if memory_checks > MAX_MEMORY_CHECKS {
            return Err(rule("steps".to_owned(), "at most 64 memory checks"));
        }
        if waited > MAX_TOTAL_WAIT_MS {
            return Err(rule(
                "steps".to_owned(),
                "waits, holds and timeouts of at most one hour",
            ));
        }
        check_text("maintainer", &self.maintainer)
    }
}
