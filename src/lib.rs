//! `rust_example` — the library half of a reference command line application.
//!
//! The crate is split the way most real CLIs are split:
//!
//! * this library holds everything that can be tested without spawning a
//!   process — the argument definitions, configuration loading, and the actual
//!   work the tool performs;
//! * [`src/main.rs`](https://github.com/theorlandog/rust_example/blob/main/src/main.rs)
//!   is a thin binary that parses arguments, calls into here, and turns a
//!   [`Result`] into a process exit code.
//!
//! Keeping the split means `cargo test` exercises real code paths instead of
//! string-matching on program output, and it lets another crate reuse the
//! pieces if this ever grows into something bigger.
//!
//! # Layout
//!
//! | Module | Responsibility |
//! | --- | --- |
//! | [`cli`] | The command line grammar, declared with `clap`'s derive API. |
//! | [`config`] | The config file format, where it is found, and how it layers with flags. |
//! | [`greeting`] | The work the tool actually does. |
//! | [`error`] | One error type for the whole library. |
//!
//! # Example
//!
//! ```
//! use rust_example::config::{Config, Settings};
//! use rust_example::greeting;
//!
//! // A config file that only sets a name; everything else falls back to defaults.
//! let file: Config = toml::from_str(r#"name = "Ferris""#).unwrap();
//! let settings = Settings::from(file);
//!
//! assert_eq!(greeting::greeting(&settings), "Hello, Ferris!");
//! ```

pub mod cli;
pub mod config;
pub mod error;
pub mod greeting;

pub use error::{Error, Result};
