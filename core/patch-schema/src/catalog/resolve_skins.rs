//! How kept skins meet the features that leave their labels to them, for
//! [`resolve`](fn@super::resolve): one configuration (a set of kept skins and an implementation
//! each), and the reasons for what it leaves off.

use super::feature::FeatureImplementation;
use super::resolve::{Skins, State, labelled_screens, subsets};
use super::skin::SkinImplementation;
use std::collections::BTreeMap;

/// Labels by slot name on one screen.
type Labels = BTreeMap<String, Vec<String>>;

/// A configuration that works: the kept skins (screens, in screen order), the features on with
/// their implementations, why the others are off, and each kept skin's implementation.
pub(super) struct Configuration<'a> {
    pub(super) kept: Vec<&'a str>,
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
            let covers = kept.iter().zip(&choice).all(|(screen, implementation)| {
                let mut covered = Labels::new();
                for id in &drawn {
                    covered.extend(self.labels_on(id, screen));
                }
                covered == implementation.labels
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
            kept: kept.to_vec(),
            picks,
            unfit,
            skin_picks: kept.iter().copied().zip(choice).collect(),
        })
    }

    /// Whether `implementation` draws every label feature `id` gives on `screen`.
    fn draws(&self, implementation: &SkinImplementation, id: &str, screen: &str) -> bool {
        self.labels_on(id, screen)
            .iter()
            .all(|(slot, labels)| implementation.labels.get(slot) == Some(labels))
    }

    /// The labels feature `id` gives on `screen`, by slot name.
    fn labels_on(&self, id: &str, screen: &str) -> Labels {
        let feature = self.catalog.feature(id).expect("available");
        feature
            .labels
            .iter()
            .filter_map(|(slot_ref, labels)| {
                let (on, slot) = slot_ref.split_once('.')?;
                (on == screen).then(|| (slot.to_owned(), labels.clone()))
            })
            .collect()
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
        let feature = self.catalog.feature(id).expect("available");
        let stock_style = feature.implementations[&self.player.id]
            .iter()
            .any(|i| self.accepts(i.maturity) && i.draws_labels.contains_key(screen));
        let hint = if stock_style {
            format!(
                "; it has an implementation drawing them in the stock style, for the stock skin \
                 on {screen}"
            )
        } else {
            String::new()
        };
        format!("skin {skin} on {screen} {what}{hint}")
    }

    /// Why the skin chosen on `screen` is off when `best`, found among the sets of `size` of the
    /// `candidates`, is kept: the rule that decided, when the skin could be kept alone.
    pub(super) fn dropped(
        &self,
        screen: &'a str,
        best: &Configuration<'a>,
        candidates: &[&'a str],
        size: usize,
    ) -> String {
        let skin = self.chosen[screen];
        if self.configuration(&[screen]).is_some() {
            let kept: Vec<String> = best
                .kept
                .iter()
                .map(|s| format!("skin {} on {s}", self.chosen[s]))
                .collect();
            // The best configuration of the same size that keeps this skin, if any works.
            let with = subsets(candidates, size)
                .into_iter()
                .filter(|set| set.contains(&screen))
                .filter_map(|set| self.configuration(&set))
                .map(|configuration| configuration.on())
                .max();
            let why = match with {
                None => "keeps more of the chosen skins",
                Some(on) if on < best.on() => "keeps more of the chosen features",
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
