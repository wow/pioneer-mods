mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{InspectArgs, PatchArgs, inspect, patch};

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
