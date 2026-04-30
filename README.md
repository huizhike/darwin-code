# Darwin Code Engine

Internal fork of OpenAI Codex CLI for Darwin's autonomous coding pipeline.

## Upstream Source

| Field | Value |
|-------|-------|
| **Upstream** | `openai/codex` (Apache-2.0) |
| **Stable npm version** | `@openai/codex@0.121.0` (`latest` dist-tag) |
| **Source commit** | `def6467d2bb7c9ff6333d59b6f64a0acf839817a` (2026-04-18) |
| **Alpha (not used)** | `0.122.0-alpha.9` |

## Directory Structure

- `codex-cli/` — Node.js CLI entry point
- `codex-rs/` — Rust sandbox runtime (codex-exec, codex-core, codex-tui)
- `sdk/` — SDK bindings
- `docs/` — Documentation

## Run from Source

> **⚠️ 警告: 编译产物体积与磁盘空间**
>
> Rust 项目默认会产生巨大的增量编译缓存。长期开发可能导致 `target/` 目录累积突破 100GB，给系统存储造成灾难。系统已默认配置了底层的 cargo 命令拦截器，每天会自动在后台静默清理 1 天前的冗余缓存。如果依然遭遇空间不足，可手动执行 `cargo sweep --time 1`。

```bash
cd 70-darwin-code
# 在本地未跟踪 config.toml 中填写 [openai_compatible.deepseek].api_key
cd codex-rs
cargo clean
cargo run -p darwin-code-cli --bin darwin-code
```

从源码目录运行且未设置 `DARWIN_CODE_HOME` / `CODEX_HOME` 时，Darwin Code 会
自动使用 checkout 根目录的 `config.toml`。如需覆盖配置目录，可显式设置：

```bash
export DARWIN_CODE_HOME=/path/to/darwin-code-home
cargo clean
cargo run -p darwin-code-cli --bin darwin-code
```

## BYOK quick start

Darwin Code 的产品默认路径是 **BYOK（Bring Your Own Key）**：用户在
`config.toml` 中明确选择 provider、model、`base_url`，并直接提供自己的
`api_key`。本地 TUI/CLI 不再要求为模型 key 配置环境变量，也不再从 provider id
推断变量名。除 BYOK direct key 外不支持 hosted OAuth/device sign-in、workspace/cloud
auth、command-backed bearer token、keychain、`.env` loader 或隐式 OpenAI 登录。

### DeepSeek（默认本地配置）

```toml
model_provider = "deepseek"
model = "deepseek-v4-flash"

[openai_compatible.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat-completions"
api_key = "你的 DeepSeek API Key"
```

### Local cli2api / generic OpenAI-compatible endpoint

任何实现 OpenAI-compatible API 的外部网关（例如公司网关、`cli2api`、本地代理）
都作为标准 endpoint 出现在 Darwin Code 里：

```toml
model_provider = "my-gateway"
model = "your-model"

[openai_compatible.my-gateway]
name = "My Gateway"
base_url = "http://127.0.0.1:8080/v1"
wire_api = "responses"
api_key = "你的网关 API Key"
```

`wire_api` 表示 provider 实际支持的协议路径：DeepSeek 官方 API 使用
`chat-completions`（请求 `/chat/completions`）；本地 `cli2api` 或其他兼容
OpenAI Responses API 的网关使用 `responses`（请求 `/responses`）。不要把
DeepSeek 的 `base_url` 写成带 `/v1` 后再走 Responses，否则会命中 DeepSeek 不支持的
`/v1/responses`。

### OpenAI / Gemini / Claude direct key

```toml
model_provider = "openai"
model = "gpt-5"

[openai]
base_url = "https://api.openai.com/v1"
api_key = "你的 OpenAI API Key"
```

```toml
model_provider = "gemini"
model = "gemini-2.5-pro"

[gemini]
base_url = "https://generativelanguage.googleapis.com/v1beta"
api_key = "你的 Gemini API Key"
```

```toml
model_provider = "claude"
model = "claude-sonnet-4-5"

[claude]
base_url = "https://api.anthropic.com"
api_key = "你的 Claude API Key"
```

当前可运行投影优先覆盖 `[openai_compatible.<id>]`；Gemini/Claude native runtime
adapter 尚未落地时，如果直接选择对应 provider，启动会给出明确错误。临时使用
Gemini/Claude 的 OpenAI-compatible 网关时，应改用 `[openai_compatible.<id>]` 并填写
网关的 `base_url` 与 `api_key`。
