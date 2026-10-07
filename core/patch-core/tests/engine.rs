use patch_core::{
    OperationRegion, PatchEngineError, apply_recipe, sha256_hex, verify_bounded_diff,
};
use patch_schema::{
    OwnerCopyWindow, PatchOperation, RecipeManifest, SchemaValidationError, SupportedFirmware,
    WriteSpan,
};

const INPUT: [u8; 10] = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9];

fn write_span(offset: u64, bytes: &[u8]) -> PatchOperation {
    PatchOperation::WriteSpan(WriteSpan {
        offset,
        length: bytes.len() as u64,
        bytes: bytes.to_vec(),
    })
}

fn owner_copy(source_offset: u64, destination_offset: u64, length: u64) -> PatchOperation {
    PatchOperation::OwnerCopyWindow(OwnerCopyWindow {
        source_offset,
        destination_offset,
        length,
    })
}

fn target_for(input: &[u8]) -> SupportedFirmware {
    SupportedFirmware {
        model: "XDJ-700".to_owned(),
        version: "1.15".to_owned(),
        size_bytes: input.len() as u64,
        sha256_hex: sha256_hex(input),
        expected_output_sha256: None,
    }
}

fn manifest(targets: Vec<SupportedFirmware>, operations: Vec<PatchOperation>) -> RecipeManifest {
    RecipeManifest {
        schema_version: 1,
        recipe_id: "engine-test".to_owned(),
        description: "engine test".to_owned(),
        targets,
        operations,
    }
}

#[test]
fn applies_write_span_and_owner_copy() {
    let recipe = manifest(
        vec![target_for(&INPUT)],
        vec![write_span(2, &[170, 171, 172]), owner_copy(0, 8, 2)],
    );

    let outcome = apply_recipe(&recipe, &INPUT).expect("apply recipe");

    assert_eq!(
        outcome.output_bytes,
        vec![0, 1, 170, 171, 172, 5, 6, 7, 0, 1]
    );
    assert_eq!(outcome.input_sha256_hex, sha256_hex(&INPUT));
    assert_eq!(outcome.output_sha256_hex, sha256_hex(&outcome.output_bytes));
    assert_eq!(outcome.target, &recipe.targets[0]);
}

#[test]
fn owner_copy_reads_original_input_not_patched_output() {
    let recipe = manifest(
        vec![target_for(&INPUT)],
        vec![write_span(0, &[99, 99]), owner_copy(0, 8, 2)],
    );

    let outcome = apply_recipe(&recipe, &INPUT).expect("apply recipe");

    assert_eq!(outcome.output_bytes, vec![99, 99, 2, 3, 4, 5, 6, 7, 0, 1]);
}

#[test]
fn output_is_deterministic_and_independent_of_operation_order() {
    let forward = manifest(
        vec![target_for(&INPUT)],
        vec![write_span(0, &[99, 99]), owner_copy(0, 8, 2)],
    );
    let reversed = manifest(
        vec![target_for(&INPUT)],
        vec![owner_copy(0, 8, 2), write_span(0, &[99, 99])],
    );

    let first = apply_recipe(&forward, &INPUT).expect("first run");
    let second = apply_recipe(&forward, &INPUT).expect("second run");
    let reordered = apply_recipe(&reversed, &INPUT).expect("reordered run");

    assert_eq!(first.output_bytes, second.output_bytes);
    assert_eq!(first.output_bytes, reordered.output_bytes);
}

#[test]
fn matches_target_hash_case_insensitively() {
    let mut target = target_for(&INPUT);
    target.sha256_hex = target.sha256_hex.to_ascii_uppercase();
    assert_ne!(
        target.sha256_hex,
        sha256_hex(&INPUT),
        "fixture must differ in case"
    );
    let recipe = manifest(vec![target], vec![write_span(0, &[7])]);

    let outcome = apply_recipe(&recipe, &INPUT).expect("uppercase hash should match");

    assert_eq!(outcome.output_bytes[0], 7);
}

#[test]
fn selects_the_matching_target_among_several() {
    let mut other = target_for(&INPUT);
    other.version = "1.14".to_owned();
    other.sha256_hex = "ab".repeat(32);
    let recipe = manifest(vec![other, target_for(&INPUT)], vec![write_span(0, &[7])]);

    let outcome = apply_recipe(&recipe, &INPUT).expect("apply recipe");

    assert_eq!(outcome.target.version, "1.15");
}

#[test]
fn rejects_input_with_wrong_hash() {
    let mut target = target_for(&INPUT);
    target.sha256_hex = "00".repeat(32);
    let recipe = manifest(vec![target], vec![write_span(0, &[7])]);

    let error = apply_recipe(&recipe, &INPUT).expect_err("hash mismatch must refuse");

    assert_eq!(
        error,
        PatchEngineError::IncompatibleInput {
            recipe_id: "engine-test".to_owned(),
            size_bytes: INPUT.len() as u64,
            sha256_hex: sha256_hex(&INPUT),
        }
    );
}

#[test]
fn rejects_input_with_matching_hash_but_wrong_size() {
    let mut target = target_for(&INPUT);
    target.size_bytes += 1;
    let recipe = manifest(vec![target], vec![write_span(0, &[7])]);

    let error = apply_recipe(&recipe, &INPUT).expect_err("size mismatch must refuse");

    assert!(matches!(error, PatchEngineError::IncompatibleInput { .. }));
}

#[test]
fn rejects_invalid_manifest_before_matching() {
    let mut recipe = manifest(vec![target_for(&INPUT)], vec![write_span(0, &[7])]);
    recipe.schema_version = 2;

    let error = apply_recipe(&recipe, &INPUT).expect_err("invalid manifest must refuse");

    assert_eq!(
        error,
        PatchEngineError::InvalidManifest(SchemaValidationError::UnsupportedSchemaVersion {
            schema_version: 2
        })
    );
}

#[test]
fn rejects_out_of_bounds_write_span() {
    let recipe = manifest(vec![target_for(&INPUT)], vec![write_span(9, &[1, 2])]);

    let error = apply_recipe(&recipe, &INPUT).expect_err("out of bounds must refuse");

    assert_eq!(
        error,
        PatchEngineError::RangeOutOfBounds {
            operation_index: 0,
            region: OperationRegion::WriteSpanDestination,
            offset: 9,
            length: 2,
            input_size: INPUT.len(),
        }
    );
}

#[test]
fn rejects_out_of_bounds_owner_copy_source() {
    let recipe = manifest(
        vec![target_for(&INPUT)],
        vec![write_span(0, &[7]), owner_copy(9, 2, 2)],
    );

    let error = apply_recipe(&recipe, &INPUT).expect_err("out of bounds source must refuse");

    assert!(matches!(
        error,
        PatchEngineError::RangeOutOfBounds {
            operation_index: 1,
            region: OperationRegion::OwnerCopySource,
            ..
        }
    ));
}

#[test]
fn rejects_offset_plus_length_overflow() {
    let recipe = manifest(vec![target_for(&INPUT)], vec![owner_copy(0, u64::MAX, 2)]);

    let error = apply_recipe(&recipe, &INPUT).expect_err("overflow must refuse");

    assert!(matches!(
        error,
        PatchEngineError::RangeOutOfBounds {
            region: OperationRegion::OwnerCopyDestination,
            ..
        }
    ));
}

#[test]
fn rejects_overlapping_destinations_across_operation_kinds() {
    let recipe = manifest(
        vec![target_for(&INPUT)],
        vec![write_span(4, &[1, 1, 1]), owner_copy(0, 2, 3)],
    );

    let error = apply_recipe(&recipe, &INPUT).expect_err("overlap must refuse");

    assert_eq!(
        error,
        PatchEngineError::OverlappingDestinations {
            operation_index: 0,
            start: 4,
            end: 7,
            other_operation_index: 1,
            other_start: 2,
            other_end: 5,
        }
    );
}

#[test]
fn allows_adjacent_destinations() {
    let recipe = manifest(
        vec![target_for(&INPUT)],
        vec![write_span(2, &[1, 1]), write_span(4, &[2, 2])],
    );

    let outcome = apply_recipe(&recipe, &INPUT).expect("adjacent ranges are not overlaps");

    assert_eq!(&outcome.output_bytes[2..6], &[1, 1, 2, 2]);
}

#[test]
fn rejects_expected_output_hash_mismatch() {
    let mut target = target_for(&INPUT);
    target.expected_output_sha256 = Some("00".repeat(32));
    let recipe = manifest(vec![target], vec![write_span(0, &[7])]);

    let error = apply_recipe(&recipe, &INPUT).expect_err("output mismatch must refuse");

    assert!(matches!(error, PatchEngineError::OutputHashMismatch { .. }));
}

#[test]
fn accepts_expected_output_hash_in_uppercase() {
    let mut expected_output = INPUT.to_vec();
    expected_output[0] = 7;
    let mut target = target_for(&INPUT);
    target.expected_output_sha256 = Some(sha256_hex(&expected_output).to_ascii_uppercase());
    let recipe = manifest(vec![target], vec![write_span(0, &[7])]);

    let outcome = apply_recipe(&recipe, &INPUT).expect("expected output should match");

    assert_eq!(outcome.output_bytes, expected_output);
}

#[test]
fn bounded_diff_accepts_changes_inside_unsorted_ranges() {
    let mut output = INPUT.to_vec();
    output[1] = 50;
    output[8] = 50;

    verify_bounded_diff(&INPUT, &output, &[7..9, 0..2]).expect("changes are declared");
}

#[test]
fn bounded_diff_rejects_change_outside_ranges() {
    let mut output = INPUT.to_vec();
    output[1] = 50;
    output[5] = 50;

    let error = verify_bounded_diff(&INPUT, &output, &[0..2, 7..9]).expect_err("byte 5 undeclared");

    assert_eq!(
        error,
        PatchEngineError::MutationOutsideDeclaredRegions { byte_offset: 5 }
    );
}

#[test]
fn bounded_diff_rejects_length_change() {
    let declared = [0..5, 5..INPUT.len()];
    let error = verify_bounded_diff(&INPUT, &INPUT[..9], &declared).expect_err("length differs");

    assert!(matches!(
        error,
        PatchEngineError::OutputLengthChanged { .. }
    ));
}
