use anyhow::{Context, Result, bail};
use patch_cli::output::{Overwrite, ensure_safe_output_path, write_output_atomically};
use patch_core::xdj700::{
    OFFICIAL_V115, OFFICIAL_V115_LABEL, is_label_higher, rebuild_with_stock_application,
    rebuild_with_stock_application_reporting, validate_reported_version,
};
use patch_core::{RebuildError, firmware_file_name, open_regular_file};
use std::io::Read;
use std::path::{Path, PathBuf};

/// Where the rebuilt application comes from.
#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum ApplicationSource {
    /// The input's own application, re-encoded: a no-op rebuild, unless `--report-version` changes
    /// its version string.
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

    /// MAIN version label for the rebuilt update, e.g. `Ver1.16`. Required, with no default: the
    /// updater writes only versions higher than the installed one.
    #[arg(long)]
    pub label: String,

    /// Path for the rebuilt update. Must not exist; it is never overwritten.
    #[arg(long)]
    pub output: PathBuf,

    /// Version the application reports about itself (`X.YY`, lower than 1.15), e.g. `0.10`.
    /// Only the application's version string changes. The unit then reports this version, so the
    /// official v1.15 update is written over it and restores stock (observed; see the guide's
    /// stages 3 and 4). Without it the application keeps `1.15`.
    #[arg(long, value_name = "X.YY")]
    pub report_version: Option<String>,
}

pub fn rebuild(args: RebuildArgs) -> Result<()> {
    // Also validates the label's form.
    if !is_label_higher(&args.label, OFFICIAL_V115_LABEL)? {
        eprintln!(
            "warning: label {} is not higher than the official {OFFICIAL_V115_LABEL}; a unit \
             running official v1.15 or later skips it (MAIN jumps straight to 100% and nothing \
             is written). See docs/xdj700-flashing.md",
            args.label
        );
    }
    if let Some(version) = &args.report_version {
        validate_reported_version(version)?;
    }
    ensure_safe_output_path(&args.input, &args.output, Overwrite::Never)?;
    let input = read_official_input(&args.input)?;

    let rebuilt = match (args.application, &args.report_version) {
        (ApplicationSource::Stock, None) => {
            rebuild_with_stock_application(&input, &OFFICIAL_V115, &args.label)
        }
        (ApplicationSource::Stock, Some(version)) => {
            rebuild_with_stock_application_reporting(&input, &OFFICIAL_V115, version, &args.label)
        }
    }
    .map_err(|error| refusal(&args.input, error))?;

    write_output_atomically(&args.output, rebuilt.bytes(), Overwrite::Never)?;

    println!("release: XDJ-700 v1.15 (official)");
    println!("input_file: {}", firmware_file_name(&args.input));
    println!("input_sha256_hex: {}", OFFICIAL_V115.upd_sha256);
    match &args.report_version {
        None => println!("application: stock (re-encoded, unchanged)"),
        Some(_) => println!("application: stock with only its version string changed"),
    }
    println!(
        "application_reported_version: {}",
        rebuilt.application_reported_version().unwrap_or("none")
    );
    println!("application_sha256_hex: {}", rebuilt.application_sha256());
    println!("version_label: {}", args.label);
    println!("main_image_len: {}", rebuilt.main_image_len());
    println!("main_image_sha256_hex: {}", rebuilt.main_image_sha256());
    println!("output_file: {}", args.output.display());
    println!("output_len: {}", rebuilt.bytes().len());
    println!("output_sha256_hex: {}", rebuilt.sha256());
    println!(
        "verified: rebuild re-parsed and checked against the input; file read back through the \
         file system before it was renamed into place"
    );
    Ok(())
}

/// Reads the input only if its length is the official file's, checked on the open handle, so
/// an arbitrary large file is refused without being read. The hash is checked by the library.
fn read_official_input(path: &Path) -> Result<Vec<u8>> {
    let read_failed = || format!("failed to read input update '{}'", path.display());
    let file = open_regular_file(path).with_context(read_failed)?;
    let len = file.metadata().with_context(read_failed)?.len();
    if len != OFFICIAL_V115.upd_len as u64 {
        bail!(
            "refusing to rebuild '{}': it is not the official XDJ-700 v1.15 update ({len} bytes, \
             expected {}); only that exact file can be rebuilt",
            path.display(),
            OFFICIAL_V115.upd_len
        );
    }
    let mut bytes = Vec::with_capacity(OFFICIAL_V115.upd_len);
    // One byte more than expected, so a file that grew after the check is still caught.
    file.take(len + 1)
        .read_to_end(&mut bytes)
        .with_context(read_failed)?;
    Ok(bytes)
}

fn refusal(path: &Path, error: RebuildError) -> anyhow::Error {
    match error {
        RebuildError::UnpinnedInput { sha256 } => not_official(path, &sha256),
        other => {
            anyhow::Error::new(other).context(format!("refusing to rebuild '{}'", path.display()))
        }
    }
}

fn not_official(path: &Path, sha256: &str) -> anyhow::Error {
    anyhow::anyhow!(
        "refusing to rebuild '{}': it is not the official XDJ-700 v1.15 update (SHA-256 \
         {sha256}); only that exact file can be rebuilt",
        path.display()
    )
}
