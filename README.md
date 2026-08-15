# rust-example

A reference example of a modern Rust command line application.

The program is deliberately trivial — it prints `Hello, world!` — so that
nothing distracts from the parts that are actually the same in every CLI you
will write: parsing arguments, layering configuration, reporting errors,
testing the result, and shipping it as an installer people can double-click.

If you have written a CLI in Go with [urfave/cli], this is the Rust equivalent:
[`clap`] plays the same role, and the config layering that urfave/cli builds in
through `altsrc` is spelled out here in about forty lines you can read.

[urfave/cli]: https://github.com/urfave/cli
[`clap`]: https://docs.rs/clap

```console
$ rust-example
Hello, world!

$ rust-example --name Ferris --repeat 2 --shout
HELLO, FERRIS!
HELLO, FERRIS!

$ RUST_EXAMPLE_NAME=Ferris rust-example
Hello, Ferris!
```

## Quick start

```sh
git clone https://github.com/theorlandog/rust_example
cd rust_example

cargo run                          # Hello, world!
cargo run -- --name Ferris         # Hello, Ferris!
cargo run -- --help                # the generated help
cargo test                         # 48 tests
```

Or install one of the [release installers](#installers), or
`cargo install --path .`.

## The command line

```console
$ rust-example --help
A reference example of a modern Rust CLI: argument parsing, layered configuration, and native
installers.

Usage: rust-example [OPTIONS] [COMMAND]

Commands:
  config       Print the settings in effect, and where they came from
  completions  Print a shell completion script
  help         Print this message or the help of the given subcommand(s)

Options:
  -n, --name <NAME>    Who to greet [default: world] [env: RUST_EXAMPLE_NAME=]
  -r, --repeat <N>     How many times to print the greeting [default: 1] [env: RUST_EXAMPLE_REPEAT=]
      --shout          Shout the greeting in uppercase
      --no-shout       Do not shout the greeting, overriding `shout` in the config file
  -c, --config <FILE>  Read this config file instead of the default one [env: RUST_EXAMPLE_CONFIG=]
  -v, --verbose...     Print more detail on standard error; repeat for more (`-vv`)
  -q, --quiet          Suppress diagnostics on standard error
  -h, --help           Print help (see more with '--help')
  -V, --version        Print version
```

Every line of that is generated from the struct in [`src/cli.rs`](src/cli.rs) —
the doc comments *are* the help text.

There is no `hello` subcommand: greeting is what the tool does, and `config`
and `completions` sit alongside it. Greeting options still apply to `config`,
so `rust-example --name Ferris config` shows what those settings *would* be.

## Configuration

Settings come from four places. Later ones win:

```
built-in defaults  <  config file  <  environment variables  <  command line flags
```

The config file is TOML, and lives where the platform says it should:

| Platform | Path |
| --- | --- |
| Linux | `$XDG_CONFIG_HOME/rust-example/config.toml`, else `~/.config/rust-example/config.toml` |
| macOS | `~/Library/Application Support/rust-example/config.toml` |
| Windows | `%APPDATA%\rust-example\config.toml` |

```toml
# ~/.config/rust-example/config.toml
name = "Ferris"
repeat = 2
shout = false
```

A missing config file is fine; a file named with `--config` that is missing is
an error, because the user asked for it by name. An unknown key is an error
too, so `nmae = "Ferris"` is reported rather than ignored. `rust-example config`
prints the settings in effect and where they came from, in a form you can save
straight back to the config file. See
[`packaging/config.example.toml`](packaging/config.example.toml).

### How the layering works

The whole mechanism is two types and one method:

- [`Config`](src/config.rs) is a *partial* configuration — every field is an
  `Option`, so "not mentioned" stays distinguishable from "set to the default
  value". Both the config file and the command line parse into one.
- [`Settings`](src/config.rs) is *resolved* — every field has a value, so the
  rest of the program never thinks about precedence again.
- `Config::merge` layers one over another.

Which makes the entire precedence rule a single line in `main.rs`:

```rust
let settings = Settings::from(Config::from(args).merge(loaded.config));
```

Environment variables need no special handling at all: `clap` reads them into
the command line layer, which is exactly where they belong in the ordering.

This is also why no argument declares a `clap` `default_value`. A default
applied by the parser is indistinguishable from a value the user typed, so it
would silently beat the config file — the single most common way a layered
config system ends up not working.

## Project layout

| Path | What lives there |
| --- | --- |
| [`src/main.rs`](src/main.rs) | The binary: parse, dispatch, report errors, choose an exit code. |
| [`src/lib.rs`](src/lib.rs) | The library root. Everything testable lives behind it. |
| [`src/cli.rs`](src/cli.rs) | The command line grammar, as `clap` derive structs. |
| [`src/config.rs`](src/config.rs) | Config file format, discovery, and the precedence rules. |
| [`src/greeting.rs`](src/greeting.rs) | The work the tool actually does. |
| [`src/error.rs`](src/error.rs) | One error type, with `Display` and `source` written out by hand. |
| [`tests/cli.rs`](tests/cli.rs) | End-to-end tests that run the real binary. |
| [`packaging/`](packaging/) | The four installers. See [packaging/README.md](packaging/README.md). |
| [`.github/workflows/`](.github/workflows/) | CI on every push; installers on every tag. |

Splitting a binary into `main.rs` plus a library is the single highest-value
structural decision here. `main.rs` stays about a hundred lines of wiring, and
everything else can be unit tested without spawning a process.

## Dependencies

Four, and the reasoning for each:

| Crate | Why it is here |
| --- | --- |
| [`clap`] | Argument parsing, help text, `--version`, env-var fallbacks, and value validation, all generated from one struct. Writing this by hand is a month of work nobody thanks you for. |
| [`clap_complete`] | Completion scripts for bash, zsh, fish, PowerShell, and elvish, generated from the same struct. |
| [`serde`] | Deriving the config file's `Deserialize`. |
| [`toml`] | The config file format. TOML is what Rust developers already read and write. |

[`clap_complete`]: https://docs.rs/clap_complete
[`serde`]: https://docs.rs/serde
[`toml`]: https://docs.rs/toml

Four crates and their transitive dependencies are 40 packages. What is *not*
here is as much of the point:

| Not used | Instead | Why |
| --- | --- | --- |
| `anyhow`, `thiserror` | [`src/error.rs`](src/error.rs) | One enum with hand-written `Display` and `source` impls is thirty lines, and shows exactly what those crates generate. Reach for `thiserror` past a handful of variants, and `anyhow` in a binary whose callers never match on errors. |
| `directories`, `dirs` | `config_dir()` in [`src/config.rs`](src/config.rs) | Three `#[cfg]`-gated functions reading `$XDG_CONFIG_HOME`, `$HOME`, and `%APPDATA%`. The specifications are written in terms of those variables anyway. |
| `assert_cmd`, `tempfile` | `CARGO_BIN_EXE_*`, `CARGO_TARGET_TMPDIR` | Cargo sets both for integration tests. See the header of [`tests/cli.rs`](tests/cli.rs). |
| `env_logger`, `tracing` | `-v` / `-q` and `eprintln!` | A tool with three diagnostics does not need log levels, filtering, and structured spans. Add `tracing` when there is something worth tracing. |

None of these are bad crates — `anyhow` in particular earns its place in most
real binaries. The point is that a dependency should be a decision, and at this
size the standard library is genuinely enough.

## Behaviour worth copying

Small things that separate a program that works from one that behaves:

- **Results on stdout, commentary on stderr.** `-v` never pollutes the output,
  so `rust-example -v --name Ferris | wc -l` still says `1`.
- **A closed pipe is not an error.** `rust-example --repeat 100 | head -1`
  exits 0 instead of printing `failed printing to stdout`.
- **Exit codes mean something.** 0 success, 1 the command failed, 2 the command
  line was wrong — the convention `clap` already follows.
- **Errors print their causes.** `cannot read config file 'x.toml'` followed by
  `caused by: No such file or directory (os error 2)`. The path is attached
  where the failure happens, not guessed at by the reader.
- **Output is buffered and flushed explicitly.** `BufWriter`'s flush-on-drop
  cannot report a failure, so a full disk would otherwise exit 0 having lost
  the last of the output.
- **`main` returns `ExitCode`.** `std::process::exit` skips destructors.
- **Tri-state flags.** `--shout` and `--no-shout` let the command line override
  the config file in *both* directions. A plain `bool` flag can only ever turn
  something on.
- **Values are validated by the parser.** `--repeat 0` is rejected by `clap`
  before `main` runs, so no code downstream has to consider it.

## Testing

```sh
cargo test              # unit, integration, and documentation tests
cargo clippy --all-targets
cargo fmt --all --check
```

48 tests in three flavours, each pulling its weight:

- **Unit tests** next to the code they cover, for the precedence rules, the
  parser grammar (including `Cli::command().debug_assert()`, which proves the
  `clap` definition is internally consistent), and error formatting.
- **Integration tests** in `tests/cli.rs` that run the real binary and check
  exit codes, stdout versus stderr, and environment variables — the wiring
  mistakes in `main.rs` that every unit test happily passes.
- **Documentation tests**, so the examples in the docs cannot rot.

## Installers

Every tagged release publishes four installers plus a `SHA256SUMS` file:

| Platform | Artefact | Installs to |
| --- | --- | --- |
| Debian, Ubuntu | `.deb` | `/usr/bin`, with shell completions and docs |
| Fedora, RHEL, openSUSE | `.rpm` | `/usr/bin`, with shell completions and docs |
| macOS (Intel and Apple silicon) | `.pkg` | `/usr/local/bin`, universal binary |
| Windows | `.msi` | `Program Files`, added to `PATH`, with an uninstaller |

```sh
sudo dpkg --install rust-example_0.2.0-1_amd64.deb    # or: sudo apt install ./…
sudo rpm --install rust-example-0.2.0-1.x86_64.rpm    # or: sudo dnf install ./…
sudo installer -pkg rust-example-0.2.0-universal.pkg -target /
msiexec /i rust-example-0.2.0-x64.msi                 # /quiet for unattended
```

Build them yourself with the scripts in [`packaging/`](packaging/) — the same
ones CI runs. [packaging/README.md](packaging/README.md) covers signing,
notarisation, cross-compiling, and why the Linux build deliberately uses an old
runner image.

## Releasing

```sh
# 1. Bump the version in Cargo.toml (and refresh Cargo.lock)
cargo update --workspace

# 2. Commit, tag, push
git commit -am "Release 0.3.0"
git tag v0.3.0
git push origin main --tags
```

The [release workflow](.github/workflows/release.yml) then builds all four
installers on native runners, checks that the tag matches the version in
`Cargo.toml` (a mismatch fails the release rather than publishing a lie),
generates checksums, and creates the GitHub release with notes from the commit
log. Running it manually instead builds the installers without publishing —
which is how to test a packaging change without spending a version number.

## Continuous integration

[`ci.yml`](.github/workflows/ci.yml) runs on every push and pull request:

- formatting, `clippy` (with `-D warnings`), and `cargo doc`;
- the test suite on Linux, macOS, and Windows;
- a build against Rust 1.85, the declared minimum, so `rust-version` in
  `Cargo.toml` is a fact rather than a claim;
- the Debian and RPM packages, which are then *installed and run* — packaging
  breaks in ways compiling does not, and the pull request is the right place to
  find out.

## Intentionally left out

Not because they are wrong, but because each one would obscure the parts above.
In rough order of how soon a real tool tends to need them:

- **A man page.** `clap_mangen`, four lines, same shape as completions.
- **Colour output.** `anstream` (already in the tree via `clap`), plus honouring
  `NO_COLOR` and a `--color auto|always|never` flag.
- **Structured logging.** `tracing` and `tracing-subscriber`, once `-v` covers
  more than three messages.
- **Async.** Nothing here waits on anything.
- **Config file discovery beyond one location.** No `/etc` fallback, no
  walking up from the working directory.
- **`cargo-dist` or `cargo-binstall`.** Both are good; the explicit scripts in
  `packaging/` show what those tools do for you.

## Licence

BSD 2-Clause. See [LICENSE](LICENSE).
