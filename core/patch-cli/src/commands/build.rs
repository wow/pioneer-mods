use super::compose::{ComposeJob, compose_checked};
use super::resolve::print_resolution;
use anyhow::{Context, Result, bail};
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
/// Everything that needs no firmware is checked before the input is read. Reports the
/// resolution (each item on with its evidence), the output's identity and the restore plan.
pub fn build(args: BuildArgs) -> Result<()> {
    let catalog = load_catalog(&args.root)?;
    let profile = read_profile(&args.profile)?;
    let refusing = format!("refusing to build profile '{}'", args.profile.display());
    let resolution = resolve_profile(&catalog, &profile).with_context(|| refusing.clone())?;
    print_resolution(&resolution);
    let Some(tier) = resolution.tier() else {
        bail!("{refusing}: nothing to build; every choice is off or keeps the stock skin");
    };
    if tier < profile.maturity {
        bail!(
            "{refusing}: the build is {tier} (a combination, or a recipe under another label or \
             reported version than its own, is a new update and experimental at most); the \
             profile accepts {}",
            profile.maturity.accepted()
        );
    }
    let mut recipes = Vec::with_capacity(resolution.fragments.len());
    let mut names = Vec::with_capacity(resolution.fragments.len());
    for fragment in &resolution.fragments {
        let recipe = catalog.recipes()[&fragment.recipe].clone();
        let context = format!("{refusing}: recipe '{}'", fragment.recipe);
        recipes.push(CheckedRecipe::from_recipe(recipe, context)?);
        names.push(format!("'{}'", fragment.recipe));
    }
    compose_checked(ComposeJob {
        recipes,
        names,
        refusing,
        verb: "build",
        input: &args.input,
        output: &args.output,
        label: &resolution.label,
        reported_version: &resolution.reported_version,
        protected_set: args.protected_set.as_deref(),
        no_protected_set: args.no_protected_set,
    })?;
    // The resolution's player is in the catalog; resolution refuses one it lacks.
    let player = catalog.catalog().player(&resolution.player);
    let file = player.map_or("its file", |player| player.firmware.file.as_str());
    println!(
        "restore: the official update ({file}, the input) brings back the stock application: the \
         build reports {}, lower than the official release, so the unit accepts the official \
         file over it; keep the stock no-op stick as the backup (docs/xdj700-flashing.md, \
         section 3, step 5)",
        resolution.reported_version
    );
    Ok(())
}
