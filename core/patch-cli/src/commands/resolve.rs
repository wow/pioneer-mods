use anyhow::{Context, Result};
use patch_cli::catalog::{load_catalog, read_profile, resolve_profile};
use patch_schema::catalog::{Resolution, Status};
use std::path::PathBuf;

#[derive(clap::Args, Debug)]
pub struct ResolveArgs {
    /// The owner's profile: the player, a skin per screen and the features (format in
    /// docs/catalog.md).
    #[arg(long)]
    pub profile: PathBuf,

    /// The repository root holding `catalog/` and `recipes/`.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Resolves a profile against the catalog and prints every choice, on or off with its reason, and
/// the fragments a build composes. Needs no firmware and writes nothing.
pub fn resolve(args: ResolveArgs) -> Result<()> {
    let catalog = load_catalog(&args.root)?;
    let profile = read_profile(&args.profile)?;
    let resolution = resolve_profile(&catalog, &profile)
        .with_context(|| format!("refusing profile '{}'", args.profile.display()))?;
    print_resolution(&resolution);
    if resolution.tier().is_some() {
        println!("{}", note(&resolution));
    }
    Ok(())
}

/// Prints every choice of `resolution`, on (with its evidence and limits) or off with its reason,
/// its fragments and its tier. What a build of it is ([`note`]) is for the caller to print:
/// `build` prints it only once the file is written.
pub(super) fn print_resolution(resolution: &Resolution) {
    println!("player: {}", resolution.player);
    println!("label: {}", resolution.label);
    println!("reported_version: {}", resolution.reported_version);
    println!("accepts: {}", resolution.maturity.accepted());
    for screen in &resolution.screens {
        let status = describe(&screen.status);
        match screen.status {
            Status::Stock => println!("screen {}: stock", screen.screen),
            Status::Off { .. } => println!(
                "screen {}: skin {} {status}; it keeps the stock skin",
                screen.screen, screen.chosen
            ),
            Status::On { .. } => {
                println!("screen {}: skin {} {status}", screen.screen, screen.chosen)
            }
        }
        print_backing(&screen.status);
    }
    for screen in &resolution.missing_screens {
        println!(
            "screen {screen}: not on {}; the profile's choice for it is ignored",
            resolution.player
        );
    }
    for feature in &resolution.features {
        println!("feature {}: {}", feature.feature, describe(&feature.status));
        print_backing(&feature.status);
    }
    println!("fragments: {}", resolution.fragments.len());
    for (index, fragment) in resolution.fragments.iter().enumerate() {
        let new_update = match (fragment.as_pinned, &fragment.pinned_update) {
            (true, _) => "",
            (false, None) => "; the recipe pins no update file",
            (false, Some(_)) => "; under another label or reported version than its own",
        };
        println!(
            "fragment[{index}]: {} ({}; {}{new_update})",
            fragment.recipe,
            fragment.maturity,
            fragment.builds.join(", "),
        );
    }
    match resolution.tier() {
        None => println!("tier: none; nothing to build"),
        Some(tier) => println!("tier: {tier}"),
    }
}

/// What a build of `resolution` is: the very file its one recipe pins, or a new update.
pub(super) fn note(resolution: &Resolution) -> String {
    match resolution.fragments.as_slice() {
        [lone] if lone.as_pinned => format!(
            "note: the build is {}'s own output, under its own label and reported version",
            lone.recipe
        ),
        _ => "note: a build composes these fragments under the profile's label and reported \
              version, a new update to rehearse both ways in emulation before flashing \
              (docs/modular-builds.md)"
            .to_owned(),
    }
}

fn describe(status: &Status) -> String {
    match status {
        Status::On {
            recipe,
            maturity: m,
            ..
        } => format!("on ({m}, {recipe})"),
        Status::Stock => "stock".to_owned(),
        Status::Off { reason } => format!("off: {reason}"),
    }
}

/// What backs an item that is on: the evidence for its maturity, and its known limits.
fn print_backing(status: &Status) {
    if let Status::On {
        evidence, limits, ..
    } = status
    {
        println!("  evidence: {evidence}");
        for limit in limits {
            println!("  limit: {limit}");
        }
    }
}
