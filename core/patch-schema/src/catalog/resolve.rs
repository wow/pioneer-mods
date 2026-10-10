//! Resolving a profile against a checked catalog ([`resolve`]): which chosen skins and features
//! a build can hold on the profile's player, which implementation of each, and why anything else
//! is switched off. Needs no firmware; composing the fragments is for the engine.
//!
//! The profile's choices are honoured as far as they fit:
//! 1. A skin or feature that is not in the catalog, has no implementation for the player, or
//!    none at the maturity the profile accepts, is off. So is a skin for another screen.
//! 2. Chosen features that conflict are both off: the owner chooses one. This is decided once,
//!    among the features step 1 leaves, whatever the skins.
//! 3. Then, in rounds from scratch until no skin goes off:
//!    - each feature takes its first implementation that fits the screens' skins: it draws the
//!      labels of a screen that keeps the `stock` skin, in the stock style, and leaves those of
//!      a screen with another skin to that skin;
//!    - each chosen skin takes the implementation drawing the labels of the most features that
//!      leave theirs to it (exactly those labels; the first such implementation on a tie), and
//!      the features it does not draw are off. A skin with no such implementation, not even
//!      one without labels, is off, its screen keeps `stock`, and the round starts again.
//!
//! An explicit skin choice is kept whenever it can be, and only a skin is ever switched off
//! between rounds, so the rounds end; features are picked afresh each round.

use super::check::Catalog;
use super::entry::Maturity;
use super::feature::{Feature, FeatureImplementation};
use super::player::Player;
use super::profile::Profile;
use super::resolution::{Resolution, ResolveError};
use super::skin::{STOCK_SKIN, SkinImplementation};
use crate::v2::RecipeV2;
use std::collections::BTreeMap;

/// Resolves `profile` against `catalog`, which must have passed [`Catalog::check`] with
/// `recipes`.
///
/// # Errors
///
/// [`ResolveError::Profile`] for an invalid profile, [`ResolveError::UnknownPlayer`] for a player
/// the catalog lacks. Anything narrower is switched off with its reason instead.
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
    while state.round() {}
    Ok(state.resolution(recipes))
}

/// Labels by slot name on one screen.
pub(super) type Labels = BTreeMap<String, Vec<String>>;

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
    /// The last round's implementation of each feature that is on, and why the others are off.
    pub(super) picks: BTreeMap<&'a str, &'a FeatureImplementation>,
    pub(super) unfit: BTreeMap<&'a str, String>,
    /// The last round's implementation of each non-stock skin, by screen.
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

    /// The skin `screen` uses: the chosen one, unless it is off.
    pub(super) fn effective(&self, screen: &str) -> &'a str {
        match self.chosen.get(screen) {
            Some(skin) if !self.skins_off.contains_key(screen) => skin,
            _ => STOCK_SKIN,
        }
    }

    /// One round from scratch; whether it switched a skin off (and another round is needed).
    fn round(&mut self) -> bool {
        self.picks.clear();
        self.unfit.clear();
        self.skin_picks.clear();
        for id in &self.profile.features {
            if self.features_off.contains_key(id.as_str()) {
                continue;
            }
            let feature = self.catalog.feature(id).expect("available");
            match self.pick(feature) {
                Ok(implementation) => {
                    self.picks.insert(id, implementation);
                }
                Err(reason) => {
                    self.unfit.insert(id, reason);
                }
            }
        }
        // A feature a skin does not draw no longer gives labels on its other screens: settle.
        loop {
            let mut changed = false;
            for screen in self.player.screens.iter().map(String::as_str) {
                let skin = self.effective(screen);
                if skin == STOCK_SKIN {
                    continue;
                }
                let ready = self.leaving_labels(screen);
                let Some((implementation, drawn)) = self.best_drawing(skin, screen, &ready) else {
                    let reason = if ready.is_empty() {
                        format!(
                            "skin {skin} has no implementation for {} without a feature's labels",
                            self.player.id
                        )
                    } else {
                        format!(
                            "skin {skin} has no implementation for {} drawing the labels of \
                             some of the chosen features, or none",
                            self.player.id
                        )
                    };
                    self.skins_off.insert(screen, reason);
                    return true;
                };
                self.skin_picks.insert(screen, implementation);
                for id in ready.into_iter().filter(|id| !drawn.contains(id)) {
                    let reason = self.undrawn(id, screen, skin);
                    self.picks.remove(id);
                    self.unfit.insert(id, reason);
                    changed = true;
                }
            }
            if !changed {
                return false;
            }
        }
    }

    /// The first implementation of `feature` at an accepted maturity that fits the skins.
    fn pick(&self, feature: &'a Feature) -> Result<&'a FeatureImplementation, String> {
        let accepted: Vec<&'a FeatureImplementation> = feature.implementations[&self.player.id]
            .iter()
            .filter(|i| self.accepts(i.maturity))
            .collect();
        let mut reasons = Vec::new();
        for implementation in accepted {
            match self.fits(feature, implementation) {
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
    ) -> Result<(), String> {
        for screen in labelled_screens(feature) {
            let skin = self.effective(screen);
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
