//! Owner-input test of the committed BEAT LOOP acceptance tests (`catalog/acceptance/`): stock,
//! stage 5 and stage 7, built here from the official XDJ-700 v1.15 update, in the owner's local
//! emulator with the owner's stick image. Each build passes the test its implementation names (the
//! lit-pad test, for stock) and fails the others at the steps its behaviour predicts (stages 5 and
//! 7 fail the lit-pad test, their known limit). Nothing but the PASS and FAIL lines is printed,
//! and the builds stay in a temporary directory. Nine runs, three at a time; to run locally (about
//! fifteen minutes):
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
use official_pins::UPD_ENV;
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
    let output_path = dir.join(format!("{recipe}.UPD"));
    let output = cli()
        .args(["patch", "--no-protected-set", "--recipe"])
        .arg(format!("recipes/xdj700-v1.15/{recipe}.json"))
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

    let (lengths_32, lengths_16, lit) = (
        "beat-loop-1-to-32-pads-select-lengths",
        "beat-loop-16-plays-32-pads-select-lengths",
        "beat-loop-lit-pad-follows-touch",
    );
    // Every build against every test. A length test polls the part of the length each touch
    // changes, and expects the other; the lit-pad test waits for each touch to be taken, then
    // expects the six pads' states.
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
            fails: &[23, 25, 28, 31, 34, 37],
        },
        Run {
            test: lengths_16,
            update: update.clone(),
            fails: &[37],
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
            fails: &[23, 25, 28, 31, 34],
        },
        Run {
            test: lit,
            update: stage5,
            fails: &[68],
        },
        // Stage 7: 1 to 32 beats; the pad to the right of the touched one lights, none for 32.
        Run {
            test: lengths_32,
            update: stage7.clone(),
            fails: &[],
        },
        Run {
            test: lengths_16,
            update: stage7.clone(),
            fails: &[23, 26, 28, 31, 34],
        },
        Run {
            test: lit,
            update: stage7,
            fails: &[23, 24, 32, 33, 41, 42, 50, 51, 59, 60, 68],
        },
    ];
    let mut failures = Vec::new();
    for batch in runs.chunks(3) {
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
            let ended = if run.fails.is_empty() {
                output.status.success() && stdout.contains("result: PASS (")
            } else {
                let count = format!("failed: {} of ", run.fails.len());
                !output.status.success() && stderr.contains(&count)
            };
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
