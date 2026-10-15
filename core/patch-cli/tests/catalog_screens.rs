//! The committed screens' slot images (`catalog/screens/`): they agree with the committed recipes
//! that edit them, lie on the player's screen, in its application and out of its protected
//! ranges, and no two overlap. Needs no firmware.

mod common;

use common::{copy_catalog, repo_root, write_bytes};
use patch_cli::catalog::load_catalog;
use patch_core::xdj700::recipe_target;
use serde_json::Value;
use std::collections::BTreeSet;
use std::path::Path;

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&std::fs::read(path).expect("read")).expect("json")
}

const PERFORM: &str = "catalog/screens/xdj700-v1.15/perform.json";
const STAGE_7: &str = "recipes/xdj700-v1.15/beat-loop-1-to-32.json";

#[test]
fn the_perform_pads_are_the_images_stage_7_relabels() {
    let loaded = load_catalog(&repo_root()).expect("committed catalog");
    let screen = loaded
        .catalog()
        .screen("xdj700-v1.15", "perform")
        .expect("perform");
    let images = screen.slots["beat_loop.pad"]
        .images
        .as_ref()
        .expect("pad images");
    assert_eq!(
        (images.width, images.height, images.states.len()),
        (80, 53, 6)
    );

    // Stage 7 edits exactly the slot's 36 images, each whole.
    let recipe = read_json(&repo_root().join(STAGE_7));
    let edits = recipe["image_edits"].as_array().expect("edits");
    let edited: BTreeSet<u64> = edits
        .iter()
        .map(|edit| {
            let size = (edit["width"].as_u64(), edit["height"].as_u64());
            assert_eq!(size, (Some(80), Some(53)), "{edit}");
            edit["offset"].as_u64().expect("offset")
        })
        .collect();
    let named: BTreeSet<u64> = images.images().map(|i| u64::from(i.offset)).collect();
    assert_eq!((edited.len(), named.len()), (36, 36));
    assert_eq!(edited, named);

    // Left to right, one row.
    let xs: Vec<u32> = images.elements.iter().map(|e| e.at.x).collect();
    assert!(
        xs.windows(2).all(|pair| pair[0] + images.width <= pair[1]),
        "{xs:?}"
    );
}

/// Loads a catalog copy after `edit` changes it; returns the refusal, if any.
fn load_edited(edit: impl FnOnce(&Path)) -> Option<String> {
    let root = copy_catalog();
    edit(root.path());
    load_catalog(root.path())
        .err()
        .map(|error| format!("{error:#}"))
}

/// Changes the committed PERFORM pads' images entry in a catalog copy.
fn edit_pads(root: &Path, edit: impl FnOnce(&mut Value)) {
    let path = root.join(PERFORM);
    let mut screen = read_json(&path);
    edit(&mut screen["slots"]["beat_loop.pad"]["images"]);
    write_bytes(&path, &serde_json::to_vec(&screen).expect("json"));
}

fn refused_with(edit: impl FnOnce(&Path), problem: &str) {
    let message = load_edited(edit).expect("refused");
    assert!(message.contains(problem), "{problem}: {message}");
}

/// Moves the last pad's normal image to `offset` (Stage 7 then edits a stock offset that no slot
/// image covers, which is allowed). The Neon pads skin, which paints the image where it was, is
/// left out: a skin paints only its slots' images.
fn last_pad_normal_at(offset: u32) -> impl FnOnce(&Path) {
    move |root: &Path| {
        std::fs::remove_file(root.join("catalog/skins/perform-neon-pads.json")).expect("skin");
        edit_pads(root, |i| {
            i["elements"][5]["images"]["normal"] = format!("{offset:#x}").into();
        });
    }
}

#[test]
fn an_element_off_the_screen_or_an_image_outside_the_application_is_refused() {
    // The screen is 800x480: the last pad moved to x 721 ends at 801.
    refused_with(
        |root| edit_pads(root, |i| i["elements"][5]["at"]["x"] = 721.into()),
        "slot beat_loop.pad: elements[5] is not on the 800x480 screen",
    );
    // The application is 18,601,864 bytes: an image in its last 8480 bytes loads, two bytes later
    // it ends past it.
    let last = 18_601_864 - 8480;
    assert_eq!(load_edited(last_pad_normal_at(last)), None);
    refused_with(
        last_pad_normal_at(last + 2),
        "slot beat_loop.pad's image at 0x11bb66a ends past the 18601864-byte application",
    );
    // The release's first protected range (its start-up code).
    let protected = recipe_target("xdj700-v1.15").expect("release").protected[0].clone();
    let inside = u32::try_from(protected.start).expect("u32") & !1;
    refused_with(
        last_pad_normal_at(inside),
        &format!(
            "image at {inside:#x} overlaps the release's protected range {:#x}..{:#x}",
            protected.start, protected.end
        ),
    );
}

#[test]
fn a_recipe_edits_a_slot_image_whole_and_no_two_slot_images_overlap() {
    // Stage 7's first edit, one pixel narrower: it overlaps a pad image but is not that image.
    refused_with(
        |root| {
            let path = root.join(STAGE_7);
            let mut recipe = read_json(&path);
            recipe["image_edits"][0]["width"] = 79.into();
            write_bytes(&path, &serde_json::to_vec(&recipe).expect("json"));
        },
        "image_edits[0] (79x53 at 0x34f940) overlaps screen xdj700-v1.15/perform: slot \
         beat_loop.pad's image at 0x34f940 (80x53) but is not that image",
    );
    // A second slot naming the first pad's images again, elsewhere on the screen.
    refused_with(
        |root| {
            let path = root.join(PERFORM);
            let mut screen = read_json(&path);
            let mut copy = screen["slots"]["beat_loop.pad"].clone();
            copy["count"] = 1.into();
            let first = copy["images"]["elements"][0].clone();
            copy["images"]["elements"] = Value::Array(vec![first]);
            copy["images"]["elements"][0]["at"]["y"] = 300.into();
            screen["slots"]["beat_loop.copy"] = copy;
            write_bytes(&path, &serde_json::to_vec(&screen).expect("json"));
        },
        "the images of perform.beat_loop.copy elements[0] normal (0x360240) and \
         perform.beat_loop.pad elements[0] normal (0x360240) overlap",
    );
}

#[test]
fn the_neon_pads_skin_paints_every_pad_image_once_and_whole() {
    let loaded = load_catalog(&repo_root()).expect("committed catalog");
    let catalog = loaded.catalog();
    let skin = catalog
        .skins
        .iter()
        .find(|skin| skin.id == "perform-neon-pads")
        .expect("skin");
    let path = &skin.implementations["xdj700-v1.15"][0].recipe;
    let recipe = read_json(&repo_root().join(path));
    let images = catalog
        .screen("xdj700-v1.15", "perform")
        .expect("perform")
        .slots["beat_loop.pad"]
        .images
        .as_ref()
        .expect("pad images");

    // The loader holds a skin's recipe to whole images of its slots (`check_skin`); this skin
    // also paints every one of them, each once, from its own art: no erase, no glyph, and a stock
    // colour only by position.
    let mut painted = BTreeSet::new();
    for edit in recipe["image_edits"].as_array().expect("edits") {
        assert!(
            edit.get("erase").is_none() && edit.get("glyph").is_none(),
            "{edit}"
        );
        let paint = &edit["paint"];
        assert_eq!(
            paint["at"],
            serde_json::json!({ "x": 0, "y": 0, "width": 80, "height": 53 })
        );
        for entry in paint["palette"].as_array().expect("palette") {
            assert!(
                entry.get("colour").is_some() || entry.get("from").is_some(),
                "{entry}"
            );
        }
        assert!(painted.insert(edit["offset"].as_u64().expect("offset")));
    }
    let named: BTreeSet<u64> = images.images().map(|i| u64::from(i.offset)).collect();
    assert_eq!(painted, named);
}

/// Changes the committed PERFORM screen's part `name` in a catalog copy.
fn edit_part(root: &Path, name: &str, edit: impl FnOnce(&mut Value)) {
    let path = root.join(PERFORM);
    let mut screen = read_json(&path);
    edit(&mut screen["parts"][name]);
    write_bytes(&path, &serde_json::to_vec(&screen).expect("json"));
}

#[test]
fn a_parts_records_images_and_tables_hold_to_the_release() {
    let loaded = load_catalog(&repo_root()).expect("committed catalog");
    let perform = loaded
        .catalog()
        .screen("xdj700-v1.15", "perform")
        .expect("perform");
    let zones: usize = perform.parts.values().map(|part| part.touch.len()).sum();
    assert!(perform.parts.len() >= 30 && zones >= 20, "the PERFORM map");

    let target = recipe_target("xdj700-v1.15").expect("release");
    let end = target.application_len as u64;
    let load = target.load_address;
    let rev_image = 72 * 53 * 2;
    let set = |field: &'static [&'static str], value: String| {
        move |root: &Path| {
            edit_part(root, "rev", |p| {
                let mut slot = p;
                for key in field {
                    slot = match key.parse::<usize>() {
                        Ok(index) => &mut slot[index],
                        Err(_) => &mut slot[*key],
                    };
                }
                *slot = value.into();
            });
        }
    };
    const RECORD: &[&str] = &["touch", "0", "record"];
    const HANDLER: &[&str] = &["touch", "0", "handler"];
    const IMAGE: &[&str] = &["drawn", "0", "images", "0", "offset"];

    // Accepted at the edges: a record in the last 20 bytes, an image ending at the end, a
    // handler at the load address.
    assert_eq!(load_edited(set(RECORD, format!("{:#x}", end - 20))), None);
    assert_eq!(
        load_edited(set(IMAGE, format!("{:#x}", end - rev_image))),
        None
    );
    assert_eq!(load_edited(set(HANDLER, format!("{load:#x}"))), None);

    refused_with(
        set(RECORD, format!("{:#x}", end - 16)),
        &format!(
            "part rev: touch[0].record {:#x} is not in the {end}-byte application",
            end - 16
        ),
    );
    refused_with(
        set(HANDLER, format!("{:#x}", load + end)),
        &format!(
            "part rev: touch[0].handler {:#010x} is not in the application",
            load + end
        ),
    );
    refused_with(
        set(HANDLER, format!("{:#x}", load - 2)),
        "part rev: touch[0].handler 0x07fffffe is not in the application",
    );
    refused_with(
        set(IMAGE, format!("{:#x}", end - rev_image + 2)),
        &format!("ends past the {end}-byte application"),
    );
    // Held like slot images: out of the protected ranges, and edited only whole (stage 7 edits
    // the first pad image whole; REV's image two bytes into it would be edited in part).
    refused_with(
        set(IMAGE, "0x100".to_owned()),
        "overlaps the release's protected range 0x0..0x800",
    );
    refused_with(
        set(IMAGE, "0x34f942".to_owned()),
        "overlaps screen xdj700-v1.15/perform: part rev: drawn[0].images[0] at 0x34f942 \
         (72x53) but is not that image",
    );
    // A table entry past the end.
    refused_with(
        |root| {
            let offset = format!("{:#x}", end - 1);
            edit_part(root, "beat_loop.pads", |p| {
                p["tables"][1]["offset"] = offset.into()
            })
        },
        &format!("part beat_loop.pads: tables[1] ends past the {end}-byte application"),
    );
    // Stock values published only where review accepted them: a record in stage 7's window
    // outside the touch layout, or a table entry past the SLIP LOOP table in stage 5's window
    // (which stage 7 repeats exactly).
    refused_with(
        set(RECORD, "0xd5700".to_owned()),
        "lies in recipe recipes/xdj700-v1.15/beat-loop-1-to-32.json's precondition window",
    );
    refused_with(
        |root| {
            edit_part(root, "beat_loop.pads", |p| {
                p["tables"][1]["offset"] = "0xd6260".into()
            })
        },
        "tables[1] (0xd6260..0xd6261) lies in recipe recipes/xdj700-v1.15/beat-loop-1-to-32.json's \
         precondition window (0xd6234..0xd66f5)",
    );
}
