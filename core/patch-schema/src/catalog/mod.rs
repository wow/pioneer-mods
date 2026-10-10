//! The catalog: players, their screens, features and skins, as data (`catalog/` in the
//! repository), and the owner's profile. The format and rules are in `docs/catalog.md`; the design
//! they implement is `docs/modular-builds.md`.
//!
//! Each file is checked on its own (`validate`), and the catalog as a whole against itself and
//! the recipes its implementations name ([`Catalog::check`]). Neither needs firmware: whether a
//! recipe applies, and what a composed build contains, is for the engine in `patch-core`.

mod check;
mod check_parts;
mod entry;
mod feature;
mod player;
mod profile;
mod resolution;
mod resolve;
mod resolve_output;
mod screen;
mod skin;

pub use check::{Catalog, CatalogError};
pub use entry::{
    CATALOG_SCHEMA_VERSION, CatalogEntry, EntryError, MAX_ID_LEN, MAX_LABEL_LEN, Maturity, is_id,
    is_name, is_player_id, split_slot_ref,
};
pub use feature::{Feature, FeatureImplementation, Requires};
pub use player::{Budgets, Capability, Firmware, PixelFormat, Player, ScreenClass};
pub use profile::Profile;
pub use resolution::{
    FeatureResolution, Fragment, Resolution, ResolveError, ScreenResolution, Status,
};
pub use resolve::resolve;
pub use screen::{Screen, Slot};
pub use skin::{Art, STOCK_SKIN, Skin, SkinImplementation, SkinRequires};
