use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use patch_core::identify_firmware;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "patch-cli",
    about = "Firmware patching CLI for owner-input Pioneer firmware workflows",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Inspect firmware identity (file name, size, SHA-256).
    Inspect(InspectArgs),
    /// Apply a patch recipe to a compatible firmware input.
    Patch(PatchArgs),
}

#[derive(clap::Args, Debug)]
struct InspectArgs {
    /// Path to the owner-supplied firmware file.
    #[arg(long)]
    input: PathBuf,

    /// Output format.
    #[arg(long, value_enum, default_value_t = OutputFormat::Text)]
    format: OutputFormat,
}

#[derive(clap::Args, Debug)]
struct PatchArgs {
    /// Path to the owner-supplied firmware file.
    #[arg(long)]
    input: PathBuf,

    /// Path to a recipe manifest.
    #[arg(long)]
    recipe: PathBuf,

    /// Path for generated patched firmware output.
    #[arg(long)]
    output: PathBuf,
}

#[derive(Copy, Clone, Debug, Eq, PartialEq, ValueEnum)]
enum OutputFormat {
    Text,
    Json,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli)
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Inspect(args) => inspect(args),
        Commands::Patch(args) => patch(args),
    }
}

fn inspect(args: InspectArgs) -> Result<()> {
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

fn patch(args: PatchArgs) -> Result<()> {
    let _ = (args.input, args.recipe, args.output);
    bail!(
        "the 'patch' command is not implemented yet; this milestone currently ships 'inspect' only"
    )
}
