//! The library's error type.
//!
//! There is no `anyhow` or `thiserror` here, because writing the two traits by
//! hand is about thirty lines and shows exactly what those crates generate:
//!
//! * [`Display`](std::fmt::Display) writes **one** layer of the story, without
//!   the word "error" and without a trailing period, so it reads correctly when
//!   a caller prints `error: {err}`.
//! * [`Error::source`](std::error::Error::source) links to the layer
//!   underneath, so the binary can print the whole chain
//!   (see `report` in `src/main.rs`).
//!
//! Reach for `thiserror` when the enum grows past a handful of variants, and
//! for `anyhow` in a binary that never needs callers to match on the error.

use std::fmt;
use std::io;
use std::path::PathBuf;

/// The result type used throughout this crate.
///
/// The defaulted type parameter means `Result<T>` reads like `std`'s and still
/// allows `Result<T, SomeOtherError>` where that is clearer.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Everything that can go wrong inside the library.
///
/// Each variant carries the context needed to write a message a user can act
/// on — above all, *which file* was being read.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// A configuration file could not be opened or read.
    ConfigRead {
        /// The file that could not be read.
        path: PathBuf,
        /// The underlying I/O failure.
        source: io::Error,
    },
    /// A configuration file was found and read, but is not valid.
    ConfigParse {
        /// The file that failed to parse.
        path: PathBuf,
        /// The underlying TOML failure, including line and column.
        source: toml::de::Error,
    },
    /// Writing to the output stream failed.
    Output(io::Error),
    /// The settings could not be encoded as TOML.
    ///
    /// Unreachable for the fixed shape of [`Settings`](crate::config::Settings),
    /// but a variant costs three lines and a panic costs a bug report.
    Encode(toml::ser::Error),
}

impl Error {
    /// Builds a [`Error::ConfigRead`] for `path`.
    pub fn config_read(path: impl Into<PathBuf>, source: io::Error) -> Self {
        Self::ConfigRead {
            path: path.into(),
            source,
        }
    }

    /// Builds a [`Error::ConfigParse`] for `path`.
    pub fn config_parse(path: impl Into<PathBuf>, source: toml::de::Error) -> Self {
        Self::ConfigParse {
            path: path.into(),
            source,
        }
    }

    /// Reports whether this error is a downstream reader closing the pipe.
    ///
    /// `rust-example hello --repeat 100 | head -1` makes `head` exit after the
    /// first line; every later write then fails with
    /// [`io::ErrorKind::BrokenPipe`]. That is normal shell behaviour, not a
    /// failure, so the binary exits quietly instead of printing an error.
    #[must_use]
    pub fn is_broken_pipe(&self) -> bool {
        matches!(self, Self::Output(err) if err.kind() == io::ErrorKind::BrokenPipe)
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ConfigRead { path, .. } => {
                write!(f, "cannot read config file `{}`", path.display())
            }
            Self::ConfigParse { path, .. } => {
                write!(f, "cannot parse config file `{}`", path.display())
            }
            Self::Output(_) => f.write_str("cannot write output"),
            Self::Encode(_) => f.write_str("cannot encode settings as TOML"),
        }
    }
}

impl std::error::Error for Error {
    // The arms look identical but bind different concrete types, each coerced
    // to `&dyn Error` separately, so they cannot be collapsed into one pattern.
    #[allow(clippy::match_same_arms, reason = "the bound types differ")]
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::ConfigRead { source, .. } => Some(source),
            Self::ConfigParse { source, .. } => Some(source),
            Self::Output(source) => Some(source),
            Self::Encode(source) => Some(source),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_names_the_offending_file() {
        let err = Error::config_read(
            "/etc/rust-example/config.toml",
            io::Error::from(io::ErrorKind::NotFound),
        );

        assert_eq!(
            err.to_string(),
            "cannot read config file `/etc/rust-example/config.toml`"
        );
    }

    #[test]
    fn source_exposes_the_underlying_cause() {
        let err = Error::config_read("cfg.toml", io::Error::from(io::ErrorKind::PermissionDenied));
        let source = std::error::Error::source(&err).map(ToString::to_string);

        assert!(source.is_some(), "the io::Error should be reachable");
    }

    #[test]
    fn broken_pipe_is_recognised() {
        let broken = Error::Output(io::Error::from(io::ErrorKind::BrokenPipe));
        let full = Error::Output(io::Error::from(io::ErrorKind::StorageFull));

        assert!(broken.is_broken_pipe());
        assert!(!full.is_broken_pipe());
    }
}
