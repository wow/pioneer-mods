use patch_core::identify_firmware;
use patch_schema::{OwnerCopyWindow, PatchOperation, RecipeManifest, SupportedFirmware, WriteSpan};
use std::fs;
use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::Command;

fn write_bytes(path: &Path, bytes: &[u8]) {
    let mut file = File::create(path).expect("create file");
    file.write_all(bytes).expect("write file");
    file.flush().expect("flush file");
}

fn run_patch_command(input: &Path, recipe: &Path, output: &Path) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_patch-cli"))
        .arg("patch")
        .arg("--input")
        .arg(input)
        .arg("--recipe")
        .arg(recipe)
        .arg("--output")
        .arg(output)
        .output()
        .expect("run patch-cli patch")
}

#[test]
fn patch_applies_write_and_owner_copy_operations() {
    let tempdir = tempfile::tempdir().expect("create tempdir");
    let input_path = tempdir.path().join("XDJ700.UPD");
    let recipe_path = tempdir.path().join("recipe.json");
    let output_path = tempdir.path().join("XDJ700-patched.UPD");

    write_bytes(&input_path, &[0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);

    let identity = identify_firmware(&input_path).expect("identify input");
    let manifest = RecipeManifest {
        schema_version: 1,
        recipe_id: "xdj700-smoke".to_owned(),
        description: "smoke".to_owned(),
        targets: vec![SupportedFirmware {
            model: "XDJ-700".to_owned(),
            version: "1.15".to_owned(),
            size_bytes: identity.size_bytes,
            sha256_hex: identity.sha256_hex.clone(),
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
