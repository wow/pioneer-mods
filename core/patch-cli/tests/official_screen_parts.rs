//! Owner-input test of the committed screen parts against the official XDJ-700 v1.15 update: every
//! part's touch zone is a record of the application's touch layout holding exactly its area and
//! handler, and every image a part draws is one of the application's images, with its size. Only
//! counts are printed. To run locally:
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD \
//!     cargo test -p patch-cli --test official_screen_parts -- --ignored
//! ```

mod common;
#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use common::repo_root;
use official_pins::UPD_ENV;
use patch_cli::catalog::load_catalog;
use patch_core::upd::parse_upd;
use patch_core::xdj700::decode_application;
use std::collections::BTreeMap;

/// The application's image records (measured, 2026-10-15): 1019 records of 44 bytes from decoded
/// 0x100004, each with its width and height (16-bit) first and, at +0x1C, its pixels' offset from
/// 0x100000.
const IMAGE_TABLE: usize = 0x10_0004;
const IMAGE_RECORD_LEN: usize = 44;
const IMAGE_COUNT: usize = 1019;
const RESOURCE_BASE: u32 = 0x10_0000;

fn u32_at(app: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(app[offset..offset + 4].try_into().expect("four bytes"))
}

fn u16_at(app: &[u8], offset: usize) -> u32 {
    u32::from(u16::from_le_bytes(
        app[offset..offset + 2].try_into().expect("two bytes"),
    ))
}

#[test]
#[ignore = "needs the owner's official XDJ700.UPD (set PIONEER_XDJ700_V115_UPD)"]
fn every_part_is_a_touch_record_and_application_images() {
    let path = std::env::var_os(UPD_ENV)
        .unwrap_or_else(|| panic!("set {UPD_ENV} to an owner-supplied official XDJ700.UPD"));
    let bytes = std::fs::read(path).expect("read the official update");
    let container = parse_upd(&bytes).expect("parse");
    let decoded = decode_application(&container).expect("decode");
    let app = decoded.decoded();

    // Every image of the application, by pixel offset: its width and height.
    let images: BTreeMap<u32, (u32, u32)> = (0..IMAGE_COUNT)
        .map(|id| IMAGE_TABLE + id * IMAGE_RECORD_LEN)
        .map(|record| {
            let size = (u16_at(app, record), u16_at(app, record + 2));
            (RESOURCE_BASE + u32_at(app, record + 0x1c), size)
        })
        .collect();

    let loaded = load_catalog(&repo_root()).expect("committed catalog");
    let (mut zones, mut drawn) = (0, 0);
    for screen in &loaded.catalog().screens {
        for (name, part) in &screen.parts {
            for zone in &part.touch {
                let record = zone.record.0 as usize;
                let words: Vec<u32> = (0..5).map(|k| u32_at(app, record + 4 * k)).collect();
                let area = &zone.area;
                // Compared without printing either side: a failure names the record only.
                assert!(
                    words == [area.x, area.y, area.w, area.h, zone.handler.0],
                    "{}/{name}: the record at {record:#x} differs from the map",
                    screen.id
                );
                zones += 1;
            }
            for place in &part.drawn {
                for image in &place.images {
                    let size = Some(&(image.width, image.height));
                    assert!(
                        images.get(&image.offset.0) == size,
                        "{}/{name}: the image at {:#x} is not one of that size",
                        screen.id,
                        image.offset.0
                    );
                    drawn += 1;
                }
            }
        }
    }
    println!("touch records checked: {zones}; images checked: {drawn}");
    assert!(zones >= 20 && drawn >= 90, "the PERFORM map is checked");
}
