# Repository Guidelines

## Project Structure & Module Organization
This repository is a mixed Rust and TypeScript monorepo for Darwin Code. Core runtime code lives in `codex-rs/`, with crates such as `cli/`, `core/`, `tui/`, `app-server/`, and shared utilities under `utils/`. Node tooling lives in `codex-cli/`, repo scripts live in `scripts/` and `tools/`, SDKs live in `sdk/python`, `sdk/python-runtime`, and `sdk/typescript`, and long-form docs live in `docs/`. Put new Rust integration tests in each crate’s `tests/` directory.

## Build, Test, and Development Commands
Use `just` from the repo root for the common paths:

- `just d --help` runs the Darwin Code CLI from source through `codex-rs/`.
- `just fmt` applies Rust formatting with the repo’s import layout.
- `just clippy` runs Rust lint checks for test targets too.
- `just test` runs `cargo nextest run --no-fail-fast`.
- `just bazel-test` runs the Bazel test suite.
- `pnpm format` checks Markdown/JSON/JS formatting; `pnpm format:fix` rewrites it.

For focused work, use crate-scoped Cargo commands such as `cargo test -p darwin-code-cli` from `codex-rs/`.

## Coding Style & Naming Conventions
Rust follows edition `2024`, `rustfmt`, and `clippy`. Use 4-space indentation, `snake_case` for modules/functions, `PascalCase` for types, and keep crate names aligned with the existing `darwin-code-*` pattern. TypeScript and JSON formatting is enforced by Prettier. Prefer existing utilities and nearby naming before adding new abstractions or terms.

## Testing Guidelines
Prefer narrow tests first, then rerun the shared gates. Rust unit tests commonly live in `src/*tests.rs` or `mod tests`, while integration suites live under `tests/` with descriptive filenames such as `thread_resume.rs` or `marketplace_add.rs`. Add regression coverage for behavior changes and run `just test`, `just clippy`, and any affected crate-level `cargo test -p ...` command before opening a PR.

## Commit & Pull Request Guidelines
Recent history mixes short subjects with rename/refactor commits, but this repository expects concise intent-first messages. Follow the local Lore protocol: a short reason-focused subject, then trailers such as `Constraint:`, `Rejected:`, `Confidence:`, `Scope-risk:`, `Directive:`, `Tested:`, and `Not-tested:` when they add decision context. PRs should explain the user-visible impact, list verification commands, link issues when relevant, and include screenshots only for UI or TUI changes.

## Security & Configuration Tips
Treat `config.toml` and local state files as developer-local inputs; never commit secrets or copied API keys. When testing provider changes, prefer local overrides and document any required environment or config assumptions in the PR.
