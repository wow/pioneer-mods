//! Assembling a [`Resolution`] from the settled state of [`resolve`](fn@super::resolve).

use super::entry::Maturity;
use super::resolution::{FeatureResolution, Fragment, Resolution, ScreenResolution, Status};
use super::resolve::State;
use super::skin::STOCK_SKIN;
use crate::v2::RecipeV2;
use std::collections::BTreeMap;

impl State<'_> {
    /// The resolution, with each fragment's recipe looked up in `recipes` (the catalog's, which
    /// [`Catalog::check`](super::Catalog::check) found complete).
    pub(super) fn resolution(&self, recipes: &BTreeMap<String, RecipeV2>) -> Resolution {
        let mut screens = Vec::new();
        for screen in self.player.screens.iter().map(String::as_str) {
            let chosen = self.chosen[screen];
            let status = if chosen == STOCK_SKIN {
                Status::Stock
            } else if let Some(reason) = self.skins_off.get(screen) {
                Status::Off {
                    reason: reason.clone(),
                }
            } else {
                let implementation = self.skin_picks[screen];
                on(
                    &implementation.recipe,
                    implementation.maturity,
                    &implementation.limits,
                )
            };
            screens.push(ScreenResolution {
                screen: screen.to_owned(),
                chosen: chosen.to_owned(),
                status,
            });
        }
        for (screen, skin) in &self.profile.screens {
            if !self.chosen.contains_key(screen.as_str()) {
                screens.push(ScreenResolution {
                    screen: screen.clone(),
                    chosen: skin.clone(),
                    status: Status::Off {
                        reason: format!("player {} has no screen {screen}", self.player.id),
                    },
                });
            }
        }
        let features: Vec<FeatureResolution> = self
            .profile
            .features
            .iter()
            .map(|id| {
                let reason = self
                    .features_off
                    .get(id.as_str())
                    .or(self.unfit.get(id.as_str()));
                let status = match (reason, self.picks.get(id.as_str())) {
                    (Some(reason), _) => Status::Off {
                        reason: reason.clone(),
                    },
                    (None, Some(i)) => on(&i.recipe, i.maturity, &i.limits),
                    (None, None) => unreachable!("every feature is picked, unfit or off"),
                };
                FeatureResolution {
                    feature: id.clone(),
                    status,
                }
            })
            .collect();
        let mut fragments: Vec<Fragment> = Vec::new();
        let built = features
            .iter()
            .map(|f| (format!("feature {}", f.feature), &f.status))
            .chain(
                screens
                    .iter()
                    .map(|s| (format!("skin {} on {}", s.chosen, s.screen), &s.status)),
            );
        for (builds, status) in built {
            let Status::On {
                recipe, maturity, ..
            } = status
            else {
                continue;
            };
            match fragments.iter_mut().find(|f| &f.recipe == recipe) {
                Some(fragment) => {
                    fragment.builds.push(builds);
                    fragment.maturity = fragment.maturity.min(*maturity);
                }
                None => {
                    let own = &recipes[recipe];
                    fragments.push(Fragment {
                        recipe: recipe.clone(),
                        maturity: *maturity,
                        builds: vec![builds],
                        as_pinned: own.label == self.profile.label
                            && own.reported_version == self.profile.reported_version,
                    });
                }
            }
        }
        Resolution {
            player: self.player.id.clone(),
            label: self.profile.label.clone(),
            reported_version: self.profile.reported_version.clone(),
            maturity: self.profile.maturity,
            screens,
            features,
            fragments,
        }
    }
}

fn on(recipe: &str, maturity: Maturity, limits: &[String]) -> Status {
    Status::On {
        recipe: recipe.to_owned(),
        maturity,
        limits: limits.to_vec(),
    }
}
