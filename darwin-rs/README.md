# Darwin Code Rust Workspace

This directory is the Rust implementation of Darwin Code. Use source runs for
local development, or install the published npm wrapper from
`@darwin-code/darwin-code` for a user-level command.

## Install / Run

### npm install

The npm package is published under the `@darwin-code` scope. Install and
verify it with:

```shell
npm install -g @darwin-code/darwin-code
darwin-code --version
darwin-code --help
```

After install, edit the packaged template config or copy it into a user config
directory before running the TUI with real BYOK provider keys:

```shell
$EDITOR "$(npm root -g)/@darwin-code/darwin-code/config.toml"
# Or:
mkdir -p ~/.darwin
cp "$(npm root -g)/@darwin-code/darwin-code/config.toml" ~/.darwin/config.toml
$EDITOR ~/.darwin/config.toml
```

### Run from source

Use source runs for local development. This avoids installing a large debug
binary into your user PATH:

```shell
cd /your/own/path/darwin-code
cp config-example.toml config.toml
$EDITOR config.toml
cd darwin-rs
cargo run -p darwin-code-cli --bin darwin-code
```

`config.toml` is local runtime configuration and must not be committed with real
keys. Keep provider, model, endpoint, and API key settings explicit.

If you intentionally need a user-level command, install a release build and make
sure `$HOME/.local/bin` is in `PATH`:

```shell
cd /your/own/path/darwin-code/darwin-rs
cargo install --path cli --locked --root "$HOME/.local"
command -v darwin-code
darwin-code --help
```

Do not use `cargo install --debug` for a normal user install; it is fast for
smoke tests but can create a very large binary. Remove an unwanted user install
with `rm -f ~/.local/bin/darwin-code`.

For a local npm-wrapper smoke test without the public registry, stage a built
binary into the wrapper package and install from that staged folder:

```shell
cd /your/own/path/darwin-code
cargo build --manifest-path darwin-rs/Cargo.toml -p darwin-code-cli --bin darwin-code
rm -rf /your/own/temp/darwin-cli-stage
cp -a darwin-cli /your/own/temp/darwin-cli-stage
mkdir -p /your/own/temp/darwin-cli-stage/vendor/x86_64-unknown-linux-musl/darwin-code
cp darwin-rs/target/debug/darwin-code \
  /your/own/temp/darwin-cli-stage/vendor/x86_64-unknown-linux-musl/darwin-code/darwin-code
npm install -g /your/own/temp/darwin-cli-stage
darwin-code --help
```

## Common commands

```shell
# Interactive TUI
cargo run -p darwin-code-cli --bin darwin-code

# Non-interactive execution
cargo run -p darwin-code-cli --bin darwin-code -- exec "say hello"

# MCP server over stdio
cargo run -p darwin-code-cli --bin darwin-code -- mcp-server

# Sandbox helpers
cargo run -p darwin-code-cli --bin darwin-code -- sandbox linux --help
cargo run -p darwin-code-cli --bin darwin-code -- sandbox macos --help
cargo run -p darwin-code-cli --bin darwin-code -- sandbox windows --help
```

The repository `justfile` also provides:

```shell
just d
just test
just fmt
just clippy
```

## Development validation

```shell
cd /your/own/path/darwin-code/darwin-rs
cargo clean
cargo test --workspace
cargo fmt --all
cargo clippy --workspace --all-targets
```

## Workspace layout

- `cli/` contains the top-level `darwin-code` command and subcommand routing.
- `core/` contains provider configuration, session orchestration, tool execution,
  and runtime policy.
- `tui/` contains the interactive terminal UI.
- `exec/` contains non-interactive execution support.
- `mcp-server/` contains the stdio MCP server.
- `app-server/` and `app-server-protocol/` contain the local app-server surfaces.
- `utils/` contains shared helper crates.

See the top-level [`README.md`](../README.md), [`docs/getting-started.md`](../docs/getting-started.md),
and [`docs/config.md`](../docs/config.md) for user-facing setup details.
