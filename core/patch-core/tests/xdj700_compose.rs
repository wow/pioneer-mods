//! Composing schema-v2 recipes into one build, on the synthetic release: each recipe is checked
//! against its own pinned output, recipes may share only exact repeats and same image edits, and
//! the composed application holds exactly the recipes' changes.

mod common;

use common::recipe::{Fixture, replacement, stock_application};
use patch_core::parse_upd;
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, ComposeError, ComposedUpdate, Composition, RecipeChecks,
    RecipeError, VERSION_STRING_OFFSET, compose_recipes_to, decode_section, main_document,
    precondition_hashes,
};
use patch_schema::{ExpectedV2, Glyph, ImageEdit, Pixel, PixelBox, RecipeV2};

const COMPOSITION: Composition<'static> = Composition {
    label: "Ver1.16",
    reported_version: "0.12",
};

/// `recipe` named `id`, with the output identities it produces alone as its pins.
fn pinned(fixture: &Fixture, mut recipe: RecipeV2, id: &str) -> RecipeV2 {
    recipe.recipe_id = id.to_owned();
    // A placeholder first, as in a draft (a recipe with image edits must declare one).
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some("0".repeat(64)),
        upd_sha256: None,
    });
    let target = fixture.target();
    let hashes = precondition_hashes(&recipe, &target, &fixture.update, RecipeChecks::NONE)
        .expect("a valid draft");
    recipe.expected = Some(ExpectedV2 {
        application_sha256: Some(hashes.output.application_sha256),
        upd_sha256: Some(hashes.output.upd_sha256),
    });
    recipe
}

/// A pinned recipe that inverts the stock byte at each of `offsets`.
fn inverting(fixture: &Fixture, id: &str, offsets: &[usize]) -> RecipeV2 {
    let stock = stock_application();
    let replacements = offsets
        .iter()
        .map(|&offset| replacement(offset, &[!stock[offset]]))
        .collect();
    pinned(fixture, fixture.recipe(replacements), id)
}

/// Erases part of a 17x8 image at `offset` and draws a 3x2 glyph, as in the image-edit tests.
fn edit(offset: usize, purpose: &str) -> ImageEdit {
    ImageEdit {
        offset: offset as u64,
        width: 17,
        height: 8,
        erase: Some(PixelBox {
            x: 4,
            y: 2,
            width: 6,
            height: 3,
        }),
        glyph: Glyph {
            at: PixelBox {
                x: 5,
                y: 3,
                width: 3,
                height: 2,
            },
            alpha_hex: "f8008f".to_owned(),
            colour_from: Pixel { x: 1, y: 1 },
        },
        purpose: purpose.to_owned(),
    }
}

fn with_image(fixture: &Fixture, id: &str, edit: ImageEdit, offsets: &[usize]) -> RecipeV2 {
    let mut recipe = inverting(fixture, id, offsets);
    recipe.image_edits = vec![edit];
    pinned(fixture, recipe, id)
}

fn compose(fixture: &Fixture, fragments: &[RecipeV2]) -> Result<ComposedUpdate, ComposeError> {
    let target = fixture.target();
    compose_recipes_to(
        fragments,
        &target,
        COMPOSITION,
        &fixture.update,
        RecipeChecks::NONE,
    )
}

fn decoded(bytes: &[u8]) -> Vec<u8> {
    let parsed = parse_upd(bytes).expect("parse");
    let image = main_document(&parsed)
        .expect("main")
        .image()
        .expect("image");
    let section = decode_section(image.bytes(), APPLICATION_SECTION_OFFSET).expect("decode");
    section.decoded().to_vec()
}

fn changed_offsets(output: &[u8]) -> Vec<usize> {
    let stock = stock_application();
    (0..stock.len())
        .filter(|&at| output[at] != stock[at])
        .collect()
}

#[test]
fn composes_disjoint_recipes_into_one_build() {
    let fixture = Fixture::new();
    let a = inverting(&fixture, "a", &[0x900]);
    let b = inverting(&fixture, "b", &[0xa00]);

    let composed = compose(&fixture, &[a.clone(), b.clone()]).expect("compose");

    let output = decoded(composed.rebuilt.bytes());
    // "1.15" -> "0.12" changes the first and last characters.
    let version = VERSION_STRING_OFFSET;
    assert_eq!(
        changed_offsets(&output),
        [version, version + 3, 0x900, 0xa00]
    );
    assert_eq!(
        composed.rebuilt.application_reported_version(),
        Some("0.12")
    );
    // Each recipe reproduced its own pins alone.
    for (identities, recipe) in composed.fragments.iter().zip([&a, &b]) {
        let pins = recipe.expected.as_ref().expect("pinned");
        assert_eq!(
            pins.application_sha256.as_ref(),
            Some(&identities.application_sha256)
        );
        assert_eq!(pins.upd_sha256.as_ref(), Some(&identities.upd_sha256));
    }
}

#[test]
fn an_exact_repeat_and_the_same_image_edit_are_applied_once() {
    let fixture = Fixture::new();
    let a = with_image(&fixture, "a", edit(0xd00, "a's label"), &[0x900]);
    // The same replacement and the same edit (only the purpose differs), plus one of its own.
    let b = with_image(&fixture, "b", edit(0xd00, "b's label"), &[0x900, 0xa00]);

    let composed = compose(&fixture, &[a.clone(), b]).expect("compose");

    let output = decoded(composed.rebuilt.bytes());
    let alone = decoded(&{
        let target = fixture.target();
        let rebuilt = patch_core::xdj700::apply_recipe_v2_to(
            &a,
            &target,
            &fixture.update,
            RecipeChecks::NONE,
        )
        .expect("a alone");
        rebuilt.into_bytes()
    });
    // The image holds a's (and b's) edit once, not drawn twice.
    assert_eq!(
        output[0xd00..0xd00 + 17 * 8 * 2],
        alone[0xd00..0xd00 + 17 * 8 * 2]
    );
    assert_eq!(output[0xa00], !stock_application()[0xa00]);
}

#[test]
fn recipes_that_overlap_are_refused() {
    let fixture = Fixture::new();
    let a = with_image(&fixture, "a", edit(0xd00, "a"), &[0x900]);

    // A replacement whose window overlaps a's, but is not a repeat.
    let shifted = inverting(&fixture, "shifted", &[0x905]);
    assert!(matches!(
        compose(&fixture, &[a.clone(), shifted]),
        Err(ComposeError::Overlap(_))
    ));

    // A different edit of an overlapping image.
    let other_image = with_image(&fixture, "other", edit(0xd04, "other"), &[0xa00]);
    assert_eq!(
        compose(&fixture, &[a, other_image]).map(|_| ()),
        Err(ComposeError::ImageOverlap {
            first: "a image_edits[0]".to_owned(),
            second: "other image_edits[0]".to_owned(),
        })
    );
}

#[test]
fn every_recipe_must_pin_its_output_and_reproduce_it() {
    let fixture = Fixture::new();
    let a = inverting(&fixture, "a", &[0x900]);
    let mut b = inverting(&fixture, "b", &[0xa00]);

    b.expected = None;
    assert_eq!(
        compose(&fixture, &[a.clone(), b.clone()]).map(|_| ()),
        Err(ComposeError::Unpinned {
            index: 1,
            recipe_id: "b".to_owned()
        })
    );

    b.expected = Some(ExpectedV2 {
        application_sha256: Some("ab".repeat(32)),
        upd_sha256: None,
    });
    let error = compose(&fixture, &[a, b])
        .map(|_| ())
        .expect_err("wrong pin");
    let ComposeError::Fragment {
        index,
        recipe_id,
        source,
    } = &error
    else {
        panic!("{error}");
    };
    assert_eq!((*index, recipe_id.as_str()), (1, "b"));
    assert!(
        matches!(source, RecipeError::UnexpectedOutput { .. }),
        "{source}"
    );
}

#[test]
fn the_composition_follows_the_label_and_version_rules() {
    let fixture = Fixture::new();
    let fragments = [inverting(&fixture, "a", &[0x900])];
    let target = fixture.target();
    let compose_as = |label, reported_version| {
        let composition = Composition {
            label,
            reported_version,
        };
        compose_recipes_to(
            &fragments,
            &target,
            composition,
            &fixture.update,
            RecipeChecks::NONE,
        )
        .map(|_| ())
    };

    assert!(matches!(
        compose_as("Ver1.15", "0.12"),
        Err(ComposeError::Composition(
            RecipeError::LabelNotHigher { .. }
        ))
    ));
    assert!(matches!(
        compose_as("Ver1.16", "1.15"),
        Err(ComposeError::Composition(RecipeError::Rebuild(_)))
    ));
    assert_eq!(
        compose(&fixture, &[]).map(|_| ()),
        Err(ComposeError::NoFragments)
    );
}
