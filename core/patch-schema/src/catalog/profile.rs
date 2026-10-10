//! A profile: the owner's choices for one build, small and shareable, kept apart from the
//! catalog so that they survive a firmware port.

use super::entry::{
    EntryError, Maturity, check_id, check_player_id, check_schema_version, check_unique,
};
use crate::bare_version_number;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub schema_version: u32,
    pub player: String,
    /// The skin for each screen, by screen id; a screen not listed keeps the `stock` skin.
    #[serde(default)]
    pub screens: BTreeMap<String, String>,
    #[serde(default)]
    pub features: Vec<String>,
    /// MAIN version label of the build (`VerX.YY`).
    pub label: String,
    /// Version the built application reports (`X.YY`).
    pub reported_version: String,
    /// The least settled implementation the owner accepts: `stable` (the default) or
    /// `experimental`; `dev` is never offered.
    #[serde(default = "stable")]
    pub maturity: Maturity,
}

fn stable() -> Maturity {
    Maturity::Stable
}

impl Profile {
    /// Checks the file on its own; whether the player's release accepts the label and version is
    /// for the engine to say.
    ///
    /// # Errors
    ///
    /// The first [`EntryError`] found.
    pub fn validate(&self) -> Result<(), EntryError> {
        check_schema_version(self.schema_version)?;
        check_player_id("player", &self.player)?;
        for (screen, skin) in &self.screens {
            check_id("screens", screen)?;
            check_id(&format!("screens.{screen}"), skin)?;
        }
        for feature in &self.features {
            check_id("features", feature)?;
        }
        check_unique("features", &self.features)?;
        if !self
            .label
            .strip_prefix("Ver")
            .is_some_and(|version| bare_version_number(version).is_some())
        {
            return Err(EntryError::Rule {
                field: "label".to_owned(),
                problem: format!("{:?} is not of the form VerX.YY", self.label),
            });
        }
        if bare_version_number(&self.reported_version).is_none() {
            return Err(EntryError::Rule {
                field: "reported_version".to_owned(),
                problem: format!("{:?} is not of the form X.YY", self.reported_version),
            });
        }
        if self.maturity == Maturity::Dev {
            return Err(EntryError::Rule {
                field: "maturity".to_owned(),
                problem: "dev implementations are never offered; use stable or experimental"
                    .to_owned(),
            });
        }
        Ok(())
    }
}
