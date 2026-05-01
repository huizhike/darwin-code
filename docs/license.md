## License and notices

Darwin Code is a downstream derivative of OpenAI Codex. The primary project
code is distributed under the [Apache License, Version 2.0](../LICENSE), with
both upstream OpenAI Codex attribution and Darwin Code downstream attribution
preserved in the root [NOTICE](../NOTICE).

## Upstream source

| Field                               | Value                                                                                                   |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------- |
| **Upstream project**                | `openai/codex` (Apache-2.0)                                                                             |
| **Downstream package**              | `@darwin/darwin-code`                                                                                   |
| **Local source import**             | `356a520fc4d1aae87dc2010a45a061ba4e03d22a` — `Import Codex 0.121.0 (stable) source as darwin-code base` |
| **Recorded upstream source commit** | `def6467d2bb7c9ff6333d59b6f64a0acf839817a` (2026-04-17)                                                 |

Darwin Code is not a clean-room rewrite. It is a modified downstream
distribution of OpenAI Codex. The Darwin product target is BYOK-only and
intentionally removes/replaces OpenAI OAuth, bundled cloud product, and
telemetry surfaces that are outside that target.

## Third-party and vendored licenses

Unless a file or vendored directory states otherwise, Darwin Code source files
are Apache-2.0 licensed. Third-party and vendored components keep their own
licenses and notices, including:

- `darwin-rs/vendor/bubblewrap/` — LGPL-2.0-or-later; see
  `darwin-rs/vendor/bubblewrap/LICENSE` and `COPYING`.
- `third_party/meriyah/` — ISC; see `third_party/meriyah/LICENSE`.
- `third_party/wezterm/` — MIT; see `third_party/wezterm/LICENSE`.
- Ratatui-derived code in `darwin-rs/tui/src/custom_terminal.rs` — MIT notice
  is retained in the source file.
- path-absolutize-derived code in
  `darwin-rs/utils/absolute-path/src/absolutize.rs` — MIT notice is retained in
  the source file.
- `darwin-cli/bin/rg` — DotSlash manifest for ripgrep 15.1.0; downloaded
  ripgrep artifacts are licensed by the ripgrep project under MIT OR Unlicense.

When distributing source or npm packages, include the root `LICENSE` and
`NOTICE` files and keep the license files inside vendored/third-party
directories intact.
