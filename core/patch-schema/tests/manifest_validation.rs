use patch_schema::{
    PatchOperation, RecipeManifest, SchemaValidationError, SupportedFirmware, WriteSpan,
};

fn valid_manifest() -> RecipeManifest {
    RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-waveform3".to_owned(),
        description: "Enable 3-band waveform rendering".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: 16,
            sha256_hex: "00".repeat(32),
        }],
        operations: vec![PatchOperation::WriteSpan(WriteSpan {
            offset: 4,
            length: 3,
            bytes: vec![1, 2, 3],
        })],
    }
}

#[test]
fn validates_well_formed_manifest() {
    assert!(valid_manifest().validate().is_ok());
}

#[test]
fn rejects_invalid_write_span_length_mismatch() {
    let mut manifest = valid_manifest();
    let PatchOperation::WriteSpan(span) = &mut manifest.operations[0] else {
        panic!("unexpected operation");
    };
    span.length = 10;
    assert_eq!(
        manifest.validate(),
        Err(SchemaValidationError::WriteSpanLengthMismatch)
    );
}

#[test]
fn rejects_invalid_target_hash_shape() {
    let mut manifest = valid_manifest();
    manifest.targets[0].sha256_hex = "abc123".to_owned();

    assert_eq!(
        manifest.validate(),
        Err(SchemaValidationError::InvalidTargetSha256Hex)
    );
}

#[test]
fn matches_target_by_size_and_hash_case_insensitive() {
    let manifest = valid_manifest();
    let target = manifest
        .matching_target(
            16,
            "0000000000000000000000000000000000000000000000000000000000000000",
        )
        .expect("matching target");
    assert_eq!(target.model, "XDJ-700");
}
