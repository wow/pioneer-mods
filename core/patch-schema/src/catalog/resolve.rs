//! Resolving a profile against a checked catalog ([`resolve`]): which chosen skins and features
//! a build can hold on the profile's player, which implementation of each, and why anything else
//! is switched off. Needs no firmware; composing the fragments is for the engine.
//!
//! The profile's choices are honoured as far as they fit:
//! 1. A skin or feature that is not in the catalog, has no implementation for the player, or
//!    none at the maturity the profile accepts, is off. So is a skin for another screen.
//! 2. Chosen features that conflict are both off: the owner chooses one. This is decided once,
//!    among the features step 1 leaves, whatever the skins.
//! 3. The chosen skins are kept as far as they can be. Of every set of them, largest first, and
//!    every choice of one implementation per kept skin (`resolve_skins.rs`), resolution takes
//!    the configuration in which the most features are on (the first in screen and file order
//!    on a tie), where:
//!    - each feature takes its first implementation that fits the skins: it draws the labels of
//!      a screen that keeps `stock`, in the stock style, and leaves those of a screen with a
//!      kept skin to that skin;
//!    - each kept skin's implementation draws exactly the labels of the features kept on, and a
//!      feature whose labels it does not draw is off.
//!
//! Keeping no skin is always a configuration, so there is always a result, whatever the order
//! of screens or files. The search is small for real catalogs; one that would need more than
//! [`MAX_CONFIGURATIONS`] is refused rather than searched slowly.

use super::check::Catalog;
use super::entry::Maturity;
use super::feature::{Feature, FeatureImplementation};
use super::player::Player;
use super::profile::Profile;
use super::resolution::{Resolution, ResolveError};
use super::skin::{STOCK_SKIN, SkinImplementation};
use crate::v2::RecipeV2;
use std::collections::BTreeMap;

/// The most configurations (a set of kept skins and one implementation each) resolution tries.
pub const MAX_CONFIGURATIONS: u64 = 1 << 16;

/// Resolves `profile` against `catalog`, which must have passed [`Catalog::check`] with
/// `recipes`.
///
/// # Errors
///
/// [`ResolveError::Profile`] for an invalid profile, [`ResolveError::UnknownPlayer`] for a player
/// the catalog lacks, [`ResolveError::TooManyConfigurations`] for a search too large. Anything
/// narrower is switched off with its reason instead.
pub fn resolve(
    catalog: &Catalog,
    recipes: &BTreeMap<String, RecipeV2>,
    profile: &Profile,
) -> Result<Resolution, ResolveError> {
    profile.validate().map_err(ResolveError::Profile)?;
    let player = catalog
        .player(&profile.player)
        .ok_or_else(|| ResolveError::UnknownPlayer(profile.player.clone()))?;
    let mut state = State::new(catalog, player, profile);
    state.settle()?;
    Ok(state.resolution(recipes))
}

/// The skin each screen uses in a configuration, by screen; a screen not listed keeps `stock`.
pub(super) type Skins<'a> = BTreeMap<&'a str, &'a str>;

pub(super) struct State<'a> {
    pub(super) catalog: &'a Catalog,
    pub(super) player: &'a Player,
    pub(super) profile: &'a Profile,
    /// The skin chosen for each player screen.
    pub(super) chosen: BTreeMap<&'a str, &'a str>,
    /// Chosen skins switched off, by screen; the screen keeps `stock`.
    pub(super) skins_off: BTreeMap<&'a str, String>,
    /// Features off whatever the skins (steps 1 and 2).
    pub(super) features_off: BTreeMap<&'a str, String>,
    /// The implementation of each feature that is on, and why the others are off.
    pub(super) picks: BTreeMap<&'a str, &'a FeatureImplementation>,
    pub(super) unfit: BTreeMap<&'a str, String>,
    /// The implementation of each kept skin, by screen.
    pub(super) skin_picks: BTreeMap<&'a str, &'a SkinImplementation>,
}

impl<'a> State<'a> {
    fn new(catalog: &'a Catalog, player: &'a Player, profile: &'a Profile) -> Self {
        let mut state = State {
            catalog,
            player,
            profile,
            chosen: BTreeMap::new(),
            skins_off: BTreeMap::new(),
            features_off: BTreeMap::new(),
            picks: BTreeMap::new(),
            unfit: BTreeMap::new(),
            skin_picks: BTreeMap::new(),
        };
        for screen in &player.screens {
            let skin = profile
                .screens
                .get(screen)
                .map_or(STOCK_SKIN, String::as_str);
            state.chosen.insert(screen, skin);
            if skin != STOCK_SKIN
                && let Err(reason) = state.skin_available(screen, skin)
            {
                state.skins_off.insert(screen, reason);
            }
        }
        for id in &profile.features {
            if let Err(reason) = state.feature_available(id) {
                state.features_off.insert(id, reason);
            }
        }
        state.switch_off_conflicts();
        state
    }

    pub(super) fn accepts(&self, maturity: Maturity) -> bool {
        maturity >= self.profile.maturity && maturity != Maturity::Dev
    }

    /// Why the profile's maturity setting refuses `maturities`.
    fn too_unsettled(&self, maturities: impl Iterator<Item = Maturity>) -> String {
        let mut maturities: Vec<Maturity> = maturities.collect();
        let count = maturities.len();
        maturities.sort_unstable();
        maturities.dedup();
        let names: Vec<String> = maturities.iter().map(Maturity::to_string).collect();
        let accepted = match self.profile.maturity {
            Maturity::Stable => "stable only",
            _ => "experimental and stable",
        };
        let (implementations, are) = if count == 1 {
            ("implementation", "is")
        } else {
            ("implementations", "are")
        };
        format!(
            "its {implementations} for {} {are} {}; the profile accepts {accepted}",
            self.player.id,
            names.join(" and ")
        )
    }

    fn skin_available(&self, screen: &str, id: &str) -> Result<(), String> {
        let skin = self
            .catalog
            .skin(id)
            .ok_or_else(|| format!("skin {id} is not in the catalog"))?;
        if skin.screen != screen {
            return Err(format!("skin {id} is for screen {}", skin.screen));
        }
        let implementations = skin
            .implementations
            .get(&self.player.id)
            .ok_or_else(|| format!("skin {id} has no implementation for {}", self.player.id))?;
        if implementations.iter().any(|i| self.accepts(i.maturity)) {
            Ok(())
        } else {
            let reason = self.too_unsettled(implementations.iter().map(|i| i.maturity));
            Err(format!("skin {id}: {reason}"))
        }
    }

    fn feature_available(&self, id: &str) -> Result<(), String> {
        let feature = self
            .catalog
            .feature(id)
            .ok_or_else(|| "it is not in the catalog".to_owned())?;
        let implementations = feature
            .implementations
            .get(&self.player.id)
            .ok_or_else(|| format!("it has no implementation for {}", self.player.id))?;
        if implementations.iter().any(|i| self.accepts(i.maturity)) {
            Ok(())
        } else {
            Err(self.too_unsettled(implementations.iter().map(|i| i.maturity)))
        }
    }

    /// Chosen features that conflict with another one still available are both off.
    fn switch_off_conflicts(&mut self) {
        let available: Vec<&'a str> = self
            .profile
            .features
            .iter()
            .map(String::as_str)
            .filter(|id| !self.features_off.contains_key(id))
            .collect();
        let mut off = BTreeMap::new();
        for id in &available {
            let feature = self.catalog.feature(id).expect("available");
            let others: Vec<&str> = feature
                .conflicts
                .iter()
                .map(String::as_str)
                .filter(|other| available.contains(other))
                .collect();
            if !others.is_empty() {
                let reason = format!(
                    "it conflicts with {}, also chosen; choose one",
                    others.join(" and ")
                );
                off.insert(*id, reason);
            }
        }
        self.features_off.extend(off);
    }

    /// Step 3: the configuration that keeps the most chosen skins, then the most features.
    fn settle(&mut self) -> Result<(), ResolveError> {
        let candidates: Vec<&'a str> = self
            .player
            .screens
            .iter()
            .map(String::as_str)
            .filter(|s| self.chosen[s] != STOCK_SKIN && !self.skins_off.contains_key(s))
            .collect();
        let configurations = candidates.iter().fold(1_u64, |count, screen| {
            let implementations = self.skin_implementations(screen).len() as u64;
            count.saturating_mul(implementations + 1)
        });
        if configurations > MAX_CONFIGURATIONS {
            return Err(ResolveError::TooManyConfigurations(configurations));
        }
        for size in (0..=candidates.len()).rev() {
            let best = subsets(&candidates, size)
                .into_iter()
                .filter_map(|kept| self.configuration(&kept))
                .reduce(|best, next| if next.on() > best.on() { next } else { best });
            if let Some(best) = best {
                for screen in candidates.iter().filter(|s| !best.kept.contains(s)) {
                    let reason = self.dropped(screen, &best.kept);
                    self.skins_off.insert(screen, reason);
                }
                self.picks = best.picks;
                self.unfit = best.unfit;
                self.skin_picks = best.skin_picks;
                return Ok(());
            }
        }
        unreachable!("keeping no skin is always a configuration")
    }

    /// The first implementation of `feature` at an accepted maturity that fits `skins`.
    pub(super) fn pick(
        &self,
        feature: &'a Feature,
        skins: &Skins<'a>,
    ) -> Result<&'a FeatureImplementation, String> {
        let accepted: Vec<&'a FeatureImplementation> = feature.implementations[&self.player.id]
            .iter()
            .filter(|i| self.accepts(i.maturity))
            .collect();
        let mut reasons = Vec::new();
        for implementation in accepted {
            match self.fits(feature, implementation, skins) {
                Ok(()) => return Ok(implementation),
                Err(reason) => reasons.push(reason),
            }
        }
        Err(if reasons.len() == 1 {
            reasons.remove(0)
        } else {
            format!(
                "none of its implementations for {} fits the skins: {}",
                self.player.id,
                reasons.join("; ")
            )
        })
    }

    fn fits(
        &self,
        feature: &Feature,
        implementation: &FeatureImplementation,
        skins: &Skins<'a>,
    ) -> Result<(), String> {
        for screen in labelled_screens(feature) {
            let skin = skins.get(screen).copied().unwrap_or(STOCK_SKIN);
            let draws = implementation.draws_labels.contains_key(screen);
            if draws && skin != STOCK_SKIN {
                return Err(format!(
                    "it draws the {screen} labels in the stock style, and {screen} uses skin \
                     {skin}"
                ));
            }
            if !draws && skin == STOCK_SKIN {
                return Err(format!(
                    "it leaves the {screen} labels to a skin, and {screen} uses the stock skin, \
                     which draws none"
                ));
            }
        }
        Ok(())
    }
}

/// The screens `feature` gives labels on.
pub(super) fn labelled_screens(feature: &Feature) -> impl Iterator<Item = &str> {
    let mut screens: Vec<&str> = feature
        .labels
        .keys()
        .filter_map(|slot_ref| slot_ref.split_once('.').map(|(screen, _)| screen))
        .collect();
    screens.dedup();
    screens.into_iter()
}

/// Every subset of `items` with `size` of them, in order.
fn subsets<T: Copy>(items: &[T], size: usize) -> Vec<Vec<T>> {
    if size == 0 {
        return vec![Vec::new()];
    }
    let mut all = Vec::new();
    for (index, &item) in items.iter().enumerate() {
        for mut rest in subsets(&items[index + 1..], size - 1) {
            rest.insert(0, item);
            all.push(rest);
        }
    }
    all
}
