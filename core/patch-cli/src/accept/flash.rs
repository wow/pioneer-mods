//! The emulated NOR flash of an update: erased flash holding the MAIN document's image at its
//! address, as the player's updater lays it out (loader at 0, application above it), and the
//! check that an update is of the test's release.

use anyhow::{Context, Result, bail};
use patch_core::upd::{GAP_FILL, parse_upd};
use patch_core::xdj700::{decode_application, main_document, recipe_target};

/// Builds a `size`-byte flash image, erased (`GAP_FILL`), holding `update`'s MAIN image (its
/// records, with patch-core's own gap fill) at its base address.
///
/// # Errors
///
/// An update that does not parse, has no single XDJ-700 MAIN document, or whose MAIN image ends
/// past the end of the flash.
pub fn flash_image(update: &[u8], size: usize) -> Result<Vec<u8>> {
    let container = parse_upd(update).context("the update does not parse")?;
    let main = main_document(&container).context("the update has no XDJ-700 MAIN document")?;
    let image = main
        .image()
        .context("the update's MAIN records do not form an image")?;
    let start = usize::try_from(image.base()).context("the MAIN image's base is too large")?;
    let end = start + image.bytes().len();
    if end > size {
        bail!("the update's MAIN image ends past the {size}-byte flash (at {end:#x})");
    }
    let mut flash = vec![GAP_FILL; size];
    flash[start..end].copy_from_slice(image.bytes());
    Ok(flash)
}

/// Refuses an update of another release than `release`: every update of a release (its official
/// file, stage files, builds) keeps the release's decoded application length, by the recipes'
/// same-length rule, so another release's file is caught without restricting builds.
///
/// # Errors
///
/// An unknown release, an application that does not decode, or one of another length.
pub fn check_release(update: &[u8], release: &str) -> Result<()> {
    let target = recipe_target(release).with_context(|| format!("unknown release {release}"))?;
    let container = parse_upd(update).context("the update does not parse")?;
    let decoded =
        decode_application(&container).context("the update's application does not decode")?;
    let len = decoded.decoded().len();
    if len != target.application_len {
        bail!(
            "the update is not of release {release}: its application is {len} bytes, not {}",
            target.application_len
        );
    }
    Ok(())
}
