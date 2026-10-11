mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};
use commands::{
    BuildArgs, ComposeArgs, InspectArgs, PatchArgs, PreconditionArgs, RebuildArgs, ResolveArgs,
    build, compose, inspect, patch, precondition, rebuild, resolve,
};

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
    /// Rebuild the official XDJ-700 v1.15 update around an application (verified, never
    /// overwrites).
    Rebuild(RebuildArgs),
    /// Print the precondition hashes of a schema-v2 recipe for completing a draft, computed on the
    /// official update after the recipe, committed-recipe and leak checks (writes nothing).
    Precondition(PreconditionArgs),
    /// Compose several schema-v2 recipes into one update, each checked against its own pinned
    /// output (verified, never overwrites).
    Compose(ComposeArgs),
    /// Resolve a profile against the catalog: every chosen skin and feature, on or off with its
    /// reason, and the fragments a build composes (needs no firmware, writes nothing).
    Resolve(ResolveArgs),
    /// Build a profile: resolve it against the catalog and compose its fragments under its label
    /// and reported version into one update (verified, never overwrites).
    Build(BuildArgs),
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    run(cli)
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Commands::Inspect(args) => inspect(args),
        Commands::Patch(args) => patch(args),
        Commands::Rebuild(args) => rebuild(args),
        Commands::Precondition(args) => precondition(args),
        Commands::Compose(args) => compose(args),
        Commands::Resolve(args) => resolve(args),
        Commands::Build(args) => build(args),
    }
}
