mod common;

use common::xdj700::{
    Main, SYNTHETIC_MAX_MAIN_IMAGE_LEN, application, descriptor, incompressible_application, lines,
    loader, main_image, other_document, padded_application, panel, rebuild, release, stock_update,
};
use common::{container, document_with_descriptor, record};
use patch_core::xdj700::{
    APPLICATION_SECTION_OFFSET, MAX_MAIN_GROWTH, OFFICIAL_V115, StockRelease, decode_application,
    encode_section, rebuild_with_application,
};
use patch_core::{LzssError, RebuildError, SectionError, parse_upd, sha256_hex};

#[test]
fn rebuild_matches_an_independently_built_update() {
    let stock = stock_update(&padded_application(7));
    let modified = application(5000, 11);

    let rebuilt = rebuild(&stock, &modified, "Ver1.22").expect("rebuild");

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

    let rebuilt = rebuild(&stock, &modified, "Ver1.15").expect("rebuild");

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

    let first = rebuild(&stock, &modified, "Ver1.22").expect("first");
    let second = rebuild(&stock, &modified, "Ver1.22").expect("second");

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

    let rebuilt = rebuild(&stock, &decoded, "Ver1.15").expect("rebuild");

    assert_eq!(rebuilt.bytes(), stock.as_slice());
}

#[test]
fn rebuild_drops_the_stock_padding() {
    let decoded = padded_application(7);
    let padded = main_image(&decoded, true);
    let unpadded = main_image(&decoded, false);
    assert!(padded.len() > unpadded.len(), "fixture must need padding");

    let rebuilt = rebuild(&stock_update(&decoded), &decoded, "Ver1.15").expect("rebuild");

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
            rebuild(&stock, &decoded, label),
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

    let result = rebuild(&stock, &application(300, 9), "Ver1.22");

    assert!(matches!(result, Err(RebuildError::Input(_))), "{result:?}");
}

#[test]
fn rebuild_refuses_an_unverified_main_version() {
    let stock = Main::new(&main_image(&application(300, 7), true), "Ver1.14").update();

    assert_eq!(
        rebuild(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::InputSection(
            SectionError::UnverifiedVersion {
                version: "Ver1.14".to_owned()
            }
        ))
    );
}

#[test]
fn rebuild_refuses_an_update_without_main() {
    let stock = container(&[panel()]);

    assert_eq!(
        rebuild(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::InputSection(SectionError::NoMainDocument))
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
        rebuild(&stock, &application(300, 9), "Ver1.22"),
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
        rebuild(&stock, &application(300, 9), "Ver1.22"),
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
        rebuild(&stock, &modified, "Ver1.22"),
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

    let result = rebuild(&stock, &application(300, 9), "Ver1.22");

    assert!(
        matches!(
            result,
            Err(RebuildError::InputSection(
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
        rebuild(&stock, b"no zero seed", "Ver1.22"),
        Err(RebuildError::Encode(SectionError::Encode(
            LzssError::MissingSectionSeed
        )))
    );
}

#[test]
fn rebuild_clamps_the_size_bound_to_24_bit_addresses() {
    // Incompressible bytes: the section grows by 1/8, past the 16 MiB S2 address space.
    let decoded = incompressible_application(15 << 20);
    let stock = stock_update(&application(300, 7));
    let sha256 = sha256_hex(&stock);
    let unbounded = StockRelease {
        upd_sha256: &sha256,
        max_main_image_len: usize::MAX,
    };

    let result = rebuild_with_application(&stock, &unbounded, &decoded, "Ver1.22");

    let Err(RebuildError::ImageTooLarge { len, limit }) = result else {
        panic!("expected ImageTooLarge, got {result:?}");
    };
    assert_eq!(limit, 1 << 24);
    assert!(len > limit, "{len}");
}

#[test]
fn rebuild_refuses_an_image_beyond_the_release_bound() {
    let stock = stock_update(&padded_application(7));
    let grown = incompressible_application(64 * 1024);
    let len = APPLICATION_SECTION_OFFSET + encode_section(&grown).expect("encode").len();

    assert_eq!(
        rebuild(&stock, &grown, "Ver1.22"),
        Err(RebuildError::ImageTooLarge {
            len,
            limit: SYNTHETIC_MAX_MAIN_IMAGE_LEN,
        })
    );
}

#[test]
fn rebuild_refuses_an_input_other_than_the_pinned_release() {
    let stock = stock_update(&padded_application(7));
    let other = stock_update(&application(300, 9));
    let other_sha256 = sha256_hex(&other);

    assert_eq!(
        rebuild_with_application(
            &stock,
            &release(&other_sha256),
            &application(300, 9),
            "Ver1.22"
        ),
        Err(RebuildError::UnpinnedInput {
            sha256: sha256_hex(&stock)
        })
    );
}

#[test]
fn rebuild_refuses_its_own_output_as_the_next_input() {
    // Chaining rebuilds must not reset the size bound or carry a non-official loader forward.
    let stock = stock_update(&padded_application(7));
    let sha256 = sha256_hex(&stock);
    let first = rebuild(&stock, &application(3000, 11), "Ver1.15").expect("first rebuild");

    let second = rebuild_with_application(
        first.bytes(),
        &release(&sha256),
        &application(3000, 13),
        "Ver1.15",
    );

    assert_eq!(
        second,
        Err(RebuildError::UnpinnedInput {
            sha256: first.sha256().to_owned()
        })
    );
}

#[test]
fn official_v115_release_pins_the_official_file_and_bound() {
    assert_eq!(
        OFFICIAL_V115.upd_sha256,
        "73edec9802da51672257c2599efc04209dc92478fcbaa1a0425b3b122e33f99c"
    );
    assert_eq!(OFFICIAL_V115.max_main_image_len, 7_251_904 + 256 * 1024);
    assert_eq!(MAX_MAIN_GROWTH, 256 * 1024);
}

#[test]
fn rebuild_keeps_documents_before_and_after_main() {
    let stock_image = main_image(&padded_application(7), true);
    let stock_main = Main::new(&stock_image, "Ver1.15").document();
    let stock = container(&[panel(), stock_main, other_document()]);
    let modified = application(5000, 11);

    let rebuilt = rebuild(&stock, &modified, "Ver1.22").expect("rebuild");

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
        rebuild(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::InputSection(
            SectionError::AmbiguousMainDocument { count: 2 }
        ))
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
        rebuild(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::InputSection(SectionError::ImageBase {
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
        rebuild(&stock, &application(300, 9), "Ver1.22"),
        Err(RebuildError::NonCanonicalRecordLayout)
    );
}
