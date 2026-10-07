mod common;

use common::{run_patch_command_with_args_in_dir, write_bytes};
use patch_core::identify_firmware;
use patch_schema::{OwnerCopyWindow, PatchOperation, RecipeManifest, SupportedFirmware, WriteSpan};
use std::fs;
use std::path::Path;

fn run_patch_command(input: &Path, recipe: &Path, output: &Path) -> std::process::Output {
    run_patch_command_with_args(input, recipe, output, &[])
}

fn run_patch_command_with_args(
    input: &Path,
    recipe: &Path,
    output: &Path,
    extra_args: &[&str],
) -> std::process::Output {
    run_patch_command_with_args_in_dir(input, recipe, output, extra_args, None)
}

#[test]
fn patch_applies_write_and_owner_copy_operations() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");

    write_bytes(&input_path, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let expected_output_sha256 = "cfabc3ba52c8cdba5e051fe575863a284c4cb2a1c58288fd0fd730cf008f36b6";
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-smoke".to_owned(),
        description: "smoke".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex.clone(),
            expected_output_sha256: Some(expected_output_sha256.to_owned()),
        }],
        operations: vec![
            PatchOperation::WriteSpan(WriteSpan {
                offset: 2,
                length: 3,
                bytes: vec![170, 171, 172],
            }),
            PatchOperation::OwnerCopyWindow(OwnerCopyWindow {
                source_offset: 0,
                destination_offset: 8,
                length: 2,
            }),
        ],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output = run_patch_command(&input_path, &recipe_path, &output_path);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("output_sha256_hex:"),
        "stdout: {}",
        String::from_utf8_lossy(&output.stdout)
    );

    let patched = fs::read(output_path).expect("read patched output");
    assert_eq!(patched, vec![0, 1, 170, 171, 172, 5, 6, 7, 0, 1]);
}

#[test]
fn patch_rejects_incompatible_firmware_identity() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");

    write_bytes(&input_path, &[1, 2, 3]);

    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-mismatch".to_owned(),
        description: "mismatch".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: 3,
            sha256_hex: "00".repeat(32),
            expected_output_sha256: None,
        }],
        operations: vec![PatchOperation::WriteSpan(WriteSpan {
            offset: 0,
            length: 1,
            bytes: vec![9],
        })],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output = run_patch_command(&input_path, &recipe_path, &output_path);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("not compatible with recipe"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn patch_rejects_out_of_bounds_operation() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");

    write_bytes(&input_path, &[0, 1, 2]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-out-of-bounds".to_owned(),
        description: "bounds".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex,
            expected_output_sha256: None,
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

    let output = run_patch_command(&input_path, &recipe_path, &output_path);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("out of bounds"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn patch_rejects_overlapping_destination_ranges() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");

    write_bytes(&input_path, &[0, 1, 2, 3, 4]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-overlap".to_owned(),
        description: "overlap".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex,
            expected_output_sha256: None,
        }],
        operations: vec![
            PatchOperation::WriteSpan(WriteSpan {
                offset: 1,
                length: 3,
                bytes: vec![7, 7, 7],
            }),
            PatchOperation::WriteSpan(WriteSpan {
                offset: 3,
                length: 2,
                bytes: vec![8, 8],
            }),
        ],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output = run_patch_command(&input_path, &recipe_path, &output_path);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("overlaps"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn patch_rejects_overwriting_existing_output_without_force() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");

    write_bytes(&input_path, &[0, 1, 2]);
    write_bytes(&output_path, &[9, 9, 9]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-no-overwrite".to_owned(),
        description: "overwrite guard".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex,
            expected_output_sha256: None,
        }],
        operations: vec![PatchOperation::WriteSpan(WriteSpan {
            offset: 1,
            length: 1,
            bytes: vec![7],
        })],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output = run_patch_command(&input_path, &recipe_path, &output_path);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("pass --force to overwrite"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn patch_allows_overwrite_with_force() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");

    write_bytes(&input_path, &[0, 1, 2]);
    write_bytes(&output_path, &[9, 9, 9]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-force-overwrite".to_owned(),
        description: "overwrite with force".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex,
            expected_output_sha256: None,
        }],
        operations: vec![PatchOperation::WriteSpan(WriteSpan {
            offset: 1,
            length: 1,
            bytes: vec![7],
        })],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output = run_patch_command_with_args(&input_path, &recipe_path, &output_path, &["--force"]);
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let patched = fs::read(&output_path).expect("read output");
    assert_eq!(patched, vec![0, 7, 2]);
}

#[test]
fn patch_rejects_output_equal_to_input_path() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");

    write_bytes(&input_path, &[0, 1, 2]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-same-path".to_owned(),
        description: "same path guard".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex,
            expected_output_sha256: None,
        }],
        operations: vec![PatchOperation::WriteSpan(WriteSpan {
            offset: 0,
            length: 1,
            bytes: vec![7],
        })],
    };
    fs::write(
        &recipe_path,
        serde_json::to_vec_pretty(&manifest).expect("serialize recipe"),
    )
    .expect("write recipe");

    let output = run_patch_command(&input_path, &recipe_path, &input_path);
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("same path as input firmware"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
