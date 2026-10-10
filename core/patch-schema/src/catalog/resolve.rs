//! Resolving a profile against a checked catalog ([`resolve`]): which chosen skins and features
//! a build can hold on the profile's player, which implementation of each, and why anything else
//! is switched off. Needs no firmware; composing the fragments is for the engine.
//!
//! The profile's choices are honoured as far as they fit, in rounds until nothing changes:
//! 1. A skin or feature that is not in the catalog, has no implementation for the player, or
//!    none at the maturity the profile accepts, is off for good.
//! 2. Each feature takes its first implementation that fits the screens' skins: a screen whose
//!    labels the implementation draws must use that skin, and a screen whose labels it leaves to
//!    the skin must not use `stock`, which draws none.
//! 3. Chosen features that conflict are both off for good: the owner chooses one.
//! 4. Each chosen skin needs an implementation drawing exactly the labels left to it. If it has
//!    none, the features that leave their labels to it are off for good; with no such labels,
//!    the skin is, and its screen keeps `stock`.
//!
//! Every round that changes something switches something off for good, so the rounds end.

use super::check::Catalog;
use super::entry::Maturity;
use super::feature::{Feature, FeatureImplementation};
use super::player::Player;
use super::profile::Profile;
use super::resolution::{Resolution, ResolveError};
use super::resolve_output::maturity_name;
use super::skin::{STOCK_SKIN, SkinImplementation};
use std::collections::BTreeMap;

/// Resolves `profile` against `catalog`, which must have passed [`Catalog::check`].
///
/// # Errors
///
/// [`ResolveError::Profile`] for an invalid profile, [`ResolveError::UnknownPlayer`] for a player
/// the catalog lacks. Anything narrower is switched off with its reason instead.
pub fn resolve(catalog: &Catalog, profile: &Profile) -> Result<Resolution, ResolveError> {
    profile.validate().map_err(ResolveError::Profile)?;
    let player = catalog
        .player(&profile.player)
        .ok_or_else(|| ResolveError::UnknownPlayer(profile.player.clone()))?;
    let mut state = State::new(catalog, player, profile);
    while state.round() {}
    Ok(state.resolution())
}

/// Labels by slot name on one screen.
type Labels = BTreeMap<String, Vec<String>>;

pub(super) struct State<'a> {
    pub(super) catalog: &'a Catalog,
    pub(super) player: &'a Player,
    pub(super) profile: &'a Profile,
    /// The skin chosen for each player screen.
    pub(super) chosen: BTreeMap<&'a str, &'a str>,
    /// Chosen skins off for good, by screen; the screen keeps `stock`.
    pub(super) skins_off: BTreeMap<&'a str, String>,
    /// Features off for good.
    pub(super) features_off: BTreeMap<&'a str, String>,
    /// The last round's implementation of each feature that fits, and why the others do not.
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
        state
    }

    fn accepts(&self, maturity: Maturity) -> bool {
        maturity >= self.profile.maturity && maturity != Maturity::Dev
    }

    /// Why the profile's maturity setting refuses `maturities`.
    fn too_unsettled(&self, maturities: impl Iterator<Item = Maturity>) -> String {
        let mut maturities: Vec<Maturity> = maturities.collect();
        let count = maturities.len();
        maturities.sort_unstable();
        maturities.dedup();
        let names: Vec<&str> = maturities.into_iter().map(maturity_name).collect();
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

    /// The skin `screen` uses: the chosen one, unless it is off.
    fn effective(&self, screen: &str) -> &'a str {
        match self.chosen.get(screen) {
            Some(skin) if !self.skins_off.contains_key(screen) => skin,
            _ => STOCK_SKIN,
        }
    }

    /// One round; whether it switched anything off for good.
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
        if self.switch_off_conflicts() {
            return true;
        }
        for screen in self.player.screens.iter().map(String::as_str) {
            let skin = self.effective(screen);
            if skin == STOCK_SKIN {
                continue;
            }
            let required = self.labels_left_to_skin(screen);
            let implementations =
                &self.catalog.skin(skin).expect("available").implementations[&self.player.id];
            let drawing = implementations
                .iter()
                .find(|i| self.accepts(i.maturity) && i.labels == required);
            if let Some(implementation) = drawing {
                self.skin_picks.insert(screen, implementation);
            } else if required.is_empty() {
                let reason = format!(
                    "skin {skin} has no implementation for {} without a feature's labels",
                    self.player.id
                );
                self.skins_off.insert(screen, reason);
                return true;
            } else {
                let before = self.features_off.len();
                self.switch_off_undrawn(screen, skin);
                // The labels come from picked features, so at least one is switched off and the
                // rounds still end.
                assert!(
                    self.features_off.len() > before,
                    "labels without a picked feature"
                );
                return true;
            }
        }
        false
    }

    /// The first implementation of `feature` at an accepted maturity that fits the skins.
    fn pick(&self, feature: &'a Feature) -> Result<&'a FeatureImplementation, String> {
        let accepted: Vec<&FeatureImplementation> = feature.implementations[&self.player.id]
            .iter()
            .filter(|i| self.accepts(i.maturity))
            .collect();
        let mut first_reason = None;
        for implementation in &accepted {
            match self.fits(feature, implementation) {
                Ok(()) => return Ok(implementation),
                Err(reason) => {
                    first_reason.get_or_insert(reason);
                }
            }
        }
        let reason = first_reason.expect("an accepted implementation");
        Err(if accepted.len() == 1 {
            reason
        } else {
            format!(
                "none of its implementations for {} fits the skins; the first: {reason}",
                self.player.id
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
            match implementation.draws_labels.get(screen) {
                Some(style) if style == skin => {}
                Some(style) => {
                    return Err(format!(
                        "it draws the {screen} labels as skin {style}, but {screen} uses skin \
                         {skin}"
                    ));
                }
                None if skin == STOCK_SKIN => {
                    return Err(format!(
                        "it leaves the {screen} labels to the skin, and {screen} uses the stock \
                         skin, which draws none"
                    ));
                }
                None => {}
            }
        }
        Ok(())
    }

    /// Chosen features that conflict with another chosen one are both off for good.
    fn switch_off_conflicts(&mut self) -> bool {
        let mut off = BTreeMap::new();
        for id in self.picks.keys() {
            let feature = self.catalog.feature(id).expect("available");
            let others: Vec<&str> = feature
                .conflicts
                .iter()
                .map(String::as_str)
                .filter(|other| self.picks.contains_key(other))
                .collect();
            if !others.is_empty() {
                let reason = format!(
                    "it conflicts with {}, also chosen; choose one",
                    others.join(" and ")
                );
                off.insert(*id, reason);
            }
        }
        let changed = !off.is_empty();
        self.features_off.extend(off);
        changed
    }

    /// The labels the picked features leave to the skin on `screen`, by slot name.
    fn labels_left_to_skin(&self, screen: &str) -> Labels {
        let mut labels = Labels::new();
        for (id, implementation) in &self.picks {
            if implementation.draws_labels.contains_key(screen) {
                continue;
            }
            let feature = self.catalog.feature(id).expect("available");
            for (slot_ref, slot_labels) in &feature.labels {
                if let Some((on, slot)) = slot_ref.split_once('.')
                    && on == screen
                {
                    labels.insert(slot.to_owned(), slot_labels.clone());
                }
            }
        }
        labels
    }

    /// The picked features that leave labels to `skin` on `screen`, which cannot draw them, are
    /// off for good.
    fn switch_off_undrawn(&mut self, screen: &str, skin: &str) {
        let mut off = BTreeMap::new();
        for (id, implementation) in &self.picks {
            let feature = self.catalog.feature(id).expect("available");
            if implementation.draws_labels.contains_key(screen)
                || !labelled_screens(feature).any(|labelled| labelled == screen)
            {
                continue;
            }
            let stock_style = feature.implementations[&self.player.id].iter().any(|i| {
                self.accepts(i.maturity)
                    && i.draws_labels.get(screen).is_some_and(|s| s == STOCK_SKIN)
            });
            let hint = if stock_style {
                format!("; with the stock skin on {screen}, its stock-style implementation applies")
            } else {
                String::new()
            };
            let reason = format!(
                "skin {skin} on {screen} has no implementation for {} drawing its labels{hint}",
                self.player.id
            );
            off.insert(*id, reason);
        }
        self.features_off.extend(off);
    }
}

/// The screens `feature` gives labels on.
fn labelled_screens(feature: &Feature) -> impl Iterator<Item = &str> {
    let mut screens: Vec<&str> = feature
        .labels
        .keys()
        .filter_map(|slot_ref| slot_ref.split_once('.').map(|(screen, _)| screen))
        .collect();
    screens.dedup();
    screens.into_iter()
}
