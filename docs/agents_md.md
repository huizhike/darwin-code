# AGENTS.md

`AGENTS.md` files provide local repository instructions for coding agents.
Darwin Code reads the applicable files and injects their content as scoped
guidance for the current task.

## Hierarchical agents message

When the `child_agents_md` feature flag is enabled (via `[features]` in `config.toml`), Darwin Code appends additional guidance about AGENTS.md scope and precedence to the user instructions message and emits that message even when no AGENTS.md is present.
