//! The parts of [`Catalog::check`] that follow a feature's or skin's implementations to their
//! players and recipes.

use super::check::{Catalog, CatalogError, feature_entry, skin_entry};
use super::feature::Feature;
use super::player::Player;
use super::skin::{STOCK_SKIN, Skin};
use crate::v2::RecipeV2;
use std::collections::BTreeMap;

impl Catalog {
    pub(super) fn check_feature(
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
        for (player_id, implementations) in &feature.implementations {
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
                // Validation keeps every relabelled slot in `requires.slots`, checked above.
                let count = self.slot_count(player, slot_ref).expect("a required slot");
                check_label_count(&entry, slot_ref, labels.len(), count)?;
            }
            for implementation in implementations {
                if let Some(skin) = implementation.draws_labels.as_deref() {
                    self.check_label_skin(&entry, feature, skin)?;
                }
                check_recipe(&entry, player, &implementation.recipe, recipes)?;
            }
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

    pub(super) fn check_skin(
        &self,
        skin: &Skin,
        recipes: &BTreeMap<String, RecipeV2>,
    ) -> Result<(), CatalogError> {
        let entry = skin_entry(&skin.id);
        let inconsistent = |problem: String| CatalogError::Inconsistent {
            entry: entry.clone(),
            problem,
        };
        for (player_id, implementations) in &skin.implementations {
            let player = self.implementation_player(&entry, player_id)?;
            let screen = player
                .screens
                .contains(&skin.screen)
                .then(|| self.screen(player_id, &skin.screen))
                .flatten()
                .ok_or_else(|| {
                    inconsistent(format!("player {player_id} has no screen {}", skin.screen))
                })?;
            if player.screen_class != skin.requires.screen_class {
                return Err(inconsistent(format!(
                    "player {player_id} has another screen class"
                )));
            }
            for slot in &skin.requires.slots {
                if !screen.slots.contains_key(slot) {
                    return Err(inconsistent(format!(
                        "screen {player_id}/{} has no slot {slot}",
                        skin.screen
                    )));
                }
            }
            for implementation in implementations {
                for (slot, labels) in &implementation.labels {
                    // Validation keeps every labelled slot in `requires.slots`, checked above.
                    let count = screen.slots[slot].count;
                    let slot_ref = format!("{}.{slot}", skin.screen);
                    check_label_count(&entry, &slot_ref, labels.len(), count)?;
                }
                check_recipe(&entry, player, &implementation.recipe, recipes)?;
            }
        }
        Ok(())
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
