# Getting started with Darwin Code

Darwin Code is a BYOK-only execution engine. Configure an explicit provider,
model, endpoint, and user-owned API key before launching the TUI or `exec`.

```toml
model_provider = "my-gateway"
model = "your-model"

[openai_compatible.my-gateway]
name = "My Gateway"
base_url = "https://provider.example/v1"
wire_api = "responses"
api_key = "your gateway key"
```

Run from source:

```bash
cd darwin-rs
cargo clean
cargo run -p darwin-code-cli --bin darwin-code
```

Use `docs/config.md` for provider details and `docs/install.md` for source
build notes.
