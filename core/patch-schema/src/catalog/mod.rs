//! The catalog: players, their screens, features, skins and acceptance tests, as data
//! (`catalog/` in the repository), and the owner's profile. The format and rules are in
//! `docs/catalog.md` and `docs/acceptance.md`; the design they implement is
//! `docs/modular-builds.md`.
//!
//! Each file is checked on its own (`validate`), and the catalog as a whole against itself and
//! the recipes its implementations name ([`Catalog::check`]). Neither needs firmware: whether a
//! recipe applies, and what a composed build contains, is for the engine in `patch-core`.

mod acceptance;
mod check;
mod check_acceptance;
mod check_parts;
mod entry;
mod feature;
mod player;
mod profile;
mod resolution;
mod resolve;
mod resolve_output;
mod resolve_skins;
mod screen;
mod skin;

pub use acceptance::{
    AcceptanceTest, Check, MAX_HOLD_MS, MAX_OTHER_REGIONS, MAX_STEPS, MAX_TOTAL_WAIT_MS,
    MAX_WAIT_MS, Region, Step, parse_address,
};
pub use check::{Catalog, CatalogError, CheckedCatalog};
pub use entry::{
    CATALOG_SCHEMA_VERSION, CatalogEntry, EntryError, MAX_ID_LEN, MAX_LABEL_LEN, Maturity, is_id,
    is_name, is_player_id, split_slot_ref,
};
pub use feature::{Feature, FeatureImplementation, Requires};
pub use player::{
    Budgets, Capability, Control, Firmware, PixelFormat, Player, Restore, ScreenClass,
};
pub use profile::Profile;
pub use resolution::{
    BuildError, FeatureResolution, Fragment, Resolution, ResolveError, ScreenResolution, Status,
};
pub use resolve::{MAX_CONFIGURATIONS, resolve};
pub use screen::{Screen, Slot};
pub use skin::{Art, STOCK_SKIN, Skin, SkinImplementation, SkinLicence, SkinRequires};
