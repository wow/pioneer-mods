//! The catalog as a whole ([`Catalog::check`]): every file valid on its own, ids unique, every
//! reference resolved, and every implementation consistent with its player and its recipe
//! (`check_parts.rs`). Needs no firmware.

use super::entry::{CatalogEntry, EntryError};
use super::feature::Feature;
use super::player::Player;
use super::screen::Screen;
use super::skin::Skin;
use crate::v2::RecipeV2;
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// Every catalog file, as loaded.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub players: Vec<Player>,
    pub screens: Vec<Screen>,
    pub features: Vec<Feature>,
    pub skins: Vec<Skin>,
}

/// Why the catalog is inconsistent. `entry` names the file's kind and id
/// (`feature beat-loop-1-to-32`).
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CatalogError {
    #[error("{entry}: {error}")]
    Invalid { entry: String, error: EntryError },

    #[error("{kind} {id} is defined twice")]
    Duplicate { kind: &'static str, id: String },

    #[error("{entry}: {what} is not in the catalog")]
    Missing { entry: String, what: String },

    #[error("{entry}: {problem}")]
    Inconsistent { entry: String, problem: String },
}

impl Catalog {
    pub fn player(&self, id: &str) -> Option<&Player> {
        self.players.iter().find(|player| player.id == id)
    }

    pub fn screen(&self, player: &str, id: &str) -> Option<&Screen> {
        self.screens
            .iter()
            .find(|screen| screen.player == player && screen.id == id)
    }

    pub fn feature(&self, id: &str) -> Option<&Feature> {
        self.features.iter().find(|feature| feature.id == id)
    }

    pub fn skin(&self, id: &str) -> Option<&Skin> {
        self.skins.iter().find(|skin| skin.id == id)
    }

    /// Checks the catalog against itself and against `recipes`, the recipes its implementations
    /// name, by path:
    /// - every file passes its own `validate`, and ids are unique;
    /// - every player's screens have files, and every screen's player lists it;
    /// - conflicts are resolved and listed on both sides, and two features that relabel one slot
    ///   conflict;
    /// - each implementation's player exists and meets the feature's or skin's requirements, and
    ///   label counts match slot counts;
    /// - each recipe is for that player, carries its pins and pins its output (every fragment of
    ///   a build must).
    ///
    /// # Errors
    ///
    /// The first [`CatalogError`] found.
    pub fn check(&self, recipes: &BTreeMap<String, RecipeV2>) -> Result<(), CatalogError> {
        self.check_entries()?;
        self.check_unique()?;
        self.check_screens()?;
        for feature in &self.features {
            self.check_feature(feature, recipes)?;
        }
        self.check_shared_labels()?;
        for skin in &self.skins {
            self.check_skin(skin, recipes)?;
        }
        Ok(())
    }

    fn check_entries(&self) -> Result<(), CatalogError> {
        validate_all(&self.players)?;
        validate_all(&self.screens)?;
        validate_all(&self.features)?;
        validate_all(&self.skins)
    }

    fn check_unique(&self) -> Result<(), CatalogError> {
        let ids = [
            (
                "player",
                self.players
                    .iter()
                    .map(|p| p.id.clone())
                    .collect::<Vec<_>>(),
            ),
            (
                "screen",
                self.screens
                    .iter()
                    .map(|s| format!("{}/{}", s.player, s.id))
                    .collect(),
            ),
            (
                "feature",
                self.features.iter().map(|f| f.id.clone()).collect(),
            ),
            ("skin", self.skins.iter().map(|s| s.id.clone()).collect()),
        ];
        for (kind, ids) in ids {
            let mut seen = BTreeSet::new();
            if let Some(id) = ids.into_iter().find(|id| !seen.insert(id.clone())) {
                return Err(CatalogError::Duplicate { kind, id });
            }
        }
        Ok(())
    }

    fn check_screens(&self) -> Result<(), CatalogError> {
        for player in &self.players {
            for screen in &player.screens {
                if self.screen(&player.id, screen).is_none() {
                    return Err(CatalogError::Missing {
                        entry: player_entry(&player.id),
                        what: format!("screen {}/{screen}", player.id),
                    });
                }
            }
        }
        for screen in &self.screens {
            let player = self
                .player(&screen.player)
                .ok_or_else(|| CatalogError::Missing {
                    entry: screen_entry(screen),
                    what: player_entry(&screen.player),
                })?;
            if !player.screens.contains(&screen.id) {
                return Err(CatalogError::Inconsistent {
                    entry: screen_entry(screen),
                    problem: format!("player {} does not list it", player.id),
                });
            }
        }
        Ok(())
    }

    /// Two features that give labels to one slot cannot both be chosen: they must conflict.
    fn check_shared_labels(&self) -> Result<(), CatalogError> {
        for (index, feature) in self.features.iter().enumerate() {
            for other in &self.features[index + 1..] {
                let shared = feature
                    .labels
                    .keys()
                    .find(|slot| other.labels.contains_key(*slot));
                if let Some(slot) = shared
                    && !feature.conflicts.contains(&other.id)
                {
                    return Err(CatalogError::Inconsistent {
                        entry: feature_entry(&feature.id),
                        problem: format!(
                            "it and {} both relabel {slot}; they must list each other in conflicts",
                            other.id
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    pub(super) fn implementation_player(
        &self,
        entry: &str,
        id: &str,
    ) -> Result<&Player, CatalogError> {
        self.player(id).ok_or_else(|| CatalogError::Missing {
            entry: entry.to_owned(),
            what: player_entry(id),
        })
    }

    /// The element count of `slot_ref` (`<screen>.<slot name>`) on `player`, if it has the slot.
    /// (`check_screens` has made every screen file of a player one it lists.)
    pub(super) fn slot_count(&self, player: &Player, slot_ref: &str) -> Option<u32> {
        let (screen, slot) = slot_ref.split_once('.')?;
        Some(self.screen(&player.id, screen)?.slots.get(slot)?.count)
    }
}

fn validate_all<T: CatalogEntry>(entries: &[T]) -> Result<(), CatalogError> {
    for entry in entries {
        entry.validate().map_err(|error| CatalogError::Invalid {
            entry: entry.name(),
            error,
        })?;
    }
    Ok(())
}

pub(super) fn player_entry(id: &str) -> String {
    format!("player {id}")
}

fn screen_entry(screen: &Screen) -> String {
    screen.name()
}

pub(super) fn feature_entry(id: &str) -> String {
    format!("feature {id}")
}

pub(super) fn skin_entry(id: &str) -> String {
    format!("skin {id}")
}
