//! What resolving a profile gives ([`resolve`](fn@super::resolve)): every chosen screen skin and
//! feature, on or off with its reason, and the fragments (recipes) a build composes.

use super::entry::{EntryError, Maturity};
use thiserror::Error;

/// A resolved profile. Nothing the profile names is dropped: each skin choice and feature is
/// listed, on or off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    pub player: String,
    pub label: String,
    pub reported_version: String,
    /// The least settled implementation the profile accepts.
    pub maturity: Maturity,
    /// One per player screen, in the player's order.
    pub screens: Vec<ScreenResolution>,
    /// The screens the profile names that the player lacks; their choices are ignored.
    pub missing_screens: Vec<String>,
    /// One per profile feature, in the profile's order.
    pub features: Vec<FeatureResolution>,
    /// The recipes to compose, features first (profile order) then skins (screen order), each
    /// once.
    pub fragments: Vec<Fragment>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenResolution {
    pub screen: String,
    /// The skin the profile chose (`stock` when it names none).
    pub chosen: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureResolution {
    pub feature: String,
    pub status: Status,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Status {
    /// On, built by this recipe; `evidence` backs the implementation's maturity, and `limits`
    /// are its known limits.
    On {
        recipe: String,
        maturity: Maturity,
        evidence: String,
        limits: Vec<String>,
    },
    /// The built-in `stock` skin: the screen keeps the player's own look, with no recipe.
    Stock,
    /// Switched off; a screen whose skin is off keeps the `stock` skin.
    Off { reason: String },
}

/// One recipe of the build, and what it builds (`feature beat-loop-1-to-32`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fragment {
    pub recipe: String,
    /// The least settled of the implementations that name the recipe.
    pub maturity: Maturity,
    pub builds: Vec<String>,
    /// Whether the profile's label and reported version are the recipe's own, so that the
    /// fragment alone builds the very file its pins describe.
    pub as_pinned: bool,
}

impl Resolution {
    /// The tier of the build: its least settled fragment's when the build is one fragment under
    /// the recipe's own label and reported version (the very file its pins describe), and
    /// `experimental` at most otherwise: a combination, or another label or version, is a new
    /// update that no listed combination covers yet (`docs/modular-builds.md`, "Building a
    /// profile"). `None` with no fragment.
    pub fn tier(&self) -> Option<Maturity> {
        let least = self
            .fragments
            .iter()
            .map(|fragment| fragment.maturity)
            .min()?;
        Some(match self.fragments.as_slice() {
            [lone] if lone.as_pinned => least,
            _ => least.min(Maturity::Experimental),
        })
    }
}

/// Why a profile cannot be resolved at all (anything narrower is switched off instead).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    #[error("the profile is invalid: {0}")]
    Profile(EntryError),

    #[error("player {0} is not in the catalog")]
    UnknownPlayer(String),

    /// The chosen skins and their implementations would need more configurations than
    /// [`MAX_CONFIGURATIONS`](super::MAX_CONFIGURATIONS) to search.
    #[error(
        "the chosen skins have {0} configurations to search, more than {max}; choose fewer \
         skins",
        max = super::MAX_CONFIGURATIONS
    )]
    TooManyConfigurations(u64),
}
