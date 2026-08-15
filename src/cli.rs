//! The command line grammar.
//!
//! This is `clap`'s derive API: the struct *is* the specification, and the help
//! text, `--version`, environment variable fallbacks, value validation, and
//! shell completions are all generated from it. Doc comments become help text,
//! which is why the comments below are written for users rather than for
//! maintainers.
//!
//! Three patterns here are worth copying:
//!
//! * **A primary action with auxiliary subcommands.** The interesting work
//!   happens with no subcommand at all (`rust-example --name Ferris`), while
//!   `config` and `completions` sit alongside it. See [`Cli`].
//! * **Optional-everything arguments.** No `default_value` appears anywhere,
//!   because a default applied by `clap` would be indistinguishable from a
//!   value the user typed, and would therefore always beat the config file.
//!   Defaults live in [`crate::config::Settings`].
//! * **Tri-state flags.** `--shout` and `--no-shout` produce `Some(true)` and
//!   `Some(false)`, and their absence produces `None`, so the command line can
//!   override the config file *in both directions*. See [`HelloArgs::shout`].

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand};
use clap_complete::Shell;

use crate::config::Config;

/// A reference example of a modern Rust command line application.
///
/// With no subcommand, prints a greeting. Settings are resolved from, in
/// increasing order of precedence: built-in defaults, the config file,
/// environment variables, then command line flags.
#[derive(Debug, Parser)]
#[command(
    name = "rust-example",
    version,
    about,
    long_about = None,
    // `--version` on subcommands too, so `rust-example config --version` works.
    propagate_version = true,
)]
pub struct Cli {
    /// The subcommand to run, if any. `None` is the primary action: greet.
    #[command(subcommand)]
    pub command: Option<Command>,

    /// Arguments controlling the greeting.
    ///
    /// These stay useful under `config`, where they show what the settings
    /// *would* be: `rust-example --name Ferris config`.
    #[command(flatten)]
    pub greeting: HelloArgs,

    /// Options accepted anywhere on the command line.
    #[command(flatten)]
    pub global: GlobalArgs,
}

/// The subcommands that do something other than greet.
#[derive(Debug, Subcommand)]
pub enum Command {
    /// Print the settings in effect, and where they came from.
    ///
    /// The output is a valid config file: redirect it to the path shown by
    /// `rust-example -v config` to make the current settings permanent.
    Config,

    /// Print a shell completion script.
    ///
    /// Bash: `rust-example completions bash > /etc/bash_completion.d/rust-example`
    Completions {
        /// The shell to generate a completion script for.
        #[arg(value_name = "SHELL")]
        shell: Shell,

        /// Write to this directory using the shell's conventional file name,
        /// instead of writing the script to standard output.
        #[arg(long, value_name = "DIR")]
        out_dir: Option<PathBuf>,
    },
}

/// Arguments controlling the greeting.
///
/// Every field is optional, because "the user did not say" has to stay
/// distinguishable from "the user said the default value" for the config file
/// to mean anything. The `[default: ...]` notes are written into the doc
/// comments rather than declared with `default_value`, so the help text is
/// honest without the parser inventing values.
#[derive(Debug, Default, Args)]
pub struct HelloArgs {
    /// Who to greet [default: world]
    #[arg(short, long, value_name = "NAME", env = "RUST_EXAMPLE_NAME")]
    pub name: Option<String>,

    /// How many times to print the greeting [default: 1]
    // `value_parser` rejects anything outside the range before `main` runs, so
    // the program itself never has to validate it.
    #[arg(
        short, long,
        value_name = "N",
        env = "RUST_EXAMPLE_REPEAT",
        value_parser = clap::value_parser!(u8).range(1..=100),
    )]
    pub repeat: Option<u8>,

    /// Shout the greeting in uppercase
    #[arg(long, overrides_with = "no_shout")]
    shout: bool,

    /// Do not shout the greeting, overriding `shout` in the config file
    #[arg(long, overrides_with = "shout")]
    no_shout: bool,
}

impl HelloArgs {
    /// The shouting preference expressed on the command line, if any.
    ///
    /// `overrides_with` makes the two flags mutually cancelling, so the last
    /// one wins and `None` genuinely means "the command line is silent".
    #[must_use]
    pub fn shout(&self) -> Option<bool> {
        match (self.shout, self.no_shout) {
            (true, _) => Some(true),
            (_, true) => Some(false),
            _ => None,
        }
    }
}

/// Turns the command line into a configuration layer, so it can be merged with
/// the config file by [`Config::merge`].
impl From<&HelloArgs> for Config {
    fn from(args: &HelloArgs) -> Self {
        Self {
            name: args.name.clone(),
            repeat: args.repeat,
            shout: args.shout(),
        }
    }
}

/// Options that apply to every subcommand.
///
/// `global = true` is what lets them appear before *or* after a subcommand:
/// `rust-example -v config` and `rust-example config -v` both work.
#[derive(Debug, Default, Args)]
pub struct GlobalArgs {
    /// Read this config file instead of the default one
    ///
    /// Unlike the default file, a file named here must exist.
    #[arg(
        short,
        long,
        value_name = "FILE",
        env = "RUST_EXAMPLE_CONFIG",
        global = true
    )]
    pub config: Option<PathBuf>,

    /// Print more detail on standard error; repeat for more (`-vv`)
    #[arg(short, long, action = ArgAction::Count, global = true, conflicts_with = "quiet")]
    pub verbose: u8,

    /// Suppress diagnostics on standard error
    ///
    /// Results still go to standard output; only commentary is silenced.
    #[arg(short, long, global = true)]
    pub quiet: bool,
}

impl GlobalArgs {
    /// Reports whether commentary at `level` should be printed.
    ///
    /// Level 1 is `-v`, level 2 is `-vv`, and so on.
    #[must_use]
    pub fn is_verbose(&self, level: u8) -> bool {
        !self.quiet && self.verbose >= level
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use clap::CommandFactory;

    use super::*;

    /// `clap` can prove the grammar is internally consistent — no duplicate
    /// argument ids, no dangling `overrides_with`, no impossible defaults.
    /// Without this test, those mistakes only surface at runtime.
    #[test]
    fn the_grammar_is_well_formed() {
        Cli::command().debug_assert();
    }

    fn parse(args: &[&str]) -> Cli {
        Cli::try_parse_from(args).expect("should parse")
    }

    #[test]
    fn no_arguments_means_greet_with_nothing_specified() {
        let cli = parse(&["rust-example"]);

        assert!(cli.command.is_none());
        assert_eq!(Config::from(&cli.greeting), Config::default());
    }

    #[test]
    fn a_populated_command_line_becomes_a_config_layer() {
        let cli = parse(&["rust-example", "-n", "Ferris", "-r", "2", "--shout"]);

        assert_eq!(
            Config::from(&cli.greeting),
            Config {
                name: Some("Ferris".to_owned()),
                repeat: Some(2),
                shout: Some(true),
            }
        );
    }

    #[test]
    fn greeting_arguments_still_apply_to_the_config_subcommand() {
        let cli = parse(&["rust-example", "--name", "Ferris", "config"]);

        assert!(matches!(cli.command, Some(Command::Config)));
        assert_eq!(cli.greeting.name.as_deref(), Some("Ferris"));
    }

    #[test]
    fn global_options_are_accepted_on_either_side_of_the_subcommand() {
        for args in [
            ["rust-example", "-v", "--config", "a.toml", "config"],
            ["rust-example", "config", "-v", "--config", "a.toml"],
        ] {
            let cli = parse(&args);

            assert_eq!(cli.global.verbose, 1, "for {args:?}");
            assert_eq!(cli.global.config.as_deref(), Some(Path::new("a.toml")));
        }
    }

    #[test]
    fn the_shout_flags_cancel_each_other_out_last_one_winning() {
        let cases = [
            (vec!["rust-example"], None),
            (vec!["rust-example", "--shout"], Some(true)),
            (vec!["rust-example", "--no-shout"], Some(false)),
            (vec!["rust-example", "--shout", "--no-shout"], Some(false)),
            (vec!["rust-example", "--no-shout", "--shout"], Some(true)),
        ];

        for (args, expected) in cases {
            assert_eq!(parse(&args).greeting.shout(), expected, "for {args:?}");
        }
    }

    #[test]
    fn repeat_is_validated_before_main_runs() {
        assert!(Cli::try_parse_from(["rust-example", "--repeat", "0"]).is_err());
        assert!(Cli::try_parse_from(["rust-example", "--repeat", "101"]).is_err());
        assert!(Cli::try_parse_from(["rust-example", "--repeat", "many"]).is_err());
        assert!(Cli::try_parse_from(["rust-example", "--repeat", "100"]).is_ok());
    }

    #[test]
    fn an_unknown_flag_is_rejected() {
        assert!(Cli::try_parse_from(["rust-example", "--nmae", "Ferris"]).is_err());
    }

    #[test]
    fn quiet_and_verbose_cannot_be_combined() {
        assert!(Cli::try_parse_from(["rust-example", "-v", "-q"]).is_err());
    }

    #[test]
    fn quiet_silences_commentary_and_verbose_enables_it_by_level() {
        let quiet = GlobalArgs {
            quiet: true,
            ..GlobalArgs::default()
        };
        let loud = GlobalArgs {
            verbose: 2,
            ..GlobalArgs::default()
        };

        assert!(!quiet.is_verbose(1));
        assert!(loud.is_verbose(1));
        assert!(loud.is_verbose(2));
        assert!(!loud.is_verbose(3));
    }
}
