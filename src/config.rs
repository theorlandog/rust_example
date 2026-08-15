//! Configuration: the file format, where the file lives, and how it layers
//! with environment variables and command line flags.
//!
//! # The precedence rule
//!
//! Values are resolved lowest-to-highest:
//!
//! ```text
//! built-in defaults  <  config file  <  environment variables  <  command line flags
//! ```
//!
//! # How that is implemented
//!
//! The trick is to keep two separate types:
//!
//! * [`Config`] is a *partial* configuration. Every field is an [`Option`], so
//!   "not mentioned" is distinguishable from "explicitly set". Both the config
//!   file and the command line parse into one of these.
//! * [`Settings`] is *resolved*. Every field has a value, so the rest of the
//!   program never has to think about precedence again.
//!
//! Layering is then a single call to [`Config::merge`] per level, and
//! environment variables need no special handling at all: `clap` reads them
//! into the command line layer (see [`crate::cli::HelloArgs`]), which is
//! exactly where they belong in the ordering above.

use std::env;
use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// The directory this application owns inside the platform's config directory.
const APP_DIR: &str = "rust-example";

/// The file name looked for inside [`APP_DIR`].
const FILE_NAME: &str = "config.toml";

/// A partially specified configuration.
///
/// `None` means "this layer said nothing about that setting", which is what
/// makes [`merge`](Config::merge) able to express precedence. `serde` maps a
/// missing TOML key to `None` for free.
///
/// `deny_unknown_fields` turns a typo like `nmae = "Ferris"` into an error
/// naming the line, instead of a setting that silently does nothing.
#[derive(Debug, Default, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields, rename_all = "kebab-case")]
pub struct Config {
    /// Who to greet.
    pub name: Option<String>,
    /// How many times to print the greeting.
    pub repeat: Option<u8>,
    /// Whether to shout the greeting in uppercase.
    pub shout: Option<bool>,
}

impl Config {
    /// Layers `self` on top of `base`: any value set in `self` wins, and any
    /// value it leaves unset falls through to `base`.
    ///
    /// ```
    /// use rust_example::config::Config;
    ///
    /// let file = Config { name: Some("Ferris".into()), repeat: Some(3), shout: None };
    /// let flags = Config { name: Some("world".into()), repeat: None, shout: Some(true) };
    ///
    /// let merged = flags.merge(file);
    /// assert_eq!(merged.name.as_deref(), Some("world")); // flag wins
    /// assert_eq!(merged.repeat, Some(3));                // only the file set it
    /// assert_eq!(merged.shout, Some(true));              // only the flag set it
    /// ```
    #[must_use]
    pub fn merge(self, base: Self) -> Self {
        Self {
            name: self.name.or(base.name),
            repeat: self.repeat.or(base.repeat),
            shout: self.shout.or(base.shout),
        }
    }

    /// Parses a configuration from TOML text.
    ///
    /// `path` is used only to build a good error message.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigParse`] if `text` is not valid TOML or contains a
    /// key this program does not recognise.
    pub fn parse(text: &str, path: &Path) -> Result<Self> {
        toml::from_str(text).map_err(|source| Error::config_parse(path, source))
    }

    /// Reads and parses the configuration file at `path`.
    ///
    /// # Errors
    ///
    /// Returns [`Error::ConfigRead`] if the file cannot be read (including when
    /// it does not exist), or [`Error::ConfigParse`] if its contents are not
    /// valid.
    pub fn read_from(path: &Path) -> Result<Self> {
        let text = fs::read_to_string(path).map_err(|source| Error::config_read(path, source))?;
        Self::parse(&text, path)
    }
}

/// Where the configuration that is in effect came from.
///
/// Worth reporting: "why is it greeting Ferris?" is answered by
/// `rust-example config`, not by guessing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origin {
    /// Read from this file.
    File(PathBuf),
    /// No file was read; only built-in defaults and the command line apply.
    Defaults,
}

/// A configuration together with the place it came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The parsed configuration, empty if no file was found.
    pub config: Config,
    /// Where [`Self::config`] came from.
    pub origin: Origin,
}

/// Loads the configuration file.
///
/// When `explicit` is `Some`, that exact file is required: a missing file is an
/// error, because the user asked for it by name and a silent fallback to
/// defaults would be a lie. When it is `None`, [`default_path`] is consulted
/// and a missing file simply means "use defaults".
///
/// # Errors
///
/// Returns [`Error::ConfigRead`] or [`Error::ConfigParse`] if a file that
/// should be readable is not.
pub fn load(explicit: Option<&Path>) -> Result<Loaded> {
    if let Some(path) = explicit {
        return Ok(Loaded {
            config: Config::read_from(path)?,
            origin: Origin::File(path.to_path_buf()),
        });
    }

    match default_path() {
        Some(path) if path.is_file() => Ok(Loaded {
            config: Config::read_from(&path)?,
            origin: Origin::File(path),
        }),
        _ => Ok(Loaded {
            config: Config::default(),
            origin: Origin::Defaults,
        }),
    }
}

/// Returns the config file path this program looks in when `--config` is not
/// given, or `None` if the platform's config directory cannot be determined.
///
/// | Platform | Path |
/// | --- | --- |
/// | Linux and other Unix | `$XDG_CONFIG_HOME/rust-example/config.toml`, else `$HOME/.config/rust-example/config.toml` |
/// | macOS | `$HOME/Library/Application Support/rust-example/config.toml` |
/// | Windows | `%APPDATA%\rust-example\config.toml` |
#[must_use]
pub fn default_path() -> Option<PathBuf> {
    config_dir().map(|dir| dir.join(APP_DIR).join(FILE_NAME))
}

/// Reads an environment variable, treating an empty value as unset.
///
/// An exported-but-empty `XDG_CONFIG_HOME` is common enough that ignoring the
/// difference produces paths rooted at `/`.
fn non_empty_env(key: &str) -> Option<OsString> {
    env::var_os(key).filter(|value| !value.is_empty())
}

/// The user's home directory, as `$HOME`.
///
/// Both specifications implemented below are written in terms of the
/// environment variable rather than the account database: the XDG base
/// directory specification says `$HOME`, and Apple's layout is relative to
/// `NSHomeDirectory()`, which for a command line process is `$HOME`. Honouring
/// the variable is also what lets tests and `sudo -H` redirect the lookup.
///
/// [`std::env::home_dir`] is the alternative, and additionally falls back to
/// the passwd database when `$HOME` is unset. It spent years deprecated over
/// its Windows behaviour, so check whether your MSRV still warns on it.
#[cfg(unix)]
fn home_dir() -> Option<PathBuf> {
    non_empty_env("HOME").map(PathBuf::from)
}

/// `%APPDATA%` — the roaming per-user application data directory.
#[cfg(windows)]
fn config_dir() -> Option<PathBuf> {
    non_empty_env("APPDATA").map(PathBuf::from)
}

/// `~/Library/Application Support`, per Apple's file system layout.
#[cfg(target_os = "macos")]
fn config_dir() -> Option<PathBuf> {
    home_dir().map(|home| home.join("Library").join("Application Support"))
}

/// `$XDG_CONFIG_HOME`, falling back to `~/.config` as the XDG base directory
/// specification requires.
#[cfg(all(unix, not(target_os = "macos")))]
fn config_dir() -> Option<PathBuf> {
    non_empty_env("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
        .or_else(|| home_dir().map(|home| home.join(".config")))
}

/// A fully resolved configuration: every setting has a value.
///
/// Code downstream of [`Settings::from`] never has to ask "was this set?" — a
/// small type-level guarantee that removes a whole class of `unwrap_or` calls.
///
/// It derives [`Serialize`] so `rust-example config` can print it back out as a
/// valid config file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub struct Settings {
    /// Who to greet.
    pub name: String,
    /// How many times to print the greeting.
    pub repeat: u8,
    /// Whether to shout the greeting in uppercase.
    pub shout: bool,
}

impl Settings {
    /// The greeting target when nothing else says otherwise.
    pub const DEFAULT_NAME: &'static str = "world";
    /// The number of greetings when nothing else says otherwise.
    pub const DEFAULT_REPEAT: u8 = 1;
    /// Whether to shout when nothing else says otherwise.
    pub const DEFAULT_SHOUT: bool = false;
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            name: Self::DEFAULT_NAME.to_owned(),
            repeat: Self::DEFAULT_REPEAT,
            shout: Self::DEFAULT_SHOUT,
        }
    }
}

/// Fills in the built-in defaults — the bottom layer of the precedence stack.
impl From<Config> for Settings {
    fn from(config: Config) -> Self {
        let defaults = Self::default();
        Self {
            name: config.name.unwrap_or(defaults.name),
            repeat: config.repeat.unwrap_or(defaults.repeat),
            shout: config.shout.unwrap_or(defaults.shout),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(text: &str) -> Result<Config> {
        Config::parse(text, Path::new("test.toml"))
    }

    #[test]
    fn an_empty_file_sets_nothing() {
        assert_eq!(parse("").unwrap(), Config::default());
    }

    #[test]
    fn keys_are_optional_and_independent() {
        let config = parse("repeat = 2").unwrap();

        assert_eq!(config.repeat, Some(2));
        assert_eq!(config.name, None);
        assert_eq!(config.shout, None);
    }

    #[test]
    fn a_misspelled_key_is_an_error_naming_the_file() {
        let err = parse(r#"nmae = "Ferris""#).unwrap_err();

        assert!(matches!(err, Error::ConfigParse { .. }));
        assert!(err.to_string().contains("test.toml"));
    }

    #[test]
    fn a_wrongly_typed_value_is_an_error() {
        assert!(parse(r#"repeat = "lots""#).is_err());
    }

    #[test]
    fn defaults_apply_when_nothing_is_configured() {
        let settings = Settings::from(Config::default());

        assert_eq!(settings, Settings::default());
        assert_eq!(settings.name, "world");
        assert_eq!(settings.repeat, 1);
        assert!(!settings.shout);
    }

    #[test]
    fn the_full_precedence_chain_resolves_as_documented() {
        // Layer 2 (lowest of the two): the config file sets a name and a count.
        let file = parse("name = \"file\"\nrepeat = 9").unwrap();
        // Layer 3: the command line overrides the name and turns on shouting.
        let flags = Config {
            name: Some("flag".to_owned()),
            repeat: None,
            shout: Some(true),
        };

        let settings = Settings::from(flags.merge(file));

        assert_eq!(settings.name, "flag", "flags beat the config file");
        assert_eq!(settings.repeat, 9, "the file beats the built-in default");
        assert!(settings.shout, "a flag-only value survives merging");
    }

    #[test]
    fn merging_an_empty_layer_changes_nothing() {
        let base = parse("name = \"Ferris\"\nshout = true").unwrap();

        assert_eq!(Config::default().merge(base.clone()), base);
    }

    #[test]
    fn a_missing_explicit_config_file_is_an_error() {
        let missing = Path::new("definitely/not/here/config.toml");
        let err = load(Some(missing)).unwrap_err();

        assert!(matches!(err, Error::ConfigRead { .. }));
    }

    #[test]
    fn settings_round_trip_through_toml() {
        let settings = Settings {
            name: "Ferris".to_owned(),
            repeat: 2,
            shout: true,
        };

        // `rust-example config` prints this; feeding it back in must work.
        let text = toml::to_string(&settings).unwrap();
        let reparsed = Settings::from(parse(&text).unwrap());

        assert_eq!(reparsed, settings);
    }

    #[test]
    fn the_default_path_is_under_the_platform_config_directory() {
        let Some(path) = default_path() else {
            return; // No HOME in this environment; nothing to assert.
        };

        assert!(path.is_absolute());
        assert!(path.ends_with(Path::new(APP_DIR).join(FILE_NAME)));
    }
}
