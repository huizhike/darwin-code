# Darwin Code Rust Workspace

This directory is the Rust implementation of Darwin Code. The currently tested
install path is source-based. The public npm registry package is not available
yet, so do not document `npm i -g @darwin-code/darwin-code` as a working user install
until the package has been published and verified.

## Run from source

Use source runs for local development. This avoids installing a large debug
binary into your user PATH:

```shell
cd /home/xklab/01-darwin/dcode/70-darwin-code
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
cd /home/xklab/01-darwin/dcode/70-darwin-code/darwin-rs
cargo install --path cli --locked --root "$HOME/.local"
command -v darwin-code
darwin-code --help
```

Do not use `cargo install --debug` for a normal user install; it is fast for
smoke tests but can create a very large binary. Remove an unwanted user install
with `rm -f ~/.local/bin/darwin-code`.

## npm package status

`npm i -g @darwin-code/darwin-code` currently fails because the package is not present
in the public npm registry. To make that command work, publish the package under
the `@darwin-code` scope first:

```shell
cd /home/xklab/01-darwin/dcode/70-darwin-code/darwin-cli
npm login
npm publish --access public
npm view @darwin-code/darwin-code version
```

Publishing under `@darwin-code` requires an npm account with permission to that scope
(or an npm organization/user scope that owns it). The package must include a
working platform binary payload before publishing; otherwise the JavaScript
wrapper installs but cannot launch `darwin-code`.

For a local npm-wrapper smoke test before publishing, stage a built binary into
the wrapper package and install from that staged folder:

```shell
cd /home/xklab/01-darwin/dcode/70-darwin-code
cargo build --manifest-path darwin-rs/Cargo.toml -p darwin-code-cli --bin darwin-code
rm -rf /tmp/darwin-cli-stage
cp -a darwin-cli /tmp/darwin-cli-stage
mkdir -p /tmp/darwin-cli-stage/vendor/x86_64-unknown-linux-musl/darwin-code
cp darwin-rs/target/debug/darwin-code \
  /tmp/darwin-cli-stage/vendor/x86_64-unknown-linux-musl/darwin-code/darwin-code
npm install -g /tmp/darwin-cli-stage
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
cd /home/xklab/01-darwin/dcode/70-darwin-code/darwin-rs
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
