//! The part of [`Catalog::check`] that follows acceptance tests to their players, and
//! implementations to the tests they name.

use super::acceptance::{AcceptanceTest, Step};
use super::check::{Catalog, CatalogError, feature_entry, player_entry, skin_entry};

impl Catalog {
    pub(super) fn check_acceptance(&self) -> Result<(), CatalogError> {
        for test in &self.acceptance {
            self.check_test(test)?;
        }
        let features = self.features.iter().flat_map(|feature| {
            let entry = feature_entry(&feature.id);
            (feature.implementations.iter())
                .flat_map(move |(player, list)| list.iter().map(move |i| (player, &i.acceptance)))
                .map(move |(player, tests)| (entry.clone(), player, tests))
        });
        let skins = self.skins.iter().flat_map(|skin| {
            let entry = skin_entry(&skin.id);
            (skin.implementations.iter())
                .flat_map(move |(player, list)| list.iter().map(move |i| (player, &i.acceptance)))
                .map(move |(player, tests)| (entry.clone(), player, tests))
        });
        for (entry, player, tests) in features.chain(skins) {
            for id in tests {
                let test = self
                    .acceptance_test(id)
                    .ok_or_else(|| CatalogError::Missing {
                        entry: entry.clone(),
                        what: format!("acceptance test {id}"),
                    })?;
                if &test.player != player {
                    return Err(CatalogError::Inconsistent {
                        entry,
                        problem: format!(
                            "its implementation for {player} names acceptance test {id}, which \
                             is for {}",
                            test.player
                        ),
                    });
                }
            }
        }
        Ok(())
    }

    /// The test's player exists and has every control it presses, and its touches and regions
    /// lie on that player's screen.
    fn check_test(&self, test: &AcceptanceTest) -> Result<(), CatalogError> {
        let entry = format!("acceptance test {}", test.id);
        let player = self
            .player(&test.player)
            .ok_or_else(|| CatalogError::Missing {
                entry: entry.clone(),
                what: player_entry(&test.player),
            })?;
        let (width, height) = (player.screen_class.width, player.screen_class.height);
        let off_screen = |index: usize, what: &str| CatalogError::Inconsistent {
            entry: entry.clone(),
            problem: format!(
                "steps[{index}]: {what} is not on {}'s {width}x{height} screen",
                player.id
            ),
        };
        for (index, step) in test.steps.iter().enumerate() {
            match step {
                Step::Press { control, .. } if !player.controls.contains_key(control) => {
                    return Err(CatalogError::Missing {
                        entry,
                        what: format!("control {control} of {}", player_entry(&player.id)),
                    });
                }
                Step::Touch { x, y, .. } if *x >= width || *y >= height => {
                    return Err(off_screen(index, "the touch"));
                }
                Step::Expect(check) | Step::WaitUntil { check, .. }
                    if check.address().is_some_and(|address| {
                        let bytes = check.bytes();
                        !player.state_memory.iter().any(|r| r.holds(address, bytes))
                    }) =>
                {
                    return Err(CatalogError::Inconsistent {
                        entry,
                        problem: format!(
                            "steps[{index}]: the memory check reads outside {}'s run-time state \
                             ranges (state_memory); a test never reads the loaded application",
                            player.id
                        ),
                    });
                }
                Step::Expect(check) | Step::WaitUntil { check, .. }
                    if !check.regions().iter().all(|r| r.fits(width, height)) =>
                {
                    return Err(off_screen(index, "a region"));
                }
                _ => {}
            }
        }
        Ok(())
    }
}
