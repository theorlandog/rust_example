//! End-to-end tests: run the real binary and inspect what a user would see.
//!
//! The unit tests inside `src/` cover the logic. These cover the things only a
//! process can have — exit codes, standard output versus standard error, and
//! environment variables — and they are the tests that would catch a wiring
//! mistake in `main.rs` that every unit test happily passes.
//!
//! # No test-only dependencies
//!
//! Two pieces of `cargo` support replace the usual `assert_cmd` and `tempfile`:
//!
//! * `CARGO_BIN_EXE_<name>` is set at compile time for integration tests and
//!   holds the path to the freshly built binary, so there is nothing to locate
//!   at runtime and no chance of testing a stale copy;
//! * `CARGO_TARGET_TMPDIR` is a scratch directory under `target/`, wiped by
//!   `cargo clean`, which is enough for fixture files that are written once.
//!
//! Reach for `assert_cmd` and `predicates` when the assertions get involved
//! enough that their failure output starts to matter.

// Panicking is how a test reports a failure, so the `unwrap_used` and
// `expect_used` lints from Cargo.toml are relaxed here. `clippy.toml` already
// relaxes them inside `#[cfg(test)]` modules, but an integration test is an
// ordinary crate: only `#[test]` functions count as tests, and the helpers
// below are not.
#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "a failed expectation is a failed test"
)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// The binary under test, built by `cargo test` before this runs.
const EXE: &str = env!("CARGO_BIN_EXE_rust-example");

/// Runs the binary with `args` and a deliberately empty environment.
///
/// Clearing the environment matters: a `RUST_EXAMPLE_NAME` exported in the
/// shell that started `cargo test` would otherwise change these results, and a
/// developer's own `~/.config/rust-example/config.toml` would too — which is
/// what the `HOME` override below prevents.
fn run(args: &[&str]) -> Output {
    run_with_env(args, &[])
}

/// Runs the binary with `args` and the extra environment in `env`.
fn run_with_env(args: &[&str], env: &[(&str, &str)]) -> Output {
    let mut command = Command::new(EXE);
    command
        .env_clear()
        // Point the config lookup at a directory that is guaranteed empty, so
        // the test asserts on defaults rather than on whoever is running it.
        .env("HOME", scratch("empty-home"))
        .env("XDG_CONFIG_HOME", scratch("empty-home"))
        .env("USERPROFILE", scratch("empty-home"))
        .env("APPDATA", scratch("empty-home"))
        .args(args);

    for (key, value) in env {
        command.env(key, value);
    }

    command.output().expect("the binary should be runnable")
}

/// A path inside this test run's scratch directory.
fn scratch(name: &str) -> PathBuf {
    Path::new(env!("CARGO_TARGET_TMPDIR")).join(name)
}

/// Writes `contents` to a scratch file and returns its path.
fn fixture(name: &str, contents: &str) -> PathBuf {
    let path = scratch(name);
    fs::create_dir_all(path.parent().expect("scratch paths have a parent"))
        .expect("scratch directory should be creatable");
    fs::write(&path, contents).expect("fixture should be writable");
    path
}

/// The command's standard output, asserting that it exited successfully.
fn stdout_of(output: &Output) -> String {
    assert!(
        output.status.success(),
        "expected success, got {:?}\nstderr: {}",
        output.status.code(),
        String::from_utf8_lossy(&output.stderr),
    );

    String::from_utf8(output.stdout.clone()).expect("output should be UTF-8")
}

#[test]
fn running_with_no_arguments_greets_the_world() {
    assert_eq!(stdout_of(&run(&[])), "Hello, world!\n");
}

#[test]
fn the_name_flag_changes_the_greeting() {
    assert_eq!(stdout_of(&run(&["--name", "Ferris"])), "Hello, Ferris!\n");
    assert_eq!(stdout_of(&run(&["-n", "Ferris"])), "Hello, Ferris!\n");
}

#[test]
fn repeat_and_shout_combine() {
    let output = stdout_of(&run(&["-n", "Ferris", "-r", "3", "--shout"]));

    assert_eq!(output.lines().count(), 3);
    assert!(output.lines().all(|line| line == "HELLO, FERRIS!"));
}

#[test]
fn a_config_file_supplies_values() {
    let config = fixture("greeting.toml", "name = \"Config\"\nrepeat = 2\n");

    let output = stdout_of(&run(&["--config", &config.display().to_string()]));

    assert_eq!(output, "Hello, Config!\nHello, Config!\n");
}

/// The whole point of the layering: each level beats the one below it.
#[test]
fn flags_beat_the_environment_which_beats_the_config_file() {
    let config = fixture("layered.toml", "name = \"from-file\"\n");
    let path = config.display().to_string();

    let from_file = stdout_of(&run(&["-c", &path]));
    let from_env = stdout_of(&run_with_env(
        &["-c", &path],
        &[("RUST_EXAMPLE_NAME", "from-env")],
    ));
    let from_flag = stdout_of(&run_with_env(
        &["-c", &path, "--name", "from-flag"],
        &[("RUST_EXAMPLE_NAME", "from-env")],
    ));

    assert_eq!(from_file, "Hello, from-file!\n");
    assert_eq!(from_env, "Hello, from-env!\n");
    assert_eq!(from_flag, "Hello, from-flag!\n");
}

/// A tri-state flag has to be able to turn a config file setting back *off*.
#[test]
fn no_shout_overrides_a_config_file_that_shouts() {
    let config = fixture("shouty.toml", "shout = true\n");
    let path = config.display().to_string();

    assert_eq!(stdout_of(&run(&["-c", &path])), "HELLO, WORLD!\n");
    assert_eq!(
        stdout_of(&run(&["-c", &path, "--no-shout"])),
        "Hello, world!\n"
    );
}

#[test]
fn the_config_subcommand_prints_a_usable_config_file() {
    let output = stdout_of(&run(&["--name", "Ferris", "--shout", "config"]));

    assert!(output.contains("# source: built-in defaults"), "{output}");
    assert!(output.contains("name = \"Ferris\""), "{output}");
    assert!(output.contains("shout = true"), "{output}");

    // Feeding the output back in must produce the same settings.
    let round_trip = fixture("round-trip.toml", &output);
    let greeting = stdout_of(&run(&["-c", &round_trip.display().to_string()]));

    assert_eq!(greeting, "HELLO, FERRIS!\n");
}

#[test]
fn a_missing_config_file_named_explicitly_is_an_error() {
    let output = run(&["--config", "no/such/file.toml"]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty(), "errors must not go to stdout");
    assert!(stderr.contains("cannot read config file"), "{stderr}");
    assert!(stderr.contains("no/such/file.toml"), "{stderr}");
    assert!(stderr.contains("caused by"), "the cause is shown: {stderr}");
}

#[test]
fn an_invalid_config_file_reports_the_line() {
    let config = fixture("broken.toml", "name = \"unterminated\nrepeat = 1\n");

    let output = run(&["-c", &config.display().to_string()]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("cannot parse config file"), "{stderr}");
}

#[test]
fn an_unknown_key_in_the_config_file_is_rejected() {
    let config = fixture("typo.toml", "nmae = \"Ferris\"\n");

    let output = run(&["-c", &config.display().to_string()]);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert_eq!(output.status.code(), Some(1));
    assert!(stderr.contains("unknown field"), "{stderr}");
}

#[test]
fn a_usage_error_exits_with_two() {
    let output = run(&["--not-a-flag"]);

    assert_eq!(
        output.status.code(),
        Some(2),
        "the conventional exit code for a usage error"
    );
    assert!(String::from_utf8_lossy(&output.stderr).contains("Usage:"));
}

#[test]
fn an_out_of_range_value_is_rejected_with_a_usage_error() {
    assert_eq!(run(&["--repeat", "0"]).status.code(), Some(2));
    assert_eq!(run(&["--repeat", "999"]).status.code(), Some(2));
}

#[test]
fn help_and_version_go_to_stdout_and_exit_zero() {
    let help = stdout_of(&run(&["--help"]));
    assert!(help.contains("Usage: rust-example"), "{help}");

    let version = stdout_of(&run(&["--version"]));
    assert!(
        version.contains(env!("CARGO_PKG_VERSION")),
        "the version comes from Cargo.toml: {version}"
    );
}

#[test]
fn diagnostics_go_to_standard_error_not_standard_output() {
    let output = run(&["-v", "--name", "Ferris"]);

    assert_eq!(stdout_of(&output), "Hello, Ferris!\n");
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("no config file at"),
        "-v explains where it looked"
    );
}

#[test]
fn quiet_silences_diagnostics_without_touching_results() {
    let output = run(&["-q", "--name", "Ferris"]);

    assert_eq!(stdout_of(&output), "Hello, Ferris!\n");
    assert!(output.stderr.is_empty(), "-q leaves standard error clean");
}

#[test]
fn completion_scripts_are_generated_for_every_supported_shell() {
    for shell in ["bash", "zsh", "fish", "powershell", "elvish"] {
        let script = stdout_of(&run(&["completions", shell]));

        assert!(
            script.contains("rust-example"),
            "the {shell} script should mention the binary name"
        );
    }
}

#[test]
fn completion_scripts_can_be_written_to_a_directory() {
    let dir = scratch("completions");

    let output = run(&[
        "completions",
        "bash",
        "--out-dir",
        &dir.display().to_string(),
    ]);

    assert!(output.status.success());
    assert!(
        dir.join("rust-example.bash").is_file(),
        "packaging depends on this exact file name"
    );
}
