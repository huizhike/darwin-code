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
cp config-example.toml config.toml
$EDITOR config.toml
cd darwin-rs
cargo clean
cargo run -p darwin-code-cli --bin darwin-code
```

`config-example.toml` is the redacted template. `config.toml` is the local real configuration and must not be committed. When running from source, Darwin Code reads `config.toml` from the checkout root by default. Set `DARWIN_CODE_HOME` if you need a different config directory.

Use another config directory:

```bash
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

Then edit the packaged config template and replace placeholder API keys:

```bash
$EDITOR "$(npm root -g)/@darwin-code/darwin-code/config.toml"
```

You can also keep configuration under your user directory by copying the packaged template and editing it there:

```bash
mkdir -p ~/.darwin
cp "$(npm root -g)/@darwin-code/darwin-code/config.toml" ~/.darwin/config.toml
$EDITOR ~/.darwin/config.toml
```

At minimum, verify these four groups of fields in `config.toml`:

```toml
model_provider = "qwen"        # Current provider id
model = "qwen3.6-plus"         # Current model; /model lists this provider's models

[openai_compatible.qwen]
name = "Qwen"
base_url = "https://token-plan.cn-beijing.maas.aliyuncs.com/compatible-mode/v1"
wire_api = "chat-completions"  # Use chat-completions or responses to match the endpoint
api_key = "YOUR_QWEN_API_KEY"  # Put real keys only in local config.toml
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

Darwin Code's default product path is **BYOK (Bring Your Own Key)**: select the provider, model, and `base_url` explicitly in `config.toml`, then provide your own `api_key` directly. Each provider declares selectable models under its own `models` table. The TUI `/model` command shows that model list and writes the selected `model_provider` plus `model` back to the config.

The local TUI/CLI no longer requires environment variables for model keys and no longer derives environment variable names from provider ids. Beyond BYOK direct keys, Darwin Code does not support hosted OAuth/device sign-in, workspace/cloud auth, command-backed bearer tokens, keychain storage, `.env` loading, or implicit OpenAI login.

### DeepSeek provider example

```toml
model_provider = "deepseek"
model = "deepseek-v4-flash"

[openai_compatible.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat-completions"
api_key = "YOUR_DEEPSEEK_API_KEY"

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
api_key = "YOUR_GATEWAY_API_KEY"

[openai_compatible.my-gateway.models."your-model"]
display_name = "Your Model"
context_window = 128000
max_output_tokens = 8192
thinking = true
multimodal = true
```

`wire_api` must match the protocol path that the provider actually supports. DeepSeek's official API uses `chat-completions`, which sends requests to `/chat/completions`. Gateways compatible with the OpenAI Responses API use `responses`, which sends requests to `/responses`. Do not set DeepSeek's `base_url` to a `/v1` URL and then use Responses API mode; that would hit DeepSeek's unsupported `/v1/responses` path.

### OpenAI / Gemini / Claude direct key

Model IDs below are current provider examples; access still depends on your account, region, and provider rollout.

```toml
model_provider = "openai"
model = "gpt-5.5"

[openai]
base_url = "https://api.openai.com/v1"
api_key = "YOUR_OPENAI_API_KEY"
```

```toml
model_provider = "gemini"
model = "gemini-3.1-pro-preview"

[gemini]
base_url = "https://generativelanguage.googleapis.com/v1beta"
api_key = "YOUR_GEMINI_API_KEY"
```

```toml
model_provider = "claude"
model = "claude-opus-4-7"

[claude]
base_url = "https://api.anthropic.com"
api_key = "YOUR_CLAUDE_API_KEY"
```

The current runnable projection primarily covers `[openai_compatible.<id>]`. If Gemini or Claude native runtime adapters are not yet available, selecting those providers directly will fail with an explicit error. To use Gemini or Claude through an OpenAI-compatible gateway in the meantime, configure `[openai_compatible.<id>]` with the gateway `base_url` and `api_key`.

## License and upstream

Darwin Code is a downstream derivative of OpenAI Codex. See [`LICENSE`](LICENSE), [`NOTICE`](NOTICE), and [`docs/license.md`](docs/license.md) for upstream attribution, downstream modifications, and third-party license notes.
