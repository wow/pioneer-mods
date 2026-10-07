//! `verify_rebuild` refuses outputs that break one property each. The outputs are built with
//! the synthetic helpers, independently of `rebuild_with_application`.

mod common;

use common::xdj700::{
    Main, application, descriptor, incompressible_application, lines, loader, padded_application,
    panel,
};
use common::{container, document_with_descriptor, panel_lines, record};
use patch_core::xdj700::{MAX_MAIN_GROWTH, encode_section, verify_rebuild};
use patch_core::{RebuildCheck, RebuildError, SectionError};

struct Case {
    stock: Vec<u8>,
    stock_image_len: usize,
    stock_main: Vec<u8>,
    decoded: Vec<u8>,
    image: Vec<u8>,
}

fn case() -> Case {
    let stock_decoded = padded_application(7);
    let mut stock_image = loader();
    stock_image.extend(encode_section(&stock_decoded).expect("encode"));
    stock_image.resize(stock_image.len().next_multiple_of(32), 0xFF);
    let decoded = application(5000, 11);
    let mut image = loader();
    image.extend(encode_section(&decoded).expect("encode"));
    Case {
        stock: Main::new(&stock_image, "Ver1.15").update(),
        stock_image_len: stock_image.len(),
        stock_main: Main::new(&stock_image, "Ver1.15").document(),
        decoded,
        image,
    }
}

impl Case {
    fn output(&self) -> Vec<u8> {
        Main::new(&self.image, "Ver1.22").update()
    }

    fn verify(&self, output: &[u8]) -> Result<(), RebuildError> {
        verify_rebuild(&self.stock, output, &self.decoded, "Ver1.22")
    }
}

fn failed(check: RebuildCheck) -> Result<(), RebuildError> {
    Err(RebuildError::Verification(check))
}

#[test]
fn accepts_a_correct_rebuild() {
    let case = case();

    assert_eq!(case.verify(&case.output()), Ok(()));
}

#[test]
fn refuses_output_that_does_not_parse() {
    let case = case();
    let mut output = case.output();
    output.pop();

    assert!(matches!(
        case.verify(&output),
        Err(RebuildError::OutputUnparseable(_))
    ));
}

#[test]
fn refuses_a_missing_document() {
    let case = case();
    let output = container(&[Main::new(&case.image, "Ver1.22").document()]);

    assert_eq!(case.verify(&output), failed(RebuildCheck::DocumentCount));
}

#[test]
fn refuses_a_changed_panl_document() {
    let case = case();
    let mut lines = panel_lines();
    lines[1] = record(b'2', 3, 0x0C0000, &[0x45; 3]);
    let panl = document_with_descriptor(&descriptor("PANL", "Ver1.00"), &lines);
    let output = container(&[Main::new(&case.image, "Ver1.22").document(), panl]);

    assert_eq!(
        case.verify(&output),
        failed(RebuildCheck::UntouchedDocument { index: 1 })
    );
}

#[test]
fn refuses_an_undeclared_version_label() {
    let case = case();

    assert_eq!(
        verify_rebuild(&case.stock, &case.output(), &case.decoded, "Ver1.23"),
        failed(RebuildCheck::MainFraming)
    );
}

#[test]
fn refuses_a_changed_s0_header() {
    let case = case();
    let output = container(&[
        Main {
            header: b"romobj  moT",
            ..Main::new(&case.image, "Ver1.22")
        }
        .document(),
        panel(),
    ]);

    assert_eq!(case.verify(&output), failed(RebuildCheck::MainFraming));
}

#[test]
fn refuses_a_changed_reserved_descriptor_field() {
    let case = case();
    let lines = lines(&Main::new(&case.image, "Ver1.22").document());
    let mut changed = descriptor("MAIN", "Ver1.22");
    changed[31] = b'1';
    let output = container(&[document_with_descriptor(&changed, &lines), panel()]);

    assert_eq!(case.verify(&output), failed(RebuildCheck::MainFraming));
}

#[test]
fn refuses_a_changed_termination() {
    let case = case();
    let output = container(&[
        Main {
            entry: 0xA000_0004,
            ..Main::new(&case.image, "Ver1.22")
        }
        .document(),
        panel(),
    ]);

    assert_eq!(case.verify(&output), failed(RebuildCheck::MainFraming));
}

#[test]
fn refuses_a_main_image_that_ends_inside_the_loader() {
    let case = case();
    let lines: Vec<Vec<u8>> = std::iter::once(record(b'0', 2, 0, b"romobj  mot"))
        .chain([
            record(b'2', 3, 0x000, &case.image[0x000..0x020]),
            record(b'2', 3, 0x020, &case.image[0x020..0x040]),
            record(b'2', 3, 0x100, &case.image[0x100..0x120]),
            record(b'2', 3, 0x120, &case.image[0x120..0x140]),
            record(b'2', 3, 0x140, &case.image[0x140..0x150]),
            record(b'7', 4, 0xA000_0000, &[]),
        ])
        .collect();
    let main = document_with_descriptor(&descriptor("MAIN", "Ver1.22"), &lines);

    assert_eq!(
        case.verify(&container(&[main, panel()])),
        failed(RebuildCheck::RecordLayout)
    );
}

#[test]
fn refuses_records_off_the_grid() {
    let case = case();
    let output = container(&[
        Main {
            record_len: 16,
            ..Main::new(&case.image, "Ver1.22")
        }
        .document(),
        panel(),
    ]);

    assert_eq!(case.verify(&output), failed(RebuildCheck::RecordLayout));
}

#[test]
fn refuses_a_changed_loader_byte() {
    let mut case = case();
    case.image[0x10] ^= 0x01;

    assert_eq!(case.verify(&case.output()), failed(RebuildCheck::Loader));
}

#[test]
fn refuses_data_after_the_section() {
    let mut case = case();
    case.image.extend_from_slice(&[0xFF; 32]);

    assert_eq!(
        case.verify(&case.output()),
        failed(RebuildCheck::TrailingData)
    );
}

#[test]
fn refuses_a_different_application() {
    let case = case();

    assert_eq!(
        verify_rebuild(
            &case.stock,
            &case.output(),
            &application(5000, 13),
            "Ver1.22"
        ),
        failed(RebuildCheck::Application)
    );
}

#[test]
fn refuses_an_invalid_section_and_keeps_the_cause() {
    let mut case = case();
    let last = case.image.len() - 1;
    case.image[last] ^= 0x01;

    let result = case.verify(&case.output());

    assert!(
        matches!(
            result,
            Err(RebuildError::OutputSection(
                SectionError::ChecksumMismatch { .. }
            ))
        ),
        "{result:?}"
    );
}

#[test]
fn refuses_a_malformed_declared_label() {
    let case = case();

    for label in ["1.22", "Ver1.222", "Ver1.2"] {
        assert_eq!(
            verify_rebuild(&case.stock, &case.output(), &case.decoded, label),
            Err(RebuildError::InvalidVersionLabel {
                label: label.to_owned()
            }),
            "{label:?}"
        );
    }
}

#[test]
fn refuses_a_changed_document_before_main() {
    let case = case();
    let stock_main = lines(&case.stock_main);
    let stock = container(&[
        panel(),
        document_with_descriptor(&descriptor("MAIN", "Ver1.15"), &stock_main),
    ]);
    let mut panel_changed = panel_lines();
    panel_changed[1] = record(b'2', 3, 0x0C0000, &[0x45; 3]);
    let output = container(&[
        document_with_descriptor(&descriptor("PANL", "Ver1.00"), &panel_changed),
        Main::new(&case.image, "Ver1.22").document(),
    ]);

    assert_eq!(
        verify_rebuild(&stock, &output, &case.decoded, "Ver1.22"),
        failed(RebuildCheck::UntouchedDocument { index: 0 })
    );
    let good = container(&[panel(), Main::new(&case.image, "Ver1.22").document()]);
    assert_eq!(
        verify_rebuild(&stock, &good, &case.decoded, "Ver1.22"),
        Ok(())
    );
}

#[test]
fn refuses_an_output_main_image_that_cannot_be_built() {
    let case = case();
    let lines = [
        record(b'0', 2, 0, b"romobj  mot"),
        record(b'3', 4, 0, &case.image[..32]),
        record(b'3', 4, 0x1000_0000, &[0xFF; 32]),
        record(b'7', 4, 0xA000_0000, &[]),
    ];
    let main = document_with_descriptor(&descriptor("MAIN", "Ver1.22"), &lines);

    let result = case.verify(&container(&[main, panel()]));

    assert!(
        matches!(
            result,
            Err(RebuildError::OutputSection(SectionError::Image(_)))
        ),
        "{result:?}"
    );
}

#[test]
fn refuses_output_growth_beyond_the_limit() {
    let case = case();
    let grown = incompressible_application(MAX_MAIN_GROWTH);
    let mut image = loader();
    image.extend(encode_section(&grown).expect("encode"));
    let output = Main::new(&image, "Ver1.22").update();

    assert_eq!(
        verify_rebuild(&case.stock, &output, &grown, "Ver1.22"),
        Err(RebuildError::ImageGrowthTooLarge {
            len: image.len(),
            limit: case.stock_image_len + MAX_MAIN_GROWTH,
        })
    );
}
