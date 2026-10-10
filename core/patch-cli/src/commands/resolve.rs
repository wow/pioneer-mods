use anyhow::{Context, Result};
use patch_cli::catalog::{load_catalog, read_profile};
use patch_core::xdj700::{check_label_and_version, recipe_target};
use patch_schema::catalog::{Maturity, Status, resolve as resolve_profile};
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
    let loaded = load_catalog(&args.root)?;
    let profile = read_profile(&args.profile)?;
    let refusing = || format!("refusing profile '{}'", args.profile.display());
    let resolution =
        resolve_profile(&loaded.catalog, &loaded.recipes, &profile).with_context(refusing)?;
    // The release's rules for the label and reported version, as a build would apply them.
    let target = recipe_target(&resolution.player).expect("the loader checked every player");
    check_label_and_version(&profile.label, &profile.reported_version, target)
        .with_context(refusing)?;

    println!("player: {}", resolution.player);
    println!("label: {}", resolution.label);
    println!("reported_version: {}", resolution.reported_version);
    println!(
        "accepts: {}",
        match resolution.maturity {
            Maturity::Stable => "stable only",
            _ => "experimental and stable",
        }
    );
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
        print_limits(&screen.status);
    }
    for feature in &resolution.features {
        println!("feature {}: {}", feature.feature, describe(&feature.status));
        print_limits(&feature.status);
    }
    println!("fragments: {}", resolution.fragments.len());
    for (index, fragment) in resolution.fragments.iter().enumerate() {
        println!(
            "fragment[{index}]: {} ({}; {}{})",
            fragment.recipe,
            fragment.maturity,
            fragment.builds.join(", "),
            if fragment.as_pinned {
                ""
            } else {
                "; under another label or reported version than its own"
            }
        );
    }
    match resolution.tier() {
        None => println!("tier: none; nothing to build"),
        Some(tier) => {
            println!("tier: {tier}");
            println!(
                "note: a build composes these fragments under the profile's label and reported \
                 version; a combination of several is a new update to rehearse both ways in \
                 emulation before flashing (docs/modular-builds.md)"
            );
        }
    }
    Ok(())
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

fn print_limits(status: &Status) {
    if let Status::On { limits, .. } = status {
        for limit in limits {
            println!("  limit: {limit}");
        }
    }
}
