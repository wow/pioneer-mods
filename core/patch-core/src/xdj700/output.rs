//! The identities of a recipe's output and what `expected` declares for each. Applying a recipe
//! refuses an output that differs, and `precondition` reports each status, both from the same
//! list ([`OutputIdentities::pins`]), so a new identity is compared everywhere at once.

use super::rebuild::RebuiltUpdate;
use patch_schema::ExpectedV2;

/// The identities of a recipe's output, as `expected` declares them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutputIdentities {
    /// SHA-256 of the decoded output application.
    pub application_sha256: String,
    /// SHA-256 of the complete output `.UPD`.
    pub upd_sha256: String,
}

/// One output identity, next to the value the recipe declares for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OutputPin<'a> {
    /// The field under `expected`.
    pub field: &'static str,
    /// The declared value, if any.
    pub declared: Option<&'a str>,
    /// The output's value.
    pub actual: &'a str,
}

impl OutputPin<'_> {
    /// Whether the recipe declares a value and it differs from the output's (compared without
    /// regard to case).
    pub fn differs(&self) -> bool {
        self.declared
            .is_some_and(|declared| !declared.eq_ignore_ascii_case(self.actual))
    }
}

impl OutputIdentities {
    /// The identities of `rebuilt`.
    pub fn of(rebuilt: &RebuiltUpdate) -> Self {
        Self {
            application_sha256: rebuilt.application_sha256().to_owned(),
            upd_sha256: rebuilt.sha256().to_owned(),
        }
    }

    /// Every identity `expected` can declare, in field order, next to its declared value.
    pub fn pins<'a>(&'a self, expected: Option<&'a ExpectedV2>) -> [OutputPin<'a>; 2] {
        // An exhaustive pattern, so a field added to `ExpectedV2` fails to compile here.
        let (application, upd) = match expected {
            Some(ExpectedV2 {
                application_sha256,
                upd_sha256,
            }) => (application_sha256.as_deref(), upd_sha256.as_deref()),
            None => (None, None),
        };
        [
            OutputPin {
                field: "application_sha256",
                declared: application,
                actual: &self.application_sha256,
            },
            OutputPin {
                field: "upd_sha256",
                declared: upd,
                actual: &self.upd_sha256,
            },
        ]
    }
}
