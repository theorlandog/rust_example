//! The work the tool actually does.
//!
//! Two functions, deliberately:
//!
//! * [`greeting`] builds a value and is trivially testable;
//! * [`write_greeting`] does the I/O, and is generic over [`Write`] so tests
//!   can hand it a `Vec<u8>` instead of a real terminal.
//!
//! Keeping `println!` out of library code is what makes the tests in this file
//! possible. It also means the caller decides on buffering and on what happens
//! when the write fails — see `src/main.rs`.

use std::io::{self, Write};

use crate::config::Settings;

/// Builds the greeting described by `settings`.
///
/// ```
/// use rust_example::config::Settings;
/// use rust_example::greeting::greeting;
///
/// let settings = Settings { name: "Ferris".into(), repeat: 1, shout: true };
/// assert_eq!(greeting(&settings), "HELLO, FERRIS!");
/// ```
#[must_use]
pub fn greeting(settings: &Settings) -> String {
    let text = format!("Hello, {}!", settings.name);

    if settings.shout {
        text.to_uppercase()
    } else {
        text
    }
}

/// Writes the greeting to `out`, once per [`Settings::repeat`].
///
/// # Errors
///
/// Passes through any [`io::Error`] from `out`, including the
/// [`BrokenPipe`](io::ErrorKind::BrokenPipe) that a downstream `head` causes.
pub fn write_greeting<W: Write>(out: &mut W, settings: &Settings) -> io::Result<()> {
    let line = greeting(settings);

    for _ in 0..settings.repeat {
        writeln!(out, "{line}")?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered(settings: &Settings) -> String {
        let mut out = Vec::new();
        write_greeting(&mut out, settings).expect("writing to a Vec cannot fail");
        String::from_utf8(out).expect("output is UTF-8")
    }

    #[test]
    fn the_default_settings_produce_hello_world() {
        assert_eq!(rendered(&Settings::default()), "Hello, world!\n");
    }

    #[test]
    fn the_name_is_interpolated() {
        let settings = Settings {
            name: "Ferris".to_owned(),
            ..Settings::default()
        };

        assert_eq!(rendered(&settings), "Hello, Ferris!\n");
    }

    #[test]
    fn shouting_uppercases_the_whole_line() {
        let settings = Settings {
            shout: true,
            ..Settings::default()
        };

        assert_eq!(rendered(&settings), "HELLO, WORLD!\n");
    }

    #[test]
    fn repeat_controls_the_line_count() {
        let settings = Settings {
            repeat: 3,
            ..Settings::default()
        };

        assert_eq!(rendered(&settings).lines().count(), 3);
    }

    #[test]
    fn a_failing_writer_surfaces_its_error() {
        /// A writer that refuses everything, standing in for a full disk.
        struct Broken;

        impl Write for Broken {
            fn write(&mut self, _: &[u8]) -> io::Result<usize> {
                Err(io::Error::from(io::ErrorKind::BrokenPipe))
            }
            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        let err = write_greeting(&mut Broken, &Settings::default()).unwrap_err();

        assert_eq!(err.kind(), io::ErrorKind::BrokenPipe);
    }
}
