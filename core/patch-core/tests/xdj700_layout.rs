//! Layout verification by loader region (`verify_main_layout_with`), on synthetic images. The
//! official loader identity itself is checked by the owner-input test `official_firmware`.

mod common;

use common::xdj700::{LOADER_EXTENTS, Main, application, loader, main_image};
use common::{container, document_with_descriptor, record};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, OFFICIAL_V115_LABEL, VERIFIED_MAIN_VERSIONS, is_label_higher,
    main_document, validate_version_label, verify_main_layout, verify_main_layout_with,
};
use patch_core::{RebuildError, SectionError, parse_upd, sha256_hex};

fn check(update: &[u8], verified_loaders: &[&str]) -> Result<(), SectionError> {
    let parsed = parse_upd(update).expect("valid container");
    let main = main_document(&parsed).expect("main");
    let image = main.image().expect("image");
    verify_main_layout_with(main, &image, verified_loaders)
}

fn unverified(version: &str) -> Result<(), SectionError> {
    Err(SectionError::UnverifiedVersion {
        version: version.to_owned(),
    })
}

#[test]
fn an_unverified_label_is_accepted_when_the_loader_region_matches() {
    let update = Main::new(&main_image(&application(300, 7), true), "Ver1.16").update();
    let loader_sha256 = sha256_hex(&loader());

    assert_eq!(check(&update, &[&loader_sha256]), Ok(()));
    assert_eq!(check(&update, &[]), unverified("Ver1.16"));
    // The default list holds only the official loader, which a synthetic image never matches.
    let parsed = parse_upd(&update).expect("valid container");
    let main = main_document(&parsed).expect("main");
    assert_eq!(
        verify_main_layout(main, &main.image().expect("image")),
        unverified("Ver1.16")
    );
}

#[test]
fn one_changed_loader_byte_is_refused() {
    let mut image = main_image(&application(300, 7), true);
    image[LOADER_EXTENTS[1].0] ^= 1;
    let update = Main::new(&image, "Ver1.16").update();

    assert_eq!(
        check(&update, &[&sha256_hex(&loader())]),
        unverified("Ver1.16")
    );
}

#[test]
fn an_image_shorter_than_the_loader_region_is_refused() {
    let short = &loader()[..LOADER_EXTENTS[1].1];
    let update = Main::new(short, "Ver1.16").update();
    let parsed = parse_upd(&update).expect("valid container");
    let image = main_document(&parsed)
        .expect("main")
        .image()
        .expect("image");
    assert!(image.bytes().len() < APPLICATION_SECTION_OFFSET);

    assert_eq!(
        check(&update, &[&sha256_hex(image.bytes())]),
        unverified("Ver1.16")
    );
}

#[test]
fn a_loader_region_not_based_at_zero_is_refused() {
    let image = main_image(&application(300, 7), true);
    let mut lines = vec![record(b'0', 2, 0, b"synthetic")];
    for (index, chunk) in image.chunks(32).enumerate() {
        lines.push(record(b'2', 3, 0x100 + (index * 32) as u32, chunk));
    }
    lines.push(record(b'7', 4, 0, &[]));
    let descriptor = common::xdj700::descriptor("MAIN", "Ver1.16");
    let update = container(&[document_with_descriptor(&descriptor, &lines)]);

    assert_eq!(
        check(&update, &[&sha256_hex(&loader())]),
        unverified("Ver1.16")
    );
}

#[test]
fn a_verified_label_is_accepted_whatever_the_loader() {
    assert_eq!(VERIFIED_MAIN_VERSIONS, [OFFICIAL_V115_LABEL]);
    let update = Main::new(&main_image(&application(300, 7), true), OFFICIAL_V115_LABEL).update();

    assert_eq!(check(&update, &[]), Ok(()));
}

#[test]
fn labels_compare_by_version_number() {
    for (label, installed, higher) in [
        ("Ver1.16", "Ver1.15", true),
        ("Ver2.00", "Ver1.99", true),
        ("Ver1.15", "Ver1.15", false),
        ("Ver0.90", "Ver1.15", false),
        ("Ver1.09", "Ver1.10", false),
    ] {
        assert_eq!(
            is_label_higher(label, installed),
            Ok(higher),
            "{label} vs {installed}"
        );
    }
    for bad in ["Ver1.5", "ver1.16", "Ver1.1a", "Ver10.00", "Ver1.160"] {
        let error = Err(RebuildError::InvalidVersionLabel {
            label: bad.to_owned(),
        });
        assert_eq!(validate_version_label(bad), error.clone().map(|_: ()| ()));
        assert_eq!(
            is_label_higher(bad, "Ver1.15"),
            error.clone().map(|_: ()| false)
        );
        assert_eq!(is_label_higher("Ver1.15", bad), error.map(|_: ()| false));
    }
}
