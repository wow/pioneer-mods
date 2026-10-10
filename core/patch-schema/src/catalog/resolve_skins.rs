//! How a chosen skin meets the features that leave their labels to it, for
//! [`resolve`](fn@super::resolve).

use super::resolve::{Labels, State, labelled_screens};
use super::skin::SkinImplementation;

impl<'a> State<'a> {
    /// The picked features that give labels on `screen` (all leave them to its skin).
    pub(super) fn leaving_labels(&self, screen: &str) -> Vec<&'a str> {
        self.picks
            .keys()
            .copied()
            .filter(|id| {
                let feature = self.catalog.feature(id).expect("available");
                labelled_screens(feature).any(|labelled| labelled == screen)
            })
            .collect()
    }

    /// The accepted implementation of `skin` drawing exactly the labels of the most features in
    /// `ready` (the first on a tie), and those features.
    pub(super) fn best_drawing(
        &self,
        skin: &str,
        screen: &str,
        ready: &[&'a str],
    ) -> Option<(&'a SkinImplementation, Vec<&'a str>)> {
        let skin = self.catalog.skin(skin).expect("available");
        let mut best: Option<(&'a SkinImplementation, Vec<&'a str>)> = None;
        for implementation in &skin.implementations[&self.player.id] {
            if !self.accepts(implementation.maturity) {
                continue;
            }
            let drawn: Vec<&'a str> = ready
                .iter()
                .copied()
                .filter(|id| {
                    self.labels_on(id, screen)
                        .iter()
                        .all(|(slot, labels)| implementation.labels.get(slot) == Some(labels))
                })
                .collect();
            let mut covered = Labels::new();
            for id in &drawn {
                covered.extend(self.labels_on(id, screen));
            }
            let more = best
                .as_ref()
                .is_none_or(|(_, most)| drawn.len() > most.len());
            if covered == implementation.labels && more {
                best = Some((implementation, drawn));
            }
        }
        best
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

    /// Why feature `id` is off when `skin` on `screen` does not draw its labels.
    pub(super) fn undrawn(&self, id: &str, screen: &str, skin: &str) -> String {
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
        format!(
            "skin {skin} on {screen} has no implementation for {} drawing its labels{hint}",
            self.player.id
        )
    }
}
