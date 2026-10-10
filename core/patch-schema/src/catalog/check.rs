//! The catalog as a whole ([`Catalog::check`]): every file valid on its own, ids unique, every
//! reference resolved, and every implementation consistent with its player and its recipe. Needs
//! no firmware.

use super::entry::EntryError;
use super::feature::Feature;
use super::player::Player;
use super::screen::Screen;
use super::skin::{STOCK_SKIN, Skin};
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
    #[error("{entry}: {source}")]
    Invalid {
        entry: String,
        #[source]
        source: EntryError,
    },

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
    /// name, by path. Each implementation's player must exist and meet the feature's or skin's
    /// requirements; label counts must match slot counts; and each recipe must be for that
    /// player, carry its pins and pin its output (every fragment of a build must).
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
        for skin in &self.skins {
            self.check_skin(skin, recipes)?;
        }
        Ok(())
    }

    fn check_entries(&self) -> Result<(), CatalogError> {
        let invalid = |entry: String| move |source| CatalogError::Invalid { entry, source };
        for player in &self.players {
            player
                .validate()
                .map_err(invalid(player_entry(&player.id)))?;
        }
        for screen in &self.screens {
            screen.validate().map_err(invalid(screen_entry(screen)))?;
        }
        for feature in &self.features {
            feature
                .validate()
                .map_err(invalid(feature_entry(&feature.id)))?;
        }
        for skin in &self.skins {
            skin.validate().map_err(invalid(skin_entry(&skin.id)))?;
        }
        Ok(())
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

    fn check_feature(
        &self,
        feature: &Feature,
        recipes: &BTreeMap<String, RecipeV2>,
    ) -> Result<(), CatalogError> {
        let entry = feature_entry(&feature.id);
        let inconsistent = |problem: String| CatalogError::Inconsistent {
            entry: entry.clone(),
            problem,
        };
        for conflict in &feature.conflicts {
            let other = self
                .feature(conflict)
                .ok_or_else(|| CatalogError::Missing {
                    entry: entry.clone(),
                    what: feature_entry(conflict),
                })?;
            if !other.conflicts.contains(&feature.id) {
                return Err(inconsistent(format!(
                    "it conflicts with {conflict}, which does not list it in its conflicts"
                )));
            }
        }
        for (player_id, implementation) in &feature.implementations {
            let player = self.implementation_player(&entry, player_id)?;
            let needs = |what: String| {
                inconsistent(format!(
                    "its implementation for {player_id} needs {what}, which {player_id} does not \
                     have"
                ))
            };
            for (name, values) in &feature.requires.capabilities {
                let available = player.capabilities.get(name);
                if !available.is_some_and(|capability| capability.includes(values)) {
                    return Err(needs(format!("capability {name} with {values:?}")));
                }
            }
            for slot_ref in &feature.requires.slots {
                if self.slot_count(player, slot_ref).is_none() {
                    return Err(needs(format!("slot {slot_ref}")));
                }
            }
            for (slot_ref, labels) in &feature.labels {
                let count = self.slot_count(player, slot_ref).expect("a required slot");
                check_label_count(&entry, slot_ref, labels.len(), count)?;
            }
            if let Some(skin) = implementation.draws_labels.as_deref() {
                self.check_label_skin(&entry, feature, skin)?;
            }
            check_recipe(&entry, player, &implementation.recipe, recipes)?;
        }
        Ok(())
    }

    /// A feature whose recipe draws its labels in `skin`'s style: the skin exists, and is for the
    /// screen of every labelled slot.
    fn check_label_skin(
        &self,
        entry: &str,
        feature: &Feature,
        skin: &str,
    ) -> Result<(), CatalogError> {
        if skin == STOCK_SKIN {
            return Ok(());
        }
        let found = self.skin(skin).ok_or_else(|| CatalogError::Missing {
            entry: entry.to_owned(),
            what: skin_entry(skin),
        })?;
        for slot_ref in feature.labels.keys() {
            let (screen, _) = slot_ref.split_once('.').expect("validated");
            if found.screen != screen {
                return Err(CatalogError::Inconsistent {
                    entry: entry.to_owned(),
                    problem: format!(
                        "it draws labels for {slot_ref} as skin {skin}, which is for screen {}",
                        found.screen
                    ),
                });
            }
        }
        Ok(())
    }

    fn check_skin(
        &self,
        skin: &Skin,
        recipes: &BTreeMap<String, RecipeV2>,
    ) -> Result<(), CatalogError> {
        let entry = skin_entry(&skin.id);
        for (player_id, implementations) in &skin.implementations {
            let player = self.implementation_player(&entry, player_id)?;
            let screen = player
                .screens
                .contains(&skin.screen)
                .then(|| self.screen(player_id, &skin.screen))
                .flatten()
                .ok_or_else(|| CatalogError::Inconsistent {
                    entry: entry.clone(),
                    problem: format!("player {player_id} has no screen {}", skin.screen),
                })?;
            if player.screen_class != skin.requires.screen_class {
                return Err(CatalogError::Inconsistent {
                    entry: entry.clone(),
                    problem: format!("player {player_id} has another screen class"),
                });
            }
            for slot in &skin.requires.slots {
                if !screen.slots.contains_key(slot) {
                    return Err(CatalogError::Inconsistent {
                        entry: entry.clone(),
                        problem: format!("screen {player_id}/{} has no slot {slot}", skin.screen),
                    });
                }
            }
            for implementation in implementations {
                for (slot, labels) in &implementation.labels {
                    let count = screen.slots[slot].count;
                    let slot_ref = format!("{}.{slot}", skin.screen);
                    check_label_count(&entry, &slot_ref, labels.len(), count)?;
                }
                check_recipe(&entry, player, &implementation.recipe, recipes)?;
            }
        }
        Ok(())
    }

    fn implementation_player(&self, entry: &str, id: &str) -> Result<&Player, CatalogError> {
        self.player(id).ok_or_else(|| CatalogError::Missing {
            entry: entry.to_owned(),
            what: player_entry(id),
        })
    }

    /// The element count of `slot_ref` (`<screen>.<slot name>`) on `player`, if it has the slot.
    fn slot_count(&self, player: &Player, slot_ref: &str) -> Option<u32> {
        let (screen, slot) = slot_ref.split_once('.')?;
        if !player.screens.iter().any(|listed| listed == screen) {
            return None;
        }
        Some(self.screen(&player.id, screen)?.slots.get(slot)?.count)
    }
}

fn check_label_count(
    entry: &str,
    slot_ref: &str,
    labels: usize,
    count: u32,
) -> Result<(), CatalogError> {
    if u32::try_from(labels).is_ok_and(|labels| labels == count) {
        Ok(())
    } else {
        Err(CatalogError::Inconsistent {
            entry: entry.to_owned(),
            problem: format!("{labels} labels for {slot_ref}, which has {count} elements"),
        })
    }
}

/// The recipe at `path` exists, is for `player`'s release with its pins, and pins its output.
fn check_recipe(
    entry: &str,
    player: &Player,
    path: &str,
    recipes: &BTreeMap<String, RecipeV2>,
) -> Result<(), CatalogError> {
    let recipe = recipes.get(path).ok_or_else(|| CatalogError::Missing {
        entry: entry.to_owned(),
        what: format!("recipe {path}"),
    })?;
    let inconsistent = |problem: String| CatalogError::Inconsistent {
        entry: entry.to_owned(),
        problem: format!("recipe {path}: {problem}"),
    };
    let target = &recipe.target;
    if target.release != player.id {
        return Err(inconsistent(format!(
            "it is for release {}, not {}",
            target.release, player.id
        )));
    }
    let pins = [
        (
            "upd_sha256",
            &target.upd_sha256,
            &player.firmware.upd_sha256,
        ),
        (
            "application_sha256",
            &target.application_sha256,
            &player.firmware.application_sha256,
        ),
    ];
    for (field, recipe_pin, player_pin) in pins {
        if !recipe_pin.eq_ignore_ascii_case(player_pin) {
            return Err(inconsistent(format!(
                "target.{field} differs from player {}'s",
                player.id
            )));
        }
    }
    let pinned = recipe.expected.as_ref();
    if pinned.and_then(|e| e.application_sha256.as_ref()).is_none() {
        return Err(inconsistent(
            "it does not pin its output (expected.application_sha256); every fragment of a \
             build must"
                .to_owned(),
        ));
    }
    Ok(())
}

fn player_entry(id: &str) -> String {
    format!("player {id}")
}

fn screen_entry(screen: &Screen) -> String {
    format!("screen {}/{}", screen.player, screen.id)
}

fn feature_entry(id: &str) -> String {
    format!("feature {id}")
}

fn skin_entry(id: &str) -> String {
    format!("skin {id}")
}
