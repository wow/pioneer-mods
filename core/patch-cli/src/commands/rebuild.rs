use anyhow::{Context, Result, bail};
use patch_cli::output::{Overwrite, ensure_safe_output_path, write_output_atomically};
use patch_core::xdj700::{
    OFFICIAL_V115, decode_application, rebuild_with_application, validate_version_label,
};
use patch_core::{firmware_file_name, parse_upd, read_regular_file, sha256_hex};
use std::path::PathBuf;

/// Where the rebuilt application comes from.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationSource {
    /// The input's own application, unchanged (re-encoded only): a no-op rebuild.
    Stock,
}

#[derive(clap::Args, Debug)]
pub struct RebuildArgs {
    /// Path to the owner-supplied official XDJ-700 v1.15 update (`XDJ700.UPD`).
    #[arg(long)]
    pub input: PathBuf,

    /// Application to place in the rebuilt update.
    #[arg(long, value_enum)]
    pub application: ApplicationSource,

    /// MAIN version label for the rebuilt update, e.g. `Ver1.15`. Required: which labels the
    /// device's updater accepts is not yet confirmed, so there is no default.
    #[arg(long)]
    pub label: String,

    /// Path for the rebuilt update. Must not exist; it is never overwritten.
    #[arg(long)]
    pub output: PathBuf,
}

pub fn rebuild(args: RebuildArgs) -> Result<()> {
    validate_version_label(&args.label)?;
    ensure_safe_output_path(&args.input, &args.output, Overwrite::Never)?;
    let input = read_regular_file(&args.input)
        .with_context(|| format!("failed to read input update '{}'", args.input.display()))?;
    let input_sha256 = sha256_hex(&input);
    if input_sha256 != OFFICIAL_V115.upd_sha256 {
        bail!(
            "refusing to rebuild '{}': it is not the official XDJ-700 v1.15 update \
             (SHA-256 {input_sha256}); only that exact file can be rebuilt",
            args.input.display()
        );
    }

    let application = match args.application {
        ApplicationSource::Stock => {
            let container = parse_upd(&input).context("failed to parse the official update")?;
            decode_application(&container).context("failed to decode the official application")?
        }
    };
    let rebuilt =
        rebuild_with_application(&input, &OFFICIAL_V115, application.decoded(), &args.label)
            .with_context(|| format!("refusing to rebuild '{}'", args.input.display()))?;

    let written_sha256 = write_output_atomically(&args.output, rebuilt.bytes(), Overwrite::Never)?;

    println!("release: XDJ-700 v1.15 (official)");
    println!("input_file: {}", firmware_file_name(&args.input));
    println!("input_sha256_hex: {input_sha256}");
    println!("application: stock (re-encoded, unchanged)");
    println!("application_sha256_hex: {}", application.decoded_sha256());
    println!("version_label: {}", args.label);
    println!("main_image_len: {}", rebuilt.main_image_len());
    println!("main_image_sha256_hex: {}", rebuilt.main_image_sha256());
    println!("output_file: {}", args.output.display());
    println!("output_len: {}", rebuilt.bytes().len());
    println!("output_sha256_hex: {written_sha256}");
    println!(
        "verified: rebuild re-parsed and checked against the input; file read back through the \
         file system before it was renamed into place"
    );
    Ok(())
}
