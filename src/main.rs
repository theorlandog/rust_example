//! The binary: parse arguments, call the library, turn the result into an exit
//! code.
//!
//! Everything here is about being a well-behaved command line program:
//!
//! * results go to standard output, commentary goes to standard error, so the
//!   tool stays usable in a pipeline even with `-v`;
//! * output is buffered once and flushed explicitly, because a failed flush is
//!   a failed run and must not be discovered in a destructor;
//! * errors print as a chain and exit non-zero, and a closed pipe is not
//!   treated as an error at all;
//! * `main` itself returns [`ExitCode`] rather than calling
//!   [`std::process::exit`], so destructors still run.
//!
//! # Exit codes
//!
//! | Code | Meaning |
//! | --- | --- |
//! | 0 | Success. |
//! | 1 | The command failed (bad config file, I/O error). |
//! | 2 | The command line was not valid; `clap` prints the usage message. |

use std::fs;
use std::io::{self, BufWriter, Write};
use std::process::ExitCode;

use clap::{CommandFactory, Parser};

use rust_example::cli::{Cli, Command, GlobalArgs, HelloArgs};
use rust_example::config::{self, Config, Origin, Settings};
use rust_example::{Error, Result, greeting};

/// The installed command name, taken from `Cargo.toml` so the two cannot drift.
const NAME: &str = env!("CARGO_BIN_NAME");

fn main() -> ExitCode {
    // `parse` handles `--help`, `--version`, and usage errors by printing and
    // exiting (with code 2 for a usage error) before returning here.
    let cli = Cli::parse();

    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        // `rust-example --repeat 100 | head -1` is a success, not a failure.
        Err(err) if err.is_broken_pipe() => ExitCode::SUCCESS,
        Err(err) => {
            report(&err);
            ExitCode::FAILURE
        }
    }
}

/// Runs the requested command.
fn run(cli: Cli) -> Result<()> {
    let Cli {
        command,
        greeting: args,
        global,
    } = cli;

    // Lock and buffer standard output once. Without the buffer, `--repeat 100`
    // is 100 write syscalls; without the lock, every `writeln!` re-acquires it.
    let stdout = io::stdout();
    let mut out = BufWriter::new(stdout.lock());

    match command {
        // No subcommand: the primary action.
        None => {
            let (settings, _) = resolve(&args, &global)?;
            greeting::write_greeting(&mut out, &settings).map_err(Error::Output)?;
        }
        Some(Command::Config) => {
            let (settings, origin) = resolve(&args, &global)?;
            write_config(&mut out, &settings, &origin)?;
        }
        Some(Command::Completions { shell, out_dir }) => {
            write_completions(&mut out, shell, out_dir.as_deref(), &global)?;
        }
    }

    // Flushing explicitly is the point: `BufWriter`'s own flush-on-drop cannot
    // report a failure, so a full disk would otherwise exit 0 with lost output.
    out.flush().map_err(Error::Output)
}

/// Resolves the settings in effect: defaults, then the config file, then the
/// environment and command line (both of which `clap` has already merged into
/// `args`).
fn resolve(args: &HelloArgs, global: &GlobalArgs) -> Result<(Settings, Origin)> {
    let loaded = config::load(global.config.as_deref())?;

    if global.is_verbose(1) {
        match &loaded.origin {
            Origin::File(path) => eprintln!("{NAME}: reading config from {}", path.display()),
            Origin::Defaults => match config::default_path() {
                Some(path) => eprintln!("{NAME}: no config file at {}", path.display()),
                None => eprintln!("{NAME}: no config directory on this platform"),
            },
        }
    }

    // The single line that implements the documented precedence order.
    let settings = Settings::from(Config::from(args).merge(loaded.config));

    if global.is_verbose(2) {
        eprintln!("{NAME}: settings {settings:?}");
    }

    Ok((settings, loaded.origin))
}

/// Writes the settings in effect as a config file, with their origin as a
/// comment so the output stays valid TOML.
fn write_config<W: Write>(out: &mut W, settings: &Settings, origin: &Origin) -> Result<()> {
    let source = match origin {
        Origin::File(path) => path.display().to_string(),
        Origin::Defaults => "built-in defaults".to_owned(),
    };
    let body = toml::to_string(settings).map_err(Error::Encode)?;

    write!(out, "# settings in effect\n# source: {source}\n{body}").map_err(Error::Output)
}

/// Writes a completion script to standard output, or to a file in `out_dir`.
///
/// The directory form is what the packaging scripts call, so the completions
/// shipped in the `.deb` and `.rpm` are generated from the same definition the
/// binary uses.
fn write_completions<W: Write>(
    out: &mut W,
    shell: clap_complete::Shell,
    out_dir: Option<&std::path::Path>,
    global: &GlobalArgs,
) -> Result<()> {
    let mut command = Cli::command();

    let Some(dir) = out_dir else {
        clap_complete::generate(shell, &mut command, NAME, out);
        return Ok(());
    };

    fs::create_dir_all(dir).map_err(Error::Output)?;
    let path = clap_complete::generate_to(shell, &mut command, NAME, dir).map_err(Error::Output)?;

    if global.is_verbose(1) {
        eprintln!("{NAME}: wrote {}", path.display());
    }

    Ok(())
}

/// Prints an error and everything that caused it.
///
/// Each layer added context on the way up ("cannot read config file `x`"), and
/// the bottom layer is the operating system's own message. Printing the whole
/// chain is the difference between a report a user can act on and "os error 2".
fn report(err: &Error) {
    eprintln!("{NAME}: error: {err}");

    let mut source = std::error::Error::source(err);
    while let Some(cause) = source {
        eprintln!("{NAME}:  caused by: {cause}");
        source = cause.source();
    }
}
