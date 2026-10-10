//! How kept skins meet the features that leave their labels to them, for
//! [`resolve`](fn@super::resolve): one configuration (a set of kept skins and an implementation
//! each), and the reasons for what it leaves off.

use super::feature::FeatureImplementation;
use super::resolve::{Labels, Skins, State, labelled_screens};
use super::skin::SkinImplementation;
use std::collections::BTreeMap;

/// A configuration that works: the features on with their implementations, why the others are
/// off, and each kept skin's implementation.
pub(super) struct Configuration<'a> {
    pub(super) picks: BTreeMap<&'a str, &'a FeatureImplementation>,
    pub(super) unfit: BTreeMap<&'a str, String>,
    pub(super) skin_picks: BTreeMap<&'a str, &'a SkinImplementation>,
}

impl Configuration<'_> {
    /// How many features are on.
    pub(super) fn on(&self) -> usize {
        self.picks.len()
    }
}

impl<'a> State<'a> {
    /// The accepted implementations, for the player, of the skin chosen on `screen`.
    pub(super) fn skin_implementations(&self, screen: &str) -> Vec<&'a SkinImplementation> {
        let skin = self.catalog.skin(self.chosen[screen]).expect("available");
        skin.implementations[&self.player.id]
            .iter()
            .filter(|i| self.accepts(i.maturity))
            .collect()
    }

    /// The best configuration keeping the skins chosen on `kept`, if one works: the choice of
    /// one implementation per kept skin that draws exactly the labels of the most features (the
    /// first on a tie).
    pub(super) fn configuration(&self, kept: &[&'a str]) -> Option<Configuration<'a>> {
        let skins: Skins<'a> = kept.iter().map(|s| (*s, self.chosen[s])).collect();
        let mut picks = BTreeMap::new();
        let mut unfit = BTreeMap::new();
        for id in &self.profile.features {
            if self.features_off.contains_key(id.as_str()) {
                continue;
            }
            let feature = self.catalog.feature(id).expect("available");
            match self.pick(feature, &skins) {
                Ok(implementation) => {
                    picks.insert(id.as_str(), implementation);
                }
                Err(reason) => {
                    unfit.insert(id.as_str(), reason);
                }
            }
        }
        let lists: Vec<Vec<&'a SkinImplementation>> =
            kept.iter().map(|s| self.skin_implementations(s)).collect();
        let mut best: Option<(Vec<&'a SkinImplementation>, Vec<&'a str>)> = None;
        for choice in product(&lists) {
            let drawn: Vec<&'a str> = picks
                .keys()
                .copied()
                .filter(|id| {
                    kept.iter()
                        .zip(&choice)
                        .all(|(screen, implementation)| self.draws(implementation, id, screen))
                })
                .collect();
            // Each drawn feature's labels are among the implementation's, and picked features
            // relabel different slots (two that share one conflict, so neither is picked): the
            // implementation draws exactly theirs when it has as many slots as they give.
            let covers = kept.iter().zip(&choice).all(|(screen, implementation)| {
                let given: usize = drawn
                    .iter()
                    .map(|id| self.labels_on(id, screen).map_or(0, Labels::len))
                    .sum();
                given == implementation.labels.len()
            });
            if covers
                && best
                    .as_ref()
                    .is_none_or(|(_, most)| drawn.len() > most.len())
            {
                best = Some((choice, drawn));
            }
        }
        let (choice, drawn) = best?;
        let undrawn: Vec<&'a str> = picks
            .keys()
            .copied()
            .filter(|id| !drawn.contains(id))
            .collect();
        for id in undrawn {
            // The kept screens that leave it out; the reason names one whose skin cannot draw it at
            // all, when there is one, since that holds whichever implementation were chosen.
            let leaving: Vec<&'a str> = kept
                .iter()
                .zip(&choice)
                .filter(|(screen, implementation)| !self.draws(implementation, id, screen))
                .map(|(screen, _)| *screen)
                .collect();
            let screen = leaving
                .iter()
                .find(|screen| !self.drawable(id, screen))
                .or(leaving.first())
                .expect("a kept skin leaves it out");
            picks.remove(id);
            unfit.insert(id, self.undrawn(id, screen));
        }
        Some(Configuration {
            picks,
            unfit,
            skin_picks: kept.iter().copied().zip(choice).collect(),
        })
    }

    /// Whether `implementation` draws every label feature `id` gives on `screen`.
    fn draws(&self, implementation: &SkinImplementation, id: &str, screen: &str) -> bool {
        self.labels_on(id, screen).is_none_or(|labels| {
            labels
                .iter()
                .all(|(slot, labels)| implementation.labels.get(slot) == Some(labels))
        })
    }

    /// The labels feature `id` gives on `screen`, by slot name, if any.
    fn labels_on(&self, id: &str, screen: &str) -> Option<&Labels> {
        self.labels.get(id)?.get(screen)
    }

    /// Whether some accepted implementation of the skin chosen on `screen` draws the labels
    /// feature `id` gives there.
    fn drawable(&self, id: &str, screen: &str) -> bool {
        self.skin_implementations(screen)
            .iter()
            .any(|implementation| self.draws(implementation, id, screen))
    }

    /// Why feature `id` is off when the skin kept on `screen` does not draw its labels.
    fn undrawn(&self, id: &str, screen: &str) -> String {
        let skin = self.chosen[screen];
        let what = if self.drawable(id, screen) {
            "does not draw its labels in the implementation that keeps the most of the chosen \
             features"
                .to_owned()
        } else {
            format!(
                "has no implementation for {} drawing its labels",
                self.player.id
            )
        };
        // An accepted implementation drawing this screen's labels in the stock style, and every
        // screen it needs on the stock skin.
        let feature = self.catalog.feature(id).expect("available");
        let stock_style = feature.implementations[&self.player.id]
            .iter()
            .find(|i| self.accepts(i.maturity) && i.draws_labels.contains_key(screen));
        let hint = stock_style.map_or_else(String::new, |implementation| {
            let screens: Vec<&str> = implementation
                .draws_labels
                .keys()
                .map(String::as_str)
                .collect();
            format!(
                "; it has an implementation drawing them in the stock style, for the stock skin \
                 on {}",
                screens.join(" and ")
            )
        });
        format!("skin {skin} on {screen} {what}{hint}")
    }

    /// Why the skin chosen on `screen` is off when the skins on `kept` are kept, with `best_on`
    /// features on, and `with` features at best among the sets of the same size that keep it
    /// (none working: `None`): the rule that decided, when the skin could be kept alone.
    pub(super) fn dropped(
        &self,
        screen: &'a str,
        kept: &[&'a str],
        best_on: usize,
        with: Option<usize>,
    ) -> String {
        let skin = self.chosen[screen];
        if self.configuration(&[screen]).is_some() {
            let kept: Vec<String> = kept
                .iter()
                .map(|s| format!("skin {} on {s}", self.chosen[s]))
                .collect();
            let why = match with {
                None => "keeps more of the chosen skins",
                Some(on) if on < best_on => "keeps more of the chosen features",
                Some(_) => "keeps as many of the chosen features and comes first in screen order",
            };
            return format!(
                "the chosen skins cannot all be kept; keeping {} {why}",
                kept.join(" and ")
            );
        }
        let alone: Skins<'a> = [(screen, skin)].into();
        let leaves_labels = self.profile.features.iter().any(|id| {
            !self.features_off.contains_key(id.as_str())
                && self.catalog.feature(id).is_some_and(|feature| {
                    self.pick(feature, &alone).is_ok()
                        && labelled_screens(feature).any(|labelled| labelled == screen)
                })
        });
        if leaves_labels {
            format!(
                "skin {skin} has no implementation for {} drawing the labels of some of the \
                 chosen features, or none",
                self.player.id
            )
        } else {
            format!(
                "skin {skin} has no implementation for {} without a feature's labels",
                self.player.id
            )
        }
    }
}

/// Every choice of one item from each of `lists`, in order.
fn product<T: Copy>(lists: &[Vec<T>]) -> Vec<Vec<T>> {
    lists.iter().fold(vec![Vec::new()], |choices, list| {
        choices
            .into_iter()
            .flat_map(|choice| {
                list.iter().map(move |&item| {
                    let mut longer = choice.clone();
                    longer.push(item);
                    longer
                })
            })
            .collect()
    })
}
