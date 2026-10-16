//! Owner-input test of the committed BEAT LOOP acceptance tests (`catalog/acceptance/`): stock,
//! stage 5, stage 7 and the Neon pads skin, built here from the official XDJ-700 v1.15 update, in
//! the owner's local emulator with the owner's stick image. Each build passes the tests its
//! implementation names (the lit-pad test, for stock, stage 7 and the skin) and fails the others
//! at the steps its behaviour predicts (stage 5 fails the lit-pad test, its known limit; the
//! skin, which changes only images, fails the length tests where stock does). Stage 7 as first
//! published, without its lit list, is the lit-pad test's failing control: it shows that the
//! checks that the other pads stay unlit can fail. Nothing but the PASS and FAIL lines is printed,
//! and the builds stay in a temporary directory. Thirteen runs, three at a time (fewer on a host
//! with fewer than six cores); to run locally (about twenty minutes):
//!
//! ```text
//! PIONEER_XDJ700_V115_UPD=/path/to/XDJ700.UPD XDJ700_QEMU=/path/to/qemu-system-sh4 \
//!     XDJ700_STICK=/path/to/stick.img \
//!     cargo test -p patch-cli --release --test official_accept_beat_loop -- --ignored
//! ```
//!
//! The stick's first track must load after USB and seven pushes of the rotary selector (the tests'
//! descriptions say so).
#![cfg(unix)]

mod common;
#[path = "../../patch-core/tests/common/official_pins.rs"]
mod official_pins;

use common::repo_root;
use official_pins::{
    LIT_CODES_OFFSET, STAGE7_FIRST_APPLICATION_SHA256, STAGE7_FIRST_REPORTED_VERSION,
    STAGE7_FIRST_UPD_SHA256, UPD_ENV,
};
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn env_path(name: &str) -> PathBuf {
    std::env::var_os(name)
        .map(PathBuf::from)
        .unwrap_or_else(|| panic!("set {name}; see the module docs"))
}

fn cli() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_patch-cli"));
    command.current_dir(repo_root());
    command
}

/// Builds the stage file of `recipe` from the official update into `dir`.
fn stage(update: &Path, recipe: &str, dir: &Path) -> PathBuf {
    let path = repo_root().join(format!("recipes/xdj700-v1.15/{recipe}.json"));
    build(update, &path, recipe, dir)
}

/// Builds stage 7 as first published into `dir`: the committed recipe without its first
/// replacement (the lit list), reporting `0.12`, pinned to that file. The lit-pad test's failing
/// control: its pads light the stock way.
fn stage7_first(update: &Path, dir: &Path) -> PathBuf {
    let path = repo_root().join("recipes/xdj700-v1.15/beat-loop-1-to-32.json");
    let text = std::fs::read_to_string(&path).expect("read the stage-7 recipe");
    let mut recipe: serde_json::Value = serde_json::from_str(&text).expect("recipe");
    let replacements = recipe["replacements"].as_array_mut().expect("replacements");
    let lit = replacements.remove(0);
    assert_eq!(
        lit["offset"], LIT_CODES_OFFSET,
        "the first replacement is the lit list"
    );
    recipe["reported_version"] = STAGE7_FIRST_REPORTED_VERSION.into();
    recipe["expected"] = serde_json::json!({
        "application_sha256": STAGE7_FIRST_APPLICATION_SHA256,
        "upd_sha256": STAGE7_FIRST_UPD_SHA256,
    });
    let first = dir.join("beat-loop-1-to-32-first.json");
    std::fs::write(&first, serde_json::to_vec(&recipe).expect("json")).expect("write");
    build(update, &first, "beat-loop-1-to-32-first", dir)
}

/// Builds the update of the recipe at `path` from the official update into `dir`, as `name`.
fn build(update: &Path, path: &Path, name: &str, dir: &Path) -> PathBuf {
    let output_path = dir.join(format!("{name}.UPD"));
    let output = cli()
        .args(["patch", "--no-protected-set", "--recipe"])
        .arg(path)
        .arg("--input")
        .arg(update)
        .arg("--output")
        .arg(&output_path)
        .output()
        .expect("run patch-cli patch");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output_path
}

/// One expected run: a test on an update, and the steps whose checks fail (none for a PASS).
struct Run {
    test: &'static str,
    update: PathBuf,
    fails: &'static [usize],
}

/// The steps whose checks failed, from the runner's lines (`steps[N] ...: FAIL (...)`).
fn failed_steps(stdout: &str) -> Vec<usize> {
    stdout
        .lines()
        .filter(|line| line.contains(": FAIL ("))
        .filter_map(|line| line.strip_prefix("steps[")?.split(']').next()?.parse().ok())
        .collect()
}

fn accept(run: &Run, emulator: &Path, stick: &Path) -> Output {
    cli()
        .args(["accept", "--test", run.test, "--update"])
        .arg(&run.update)
        .arg("--emulator")
        .arg(emulator)
        .arg("--stick")
        .arg(stick)
        .output()
        .expect("run patch-cli accept")
}

#[test]
#[ignore = "needs owner-supplied firmware, the owner's emulator and stick image; see module docs"]
fn each_build_passes_its_beat_loop_tests_and_the_tests_tell_builds_apart() {
    let update = env_path(UPD_ENV);
    let (emulator, stick) = (env_path("XDJ700_QEMU"), env_path("XDJ700_STICK"));
    let builds = tempfile::tempdir().expect("tempdir");
    let stage5 = stage(&update, "beat-loop-16-plays-32", builds.path());
    let stage7 = stage(&update, "beat-loop-1-to-32", builds.path());
    let stage7_first = stage7_first(&update, builds.path());
    let neon_pads = stage(&update, "skin-perform-neon-pads", builds.path());

    let (lengths_32, lengths_16, lit) = (
        "beat-loop-1-to-32-pads-select-lengths",
        "beat-loop-16-plays-32-pads-select-lengths",
        "beat-loop-lit-pad-follows-touch",
    );
    // Every build against every test. Each poll waits for a value no earlier touch can leave (the
    // touched pad's index; for the first pad, its length or its lit state), so the failing steps
    // do not depend on timing.
    let runs = [
        // Stock sets 1/2 to 16 beats and lights the touched pad.
        Run {
            test: lit,
            update: update.clone(),
            fails: &[],
        },
        Run {
            test: lengths_32,
            update: update.clone(),
            fails: &[23, 26, 30, 34, 38, 42],
        },
        Run {
            test: lengths_16,
            update: update.clone(),
            fails: &[42],
        },
        // Stage 5: the sixth pad sets 32 beats and lights none.
        Run {
            test: lengths_16,
            update: stage5.clone(),
            fails: &[],
        },
        Run {
            test: lengths_32,
            update: stage5.clone(),
            fails: &[23, 26, 30, 34, 38],
        },
        Run {
            test: lit,
            update: stage5,
            fails: &[79],
        },
        // Stage 7: 1 to 32 beats, and the touched pad lights (its lit list follows the table).
        Run {
            test: lengths_32,
            update: stage7.clone(),
            fails: &[],
        },
        Run {
            test: lengths_16,
            update: stage7.clone(),
            fails: &[23, 26, 30, 34, 38],
        },
        Run {
            test: lit,
            update: stage7,
            fails: &[],
        },
        // Stage 7 as first published, without the lit list: the pad to the right of the touched
        // one lights, none for 32. The failing control for the checks that other pads stay unlit.
        Run {
            test: lit,
            update: stage7_first,
            fails: &[28, 30, 39, 40, 49, 50, 59, 60, 69, 70, 79],
        },
        // The Neon pads skin changes only images: the pads light (the test it names) and set
        // lengths as on stock.
        Run {
            test: lit,
            update: neon_pads.clone(),
            fails: &[],
        },
        Run {
            test: lengths_32,
            update: neon_pads.clone(),
            fails: &[23, 26, 30, 34, 38, 42],
        },
        Run {
            test: lengths_16,
            update: neon_pads,
            fails: &[42],
        },
    ];
    let mut failures = Vec::new();
    // Each emulator keeps two host cores busy (the SH-4 and the DSP), and the tests' fixed boot
    // waits assume an unloaded host: at most three at once, fewer on a smaller host.
    let cores = std::thread::available_parallelism().map_or(2, std::num::NonZero::get);
    let at_once = (cores / 2).clamp(1, 3);
    for batch in runs.chunks(at_once) {
        let outputs: Vec<Output> = std::thread::scope(|scope| {
            let handles: Vec<_> = batch
                .iter()
                .map(|run| scope.spawn(|| accept(run, &emulator, &stick)))
                .collect();
            handles
                .into_iter()
                .map(|h| h.join().expect("run"))
                .collect()
        });
        for (run, output) in batch.iter().zip(outputs) {
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let failed = failed_steps(&stdout);
            // A run fails exactly when a check fails; the failing steps are compared below.
            let ended = output.status.success() == run.fails.is_empty();
            if !ended || failed != run.fails {
                let name = run.update.file_name().expect("name").to_string_lossy();
                failures.push(format!(
                    "{} on {name}: failed steps {failed:?}, expected {:?}\n{stdout}{stderr}",
                    run.test, run.fails
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
