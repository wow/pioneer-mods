use super::compose::{ComposeJob, compose_checked};
use super::resolve::{note, print_resolution};
use anyhow::{Context, Result};
use patch_cli::catalog::{load_catalog, read_profile, resolve_profile};
use patch_cli::recipe::CheckedRecipe;
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct BuildArgs {
    /// The owner's profile: the player, a skin per screen and the features (format in
    /// docs/catalog.md).
    #[arg(long)]
    pub profile: PathBuf,

    /// Path to the owner-supplied official update of the profile's player.
    #[arg(long)]
    pub input: PathBuf,

    /// Path for the built update.
    #[arg(long)]
    pub output: PathBuf,

    /// The repository root holding `catalog/` and `recipes/`.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,

    /// A protected set (format in docs/recipes.md); every fragment is checked against it before
    /// the input is read. Without it, `XDJ700_PROTECTED_SET` names the file.
    #[arg(long, conflicts_with = "no_protected_set")]
    pub protected_set: Option<PathBuf>,

    /// Skip the protected-set check on purpose (refused otherwise without a set).
    #[arg(long, default_value_t = false)]
    pub no_protected_set: bool,
}

/// Resolves a profile against the catalog and composes its fragments under the profile's label
/// and reported version into one verified update (`docs/catalog.md`, "Building a profile").
/// Everything that needs no firmware is checked before the input is read. Prints the resolution
/// (each item on with its evidence) first, so that a refusal shows what was resolved; then, once
/// the file is written, its identity, what it is and the player's restore plan.
pub fn build(args: BuildArgs) -> Result<()> {
    let catalog = load_catalog(&args.root)?;
    let profile = read_profile(&args.profile)?;
    let refusing = format!("refusing to build profile '{}'", args.profile.display());
    let resolution = resolve_profile(&catalog, &profile).with_context(|| refusing.clone())?;
    let player = catalog
        .catalog()
        .player(&resolution.player)
        .expect("resolution refuses a player the catalog lacks");
    print_resolution(&resolution);
    resolution.buildable().with_context(|| refusing.clone())?;
    let mut recipes = Vec::with_capacity(resolution.fragments.len());
    for fragment in &resolution.fragments {
        let recipe = catalog.recipes()[&fragment.recipe].clone();
        let context = format!("{refusing}: recipe '{}'", fragment.recipe);
        let name = format!("'{}'", fragment.recipe);
        recipes.push((CheckedRecipe::from_recipe(recipe, context)?, name));
    }
    compose_checked(ComposeJob {
        recipes,
        refusing,
        verb: "build",
        input: &args.input,
        output: &args.output,
        label: &resolution.label,
        reported_version: &resolution.reported_version,
        protected_set: args.protected_set.as_deref(),
        no_protected_set: args.no_protected_set,
        pinned_output: resolution.pinned_output(),
    })?;
    println!("{}", note(&resolution));
    let restore = &player.restore;
    println!(
        "restore: the official update ({}, the input), over this build reporting {}: {}",
        player.firmware.file, resolution.reported_version, restore.official
    );
    println!("restore_backup: {}", restore.backup);
    println!("restore_guide: {}", restore.guide);
    Ok(())
}
