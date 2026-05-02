# Darwin Code Engine

BYOK-only execution engine for Darwin's autonomous coding pipeline.

## Directory Structure

- `darwin-cli/` — Node.js CLI entry point
- `darwin-rs/` — Rust sandbox runtime (`darwin-code-exec`, `darwin-code-core`, `darwin-code-tui`)
- `docs/` — Documentation

## Install / Run

### Preferred: run from source

Use source install when you need the latest GitHub code.

> **Warning: build artifact size and disk usage**
>
> Rust projects can produce large incremental build caches. Long-running local development may grow `target/` beyond 100 GB and put system storage at risk. This checkout is configured with a low-level Cargo command interceptor that silently sweeps redundant caches older than one day in the background. If disk space is still tight, run `cargo sweep --time 1` manually.

```bash
cd /your/own/path/darwin-code
cp .env.example .env
$EDITOR .env
$EDITOR config.toml  # optional: change provider/model/API endpoint
cd darwin-rs
cargo clean
cargo run -p darwin-code-cli --bin darwin-code
```

`config.toml` is the checked-in default source-run configuration. It must stay secret-free: commit provider ids, endpoints, model metadata, and `api_key_env` names only. Put real keys in `${DARWIN_CODE_HOME}/.env` or your parent shell environment. When running from the source checkout root or any child directory such as `darwin-rs/`, Darwin Code resolves the checkout root as `DARWIN_CODE_HOME` and loads the root `.env` automatically. You do not need to `source .env`; if you do source it manually from `darwin-rs/`, use `source ../.env`. Set `DARWIN_CODE_HOME` only if you need a different config directory.

Use another config directory:

```bash
mkdir -p /path/to/darwin-code-home
cp config.toml /path/to/darwin-code-home/config.toml
cp .env.example /path/to/darwin-code-home/.env
$EDITOR /path/to/darwin-code-home/.env
export DARWIN_CODE_HOME=/path/to/darwin-code-home
cargo clean
cargo run -p darwin-code-cli --bin darwin-code
```

### Alternative: npm install

The npm package is convenient, but it may lag behind the latest GitHub source.

```bash
npm install -g @darwin-code/darwin-code
darwin-code --version
darwin-code --help
```

Then edit the packaged default config and set provider/key environment names:

```bash
$EDITOR "$(npm root -g)/@darwin-code/darwin-code/config.toml"
$EDITOR "$(npm root -g)/@darwin-code/darwin-code/.env"
```

You can also keep configuration under your user directory by copying the packaged default config and editing it there:

```bash
mkdir -p ~/.darwin
cp "$(npm root -g)/@darwin-code/darwin-code/config.toml" ~/.darwin/config.toml
$EDITOR ~/.darwin/config.toml
$EDITOR ~/.darwin/.env
```

At minimum, verify these fields in `config.toml` and put the real key in `${DARWIN_CODE_HOME}/.env`:

```toml
model_provider = "qwen"        # Current provider id
model = "qwen3.6-plus"         # Current model; /model lists this provider's models

[openai_compatible.qwen]
name = "Qwen"
base_url = "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"
wire_api = "chat-completions"  # Use chat-completions or responses to match the endpoint
api_key_env = "QWEN_API_KEY"  # Real key lives in ${DARWIN_CODE_HOME}/.env
```

To add a model, add it under the matching provider:

```toml
[openai_compatible.qwen.models."your-model"]
display_name = "Your Model"
context_window = 128000
max_output_tokens = 8192
thinking = true
multimodal = true
tools = true
```

## BYOK quick start

Darwin Code's default product path is **BYOK (Bring Your Own Key)**: select the provider, model, `base_url`, and `api_key_env` explicitly in `config.toml`, then put the real key in `${DARWIN_CODE_HOME}/.env` or an external environment variable. Each provider declares selectable models under its own `models` table. The TUI `/model` command shows that model list and writes the selected `model_provider` plus `model` back to the config.

The local TUI/CLI does not derive key names from provider ids. It loads `${DARWIN_CODE_HOME}/.env` early, without overriding existing parent environment variables. Direct `api_key` remains a local backward-compatible escape hatch, but docs and templates use `api_key_env`. Darwin Code does not support hosted OAuth/device sign-in, workspace/cloud auth, command-backed bearer tokens, keychain storage, or implicit OpenAI login.


### DeepSeek provider example

```toml
model_provider = "deepseek"
model = "deepseek-v4-flash"

[openai_compatible.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat-completions"
api_key_env = "DEEPSEEK_API_KEY"

[openai_compatible.deepseek.models."deepseek-v4-flash"]
display_name = "DeepSeek v4 Flash"
context_window = 128000
max_output_tokens = 8192
thinking = true
multimodal = true

[openai_compatible.deepseek.models."deepseek-v4-pro"]
display_name = "DeepSeek v4 Pro"
context_window = 128000
max_output_tokens = 8192
thinking = true
multimodal = true
```

### Generic OpenAI-compatible endpoint

Any external gateway that implements an OpenAI-compatible API, such as a company gateway or a local proxy, appears in Darwin Code as a standard endpoint:

```toml
model_provider = "my-gateway"
model = "your-model"

[openai_compatible.my-gateway]
name = "My Gateway"
base_url = "http://127.0.0.1:8080/v1"
wire_api = "responses"
api_key_env = "MY_GATEWAY_API_KEY"

[openai_compatible.my-gateway.models."your-model"]
display_name = "Your Model"
context_window = 128000
max_output_tokens = 8192
thinking = true
multimodal = true
```

`wire_api` must match the protocol path that the provider actually supports. DeepSeek's official API uses `chat-completions`, which sends requests to `/chat/completions`. Gateways compatible with the OpenAI Responses API use `responses`, which sends requests to `/responses`. Do not set DeepSeek's `base_url` to a `/v1` URL and then use Responses API mode; that would hit DeepSeek's unsupported `/v1/responses` path.

### OpenAI / Gemini / Claude native providers

Model IDs below are current provider examples; access still depends on your account, region, and provider rollout.

```toml
model_provider = "openai"
model = "gpt-5.5"

[openai]
base_url = "https://api.openai.com/v1"
api_key_env = "OPENAI_API_KEY"
```

```toml
model_provider = "gemini"
model = "gemini-3.1-pro-preview"

[gemini]
base_url = "https://generativelanguage.googleapis.com/v1beta"
api_key_env = "GEMINI_API_KEY"
```

```toml
model_provider = "claude"
model = "claude-opus-4-7"

[claude]
base_url = "https://api.anthropic.com"
api_key_env = "CLAUDE_API_KEY"
```

The current runnable projection primarily covers `[openai_compatible.<id>]`. If Gemini or Claude native runtime adapters are not yet available, selecting those providers directly will fail with an explicit error. To use Gemini or Claude through an OpenAI-compatible gateway in the meantime, configure `[openai_compatible.<id>]` with the gateway `base_url` and `api_key_env`.

## License and upstream

Darwin Code is a downstream derivative of OpenAI Codex. See [`LICENSE`](LICENSE), [`NOTICE`](NOTICE), and [`docs/license.md`](docs/license.md) for upstream attribution, downstream modifications, and third-party license notes.
