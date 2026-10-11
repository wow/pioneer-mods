//! The emulated NOR flash of an update: erased flash holding the MAIN document's data records at
//! their addresses, as the player's updater lays them out (loader at 0, application above it).

use anyhow::{Context, Result, bail};
use patch_core::upd::parse_upd;
use patch_core::xdj700::main_document;

/// Builds a `size`-byte flash image (erased: `0xFF`) holding `update`'s MAIN records.
///
/// # Errors
///
/// An update that does not parse, has no single XDJ-700 MAIN document, or holds a record past
/// the end of the flash.
pub fn flash_image(update: &[u8], size: usize) -> Result<Vec<u8>> {
    let container = parse_upd(update).context("the update does not parse")?;
    let main = main_document(&container).context("the update has no XDJ-700 MAIN document")?;
    let mut flash = vec![0xFF; size];
    for record in main.data_records() {
        let start = record.address() as usize;
        let end = start + record.data().len();
        if end > size {
            bail!(
                "the update's MAIN record at {start:#x} ends past the {size}-byte flash (at \
                 {end:#x})"
            );
        }
        flash[start..end].copy_from_slice(record.data());
    }
    Ok(flash)
}
