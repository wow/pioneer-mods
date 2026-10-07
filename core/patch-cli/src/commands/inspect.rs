use anyhow::{Context, Result};
use clap::ValueEnum;
use patch_core::identify_firmware;
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct InspectArgs {
    /// Path to the owner-supplied firmware file.
    #[arg(long)]
    pub input: PathBuf,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    pub format: OutputFormat,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
pub enum OutputFormat {
    Text,
    Json,
}

pub fn inspect(args: InspectArgs) -> Result<()> {
    let identity = identify_firmware(&args.input).with_context(|| {
        format!(
            "failed to inspect firmware identity for '{}'",
            args.input.display()
        )
    })?;

    match args.format {
        OutputFormat::Text => {
            println!("file_name: {}", identity.file_name);
            println!("size_bytes: {}", identity.size_bytes);
            println!("sha256_hex: {}", identity.sha256_hex);
        }
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&identity)
                .context("failed to serialize inspection output as JSON")?;
            println!("{json}");
        }
    }

    Ok(())
}
