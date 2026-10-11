use anyhow::{Context, Result, bail};
use patch_cli::accept::{
    Qemu, QemuSetup, flash_image, private_work_dir, profile, read_update, run_test,
};
use patch_cli::catalog::load_catalog;
use patch_core::{firmware_file_name, sha256_hex};
use std::path::PathBuf;

/// Names the emulator binary when `--emulator` is not given.
pub const EMULATOR_ENV: &str = "XDJ700_QEMU";

#[derive(clap::Args, Debug)]
pub struct AcceptArgs {
    /// The acceptance test to run, by id (`catalog/acceptance/<id>.json`).
    #[arg(long)]
    pub test: String,

    /// The update to run it on: any update of the test's player (the official file, a stage
    /// file, a build).
    #[arg(long)]
    pub update: PathBuf,

    /// The owner's emulator binary (`qemu-system-sh4` of the local build). Without it,
    /// `XDJ700_QEMU` names it.
    #[arg(long)]
    pub emulator: Option<PathBuf>,

    /// The owner's USB stick image, for a test that needs one (`stick`). The emulator's writes
    /// go to a throwaway overlay.
    #[arg(long)]
    pub stick: Option<PathBuf>,

    /// The repository root holding `catalog/`.
    #[arg(long, default_value = ".")]
    pub root: PathBuf,
}

/// Runs one acceptance test on an update in the local emulator (`docs/acceptance.md`, "Running a
/// test"): every check that needs no firmware first, then the run, each check reported PASS or
/// FAIL. The flash image, frames and the emulator's log stay in a temporary directory, removed
/// afterwards; the update is never changed.
pub fn accept(args: AcceptArgs) -> Result<()> {
    let catalog = load_catalog(&args.root)?;
    let refusing = format!("refusing to run acceptance test '{}'", args.test);
    let test = catalog
        .catalog()
        .acceptance_test(&args.test)
        .with_context(|| format!("{refusing}: it is not in the catalog"))?;
    let player = catalog
        .catalog()
        .player(&test.player)
        .expect("the catalog check holds every test to a player in the catalog");
    let Some(profile) = profile(&player.id) else {
        bail!(
            "{refusing}: the runner has no emulator profile for player {}",
            player.id
        );
    };
    let emulator = args
        .emulator
        .or_else(|| std::env::var_os(EMULATOR_ENV).map(PathBuf::from))
        .with_context(|| {
            format!("{refusing}: name the emulator with --emulator or {EMULATOR_ENV}")
        })?;
    if !emulator.is_file() {
        bail!(
            "{refusing}: the emulator '{}' is not a file",
            emulator.display()
        );
    }
    match (&args.stick, test.stick) {
        (None, true) => bail!("{refusing}: it needs the owner's USB stick image (--stick)"),
        (Some(stick), _) if !stick.is_file() => {
            bail!(
                "{refusing}: the stick image '{}' is not a file",
                stick.display()
            )
        }
        _ => {}
    }

    let update = read_update(&args.update)?;
    let flash = flash_image(&update, profile.flash_size)
        .with_context(|| format!("{refusing}: '{}'", args.update.display()))?;
    println!("test: {} ({})", test.id, test.player);
    println!("update_file: {}", firmware_file_name(&args.update));
    println!("update_sha256_hex: {}", sha256_hex(&update));
    let work = private_work_dir()?;
    let mut qemu = Qemu::start(QemuSetup {
        emulator,
        profile,
        work: work.path().to_path_buf(),
        flash,
        stick: args.stick,
    })?;
    let outcome = run_test(test, player, &mut qemu);
    drop(qemu);
    let outcome = outcome?;
    for line in &outcome.lines {
        println!("{line}");
    }
    if outcome.failed > 0 {
        bail!(
            "acceptance test {} failed: {} of {} checks",
            test.id,
            outcome.failed,
            outcome.checks
        );
    }
    println!("result: PASS ({} checks)", outcome.checks);
    Ok(())
}
