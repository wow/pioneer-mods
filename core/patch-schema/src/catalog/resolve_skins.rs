//! How kept skins meet the features that leave their labels to them, for
//! [`resolve`](fn@super::resolve): one configuration (a set of kept skins and an implementation
//! each), and the reasons for what it leaves off.

use super::feature::FeatureImplementation;
use super::resolve::{Skins, State, labelled_screens};
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
            let (screen, _) = kept
                .iter()
                .zip(&choice)
                .find(|(screen, implementation)| !self.draws(implementation, id, screen))
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

    /// Why feature `id` is off when the skin kept on `screen` does not draw its labels.
    fn undrawn(&self, id: &str, screen: &str) -> String {
        let skin = self.chosen[screen];
        let drawable = self
            .skin_implementations(screen)
            .iter()
            .any(|implementation| self.draws(implementation, id, screen));
        let what = if drawable {
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

    /// Why the skin chosen on `screen` is off when the skins on `kept` are kept.
    pub(super) fn dropped(&self, screen: &'a str, kept: &[&'a str]) -> String {
        let skin = self.chosen[screen];
        if self.configuration(&[screen]).is_some() {
            let others: Vec<String> = kept
                .iter()
                .map(|s| format!("skin {} on {s}", self.chosen[s]))
                .collect();
            return format!(
                "the chosen skins cannot all be kept; keeping {} keeps more of the chosen \
                 features",
                others.join(" and ")
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
