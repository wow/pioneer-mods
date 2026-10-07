mod common;

use common::xdj700::{
    Main, application, descriptor, incompressible_application, lines, loader, main_image,
    other_document, padded_application, panel, stock_update,
};
use common::{container, document_with_descriptor, record};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, MAX_MAIN_GROWTH, decode_application, encode_section,
    rebuild_with_application,
};
use patch_core::{LzssError, RebuildError, SectionError, parse_upd, sha256_hex};

#[test]
fn rebuild_matches_an_independently_built_update() {
    let stock = stock_update(&padded_application(7));
    let modified = application(5000, 11);

    let rebuilt = rebuild_with_application(&stock, &modified, "Ver1.22").expect("rebuild");

    let mut expected_image = loader();
    expected_image.extend(encode_section(&modified).expect("encode"));
    let expected = Main::new(&expected_image, "Ver1.22").update();
    assert_eq!(rebuilt.bytes(), expected.as_slice());
    assert_eq!(rebuilt.sha256(), sha256_hex(&expected));
    assert_eq!(rebuilt.main_image_len(), expected_image.len());
    assert_eq!(rebuilt.main_image_sha256(), sha256_hex(&expected_image));
}

#[test]
fn rebuilt_update_decodes_to_the_new_application_and_keeps_panl() {
    let stock = stock_update(&padded_application(7));
    let modified = application(5000, 11);

    let rebuilt = rebuild_with_application(&stock, &modified, "Ver1.15").expect("rebuild");

    let parsed = parse_upd(rebuilt.bytes()).expect("parses");
    let section = decode_application(&parsed).expect("decodes");
    assert_eq!(section.decoded(), modified.as_slice());
    let [_, panl] = parsed.documents() else {
        panic!("expected MAIN and PANL");
    };
    let panl_bytes = &rebuilt.bytes()[panl.offset()..panl.offset() + panl.length()];
    assert_eq!(panl_bytes, panel().as_slice());
}

#[test]
fn rebuild_is_deterministic() {
    let stock = stock_update(&padded_application(7));
    let modified = application(4000, 13);

    let first = rebuild_with_application(&stock, &modified, "Ver1.22").expect("first");
    let second = rebuild_with_application(&stock, &modified, "Ver1.22").expect("second");

    assert_eq!(first, second);
}

#[test]
fn noop_rebuild_reproduces_an_unpadded_input_byte_for_byte() {
    // Find an application whose section ends on a record boundary, so the input has no padding.
    let decoded = (2000..2100)
        .map(|len| application(len, 5))
        .find(|decoded| {
            encode_section(decoded)
                .expect("encode")
                .len()
                .is_multiple_of(32)
        })
        .expect("some length ends on a 32-byte boundary");
    let stock = Main::new(&main_image(&decoded, false), "Ver1.15").update();

    let rebuilt = rebuild_with_application(&stock, &decoded, "Ver1.15").expect("rebuild");

    assert_eq!(rebuilt.bytes(), stock.as_slice());
}

#[test]
fn rebuild_drops_the_stock_padding() {
    let decoded = padded_application(7);
    let padded = main_image(&decoded, true);
    let unpadded = main_image(&decoded, false);
    assert!(padded.len() > unpadded.len(), "fixture must need padding");

    let rebuilt =
        rebuild_with_application(&stock_update(&decoded), &decoded, "Ver1.15").expect("rebuild");

    assert_eq!(rebuilt.main_image_len(), unpadded.len());
    assert_eq!(rebuilt.main_image_sha256(), sha256_hex(&unpadded));
}

#[test]
fn rebuild_refuses_malformed_version_labels() {
    let stock = stock_update(&application(300, 7));
    let decoded = application(300, 9);

    for label in [
        "", "Ver1.2", "Ver1.222", "ver1.22", "Ver1,22", "VerA.22", "Ver1.2x", "Ver1.22 ",
    ] {
        assert_eq!(
            rebuild_with_application(&stock, &decoded, label),
            Err(RebuildError::InvalidVersionLabel {
                label: label.to_owned()
            }),
            "{label:?}"
        );
    }
}

#[test]
fn rebuild_refuses_an_input_that_does_not_parse() {
    let mut stock = stock_update(&application(300, 7));
    stock.pop();

    let result = rebuild_with_application(&stock, &application(300, 9), "Ver1.22");

    assert!(matches!(result, Err(RebuildError::Input(_))), "{result:?}");
}

#[test]
fn rebuild_refuses_an_unverified_main_version() {
    let stock = Main::new(&main_image(&application(300, 7), true), "Ver1.14").update();

    assert_eq!(
        rebuild_with_application(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::Section(SectionError::UnverifiedVersion {
            version: "Ver1.14".to_owned()
        }))
    );
}

#[test]
fn rebuild_refuses_an_update_without_main() {
    let stock = container(&[panel()]);

    assert_eq!(
        rebuild_with_application(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::Section(SectionError::NoMainDocument))
    );
}

#[test]
fn rebuild_refuses_data_after_the_stock_section() {
    let mut image = main_image(&padded_application(7), true);
    let last = image.len() - 1;
    assert_eq!(image[last], 0xFF, "fixture must end in padding");
    image[last] = 0x00;
    let stock = Main::new(&image, "Ver1.15").update();

    assert_eq!(
        rebuild_with_application(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::DataAfterSection)
    );
}

#[test]
fn rebuild_refuses_a_stock_layout_off_the_record_grid() {
    let image = main_image(&padded_application(7), true);
    let stock = Main {
        record_len: 16,
        ..Main::new(&image, "Ver1.15")
    }
    .update();

    assert_eq!(
        rebuild_with_application(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::NonCanonicalRecordLayout)
    );
}

#[test]
fn rebuild_refuses_a_stock_layout_with_an_extent_after_the_section() {
    // An erased-looking (0xFF) record beyond a gap: the padding check passes, but the section
    // is no longer in the last extent. The new section is long enough to reach past that
    // record, so only the extent-order check names the cause.
    let mut image = main_image(&padded_application(7), true);
    let stock_document = Main::new(&image, "Ver1.15").document();
    let far = image.len() + 0x100;
    image.resize(far + 32, 0xFF);
    let mut lines = lines(&stock_document);
    let termination = lines.pop().expect("termination");
    lines.push(record(b'2', 3, far as u32, &[0xFF; 32]));
    lines.push(termination);
    let main = document_with_descriptor(&descriptor("MAIN", "Ver1.15"), &lines);
    let stock = container(&[main, panel()]);

    let modified = application(60_000, 9);
    let new_end = 0x40000 + encode_section(&modified).expect("encode").len();
    assert!(
        new_end > far + 32,
        "fixture: the new section must reach past the far record"
    );

    assert_eq!(
        rebuild_with_application(&stock, &modified, "Ver1.22"),
        Err(RebuildError::NonCanonicalRecordLayout)
    );
}

#[test]
fn rebuild_refuses_a_main_image_without_a_section() {
    let lines = vec![
        record(b'0', 2, 0, b"romobj  mot"),
        record(b'2', 3, 0, &[0x5A; 32]),
        record(b'7', 4, 0xA000_0000, &[]),
    ];
    let main = document_with_descriptor(&descriptor("MAIN", "Ver1.15"), &lines);
    let stock = container(&[main, panel()]);

    let result = rebuild_with_application(&stock, &application(300, 9), "Ver1.22");

    assert!(
        matches!(
            result,
            Err(RebuildError::Section(
                SectionError::SizeFieldOutOfBounds { .. }
            ))
        ),
        "{result:?}"
    );
}

#[test]
fn rebuild_refuses_an_application_without_the_zero_seed() {
    let stock = stock_update(&application(300, 7));

    assert_eq!(
        rebuild_with_application(&stock, b"no zero seed", "Ver1.22"),
        Err(RebuildError::Section(SectionError::Encode(
            LzssError::MissingSectionSeed
        )))
    );
}

#[test]
fn rebuild_refuses_an_image_beyond_24_bit_addresses() {
    // Incompressible bytes: the section grows by 1/8, past the 16 MiB S2 address space.
    let decoded = incompressible_application(15 << 20);
    let stock = stock_update(&application(300, 7));

    let result = rebuild_with_application(&stock, &decoded, "Ver1.22");

    let Err(RebuildError::ImageTooLarge { len }) = result else {
        panic!("expected ImageTooLarge, got {result:?}");
    };
    assert!(len > 1 << 24, "{len}");
}

#[test]
fn rebuild_refuses_growth_beyond_the_limit() {
    let decoded = padded_application(7);
    let stock_image = main_image(&decoded, true);
    let stock = Main::new(&stock_image, "Ver1.15").update();
    let grown = incompressible_application(MAX_MAIN_GROWTH);
    let len = APPLICATION_SECTION_OFFSET + encode_section(&grown).expect("encode").len();

    assert_eq!(
        rebuild_with_application(&stock, &grown, "Ver1.22"),
        Err(RebuildError::ImageGrowthTooLarge {
            len,
            limit: stock_image.len() + MAX_MAIN_GROWTH,
        })
    );
}

#[test]
fn rebuild_keeps_documents_before_and_after_main() {
    let stock_image = main_image(&padded_application(7), true);
    let stock_main = Main::new(&stock_image, "Ver1.15").document();
    let stock = container(&[panel(), stock_main, other_document()]);
    let modified = application(5000, 11);

    let rebuilt = rebuild_with_application(&stock, &modified, "Ver1.22").expect("rebuild");

    let mut image = loader();
    image.extend(encode_section(&modified).expect("encode"));
    let main = Main::new(&image, "Ver1.22").document();
    assert_eq!(
        rebuilt.bytes(),
        container(&[panel(), main, other_document()]).as_slice()
    );
}

#[test]
fn rebuild_refuses_two_main_documents() {
    let main = Main::new(&main_image(&padded_application(7), true), "Ver1.15").document();
    let stock = container(&[main.clone(), main, panel()]);

    assert_eq!(
        rebuild_with_application(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::Section(SectionError::AmbiguousMainDocument {
            count: 2
        }))
    );
}

#[test]
fn rebuild_refuses_a_main_image_not_based_at_zero() {
    let main = Main::new(&main_image(&padded_application(7), true), "Ver1.15").document();
    let mut lines = lines(&main);
    lines.remove(1);
    let stock = container(&[
        document_with_descriptor(&descriptor("MAIN", "Ver1.15"), &lines),
        panel(),
    ]);

    assert_eq!(
        rebuild_with_application(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::Section(SectionError::ImageBase {
            base: 0x20
        }))
    );
}

#[test]
fn rebuild_refuses_a_stock_layout_with_non_s2_records() {
    let image = main_image(&padded_application(7), true);
    let mut lines = lines(&Main::new(&image, "Ver1.15").document());
    lines[1] = record(b'3', 4, 0, &image[..32]);
    let stock = container(&[
        document_with_descriptor(&descriptor("MAIN", "Ver1.15"), &lines),
        panel(),
    ]);

    assert_eq!(
        rebuild_with_application(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::NonCanonicalRecordLayout)
    );
}
