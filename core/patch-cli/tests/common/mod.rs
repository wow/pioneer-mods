use std::fs::File;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Output};

pub fn write_bytes(path: &Path, bytes: &[u8]) {
    let mut file = File::create(path).expect("create file");
    file.write_all(bytes).expect("write file");
    file.flush().expect("flush file");
}

pub fn run_patch_command_with_args_in_dir(
    input: &Path,
    recipe: &Path,
    output: &Path,
    extra_args: &[&str],
    current_dir: Option<&Path>,
) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    cmd.arg("patch")
        .arg("--input")
        .arg(input)
        .arg("--recipe")
        .arg(recipe)
        .arg("--output")
        .arg(output);
    if let Some(current_dir) = current_dir {
        cmd.current_dir(current_dir);
    }
    cmd.args(extra_args);
    cmd.output().expect("run patch-cli patch")
}
