//! Owner-input test of the committed BEAT LOOP acceptance tests (`catalog/acceptance/`): stock,
//! stage 5 and stage 7, built here from the official XDJ-700 v1.15 update, in the owner's local
//! emulator with the owner's stick image. Each build passes the test its implementation names (the
//! lit-pad test, for stock), and two runs show the tests telling builds apart: stage 7 fails the
//! lit-pad test (its known limit) and stock fails stage 7's lengths. Nothing but the PASS and FAIL
//! lines is printed, and the builds stay in a temporary directory. Five runs, three at a time; to
//! run locally (about ten minutes):
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

/// One expected run: a test on an update, and how its last line (PASS or the failure) reads.
struct Run {
    test: &'static str,
    update: PathBuf,
    result: &'static str,
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

    let pass = "result: PASS (";
    let runs = [
        Run {
            test: "beat-loop-lit-pad-follows-touch",
            update: update.clone(),
            result: pass,
        },
        Run {
            test: "beat-loop-16-plays-32-pads-select-lengths",
            update: stage5,
            result: pass,
        },
        Run {
            test: "beat-loop-1-to-32-pads-select-lengths",
            update: stage7.clone(),
            result: pass,
        },
        // Stage 7 lights the pad to the right of the touched one, and none for 32.
        Run {
            test: "beat-loop-lit-pad-follows-touch",
            update: stage7,
            result: "failed: 6 of 8 checks",
        },
        // Stock sets 1/2 to 16 beats: every pad but the first misses its stage-7 length, and the
        // first misses its denominator.
        Run {
            test: "beat-loop-1-to-32-pads-select-lengths",
            update,
            result: "failed: 6 of 14 checks",
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
            let last = if output.status.success() {
                stdout.trim_end().lines().last().unwrap_or("").to_owned()
            } else {
                stderr.trim_end().to_owned()
            };
            if !last.contains(run.result) {
                let name = run.update.file_name().expect("name").to_string_lossy();
                failures.push(format!("{} on {name}: {last}\n{stdout}", run.test));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n\n"));
}
