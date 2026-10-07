mod common;

use common::{run_patch_command_with_args_in_dir, write_bytes};
use patch_core::identify_firmware;
use patch_schema::{PatchOperation, RecipeManifest, SupportedFirmware, WriteSpan};
use std::fs;
use std::path::Path;

#[test]
fn patch_rejects_expected_output_sha256_mismatch_without_writing_output() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");

    write_bytes(&input_path, &[0, 1, 2, 3, 4]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-hash-mismatch".to_owned(),
        description: "hash mismatch".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex,
            expected_output_sha256: Some("00".repeat(32)),
        }],
        operations: vec![PatchOperation::WriteSpan(WriteSpan {
            offset: 2,
            length: 2,
            bytes: vec![9, 9],
        })],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output =
        run_patch_command_with_args_in_dir(&input_path, &recipe_path, &output_path, &[], None);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("output SHA-256 mismatch"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!output_path.exists(), "output file must not be created");
}

#[test]
fn patch_supports_bare_relative_output_filename() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let relative_output = Path::new("out.UPD");
    let output_path = tempdir.path().join(relative_output);

    write_bytes(&input_path, &[0, 1, 2, 3]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-relative-output".to_owned(),
        description: "relative output".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex,
            expected_output_sha256: None,
        }],
        operations: vec![PatchOperation::WriteSpan(WriteSpan {
            offset: 1,
            length: 2,
            bytes: vec![9, 9],
        })],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output = run_patch_command_with_args_in_dir(
        &input_path,
        &recipe_path,
        relative_output,
        &[],
        Some(tempdir.path()),
    );
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output_path.exists(), "expected output file to be created");
}

#[test]
fn patch_refuses_non_regular_input_without_writing_output() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_dir = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");
    fs::create_dir(&input_dir).expect("create directory input");

    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "non-regular-input".to_owned(),
        description: "input is a directory".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: 1,
            sha256_hex: "00".repeat(32),
            expected_output_sha256: None,
        }],
        operations: vec![PatchOperation::WriteSpan(WriteSpan {
            offset: 0,
            length: 1,
            bytes: vec![1],
        })],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output =
        run_patch_command_with_args_in_dir(&input_dir, &recipe_path, &output_path, &[], None);

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success());
    assert!(
        stderr.contains("does not point to a regular file"),
        "stderr: {stderr}"
    );
    assert!(!output_path.exists(), "no output may be written");
}
