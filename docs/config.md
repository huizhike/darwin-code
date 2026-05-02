# Configuration

## BYOK providers

Darwin Code 的默认产品配置是 BYOK：用户在 `config.toml` 中选择一个 provider id，
指定模型、endpoint，并直接写入本地 direct `api_key`。模型认证路径不再读取旧环境变量字段，也不从 provider id 推断环境变量。仓库模板可以保留占位 key，但不要提交真实
密钥；真实 key 应只存在于本地未跟踪配置或用户自己的私有配置中。Darwin Code 不实现
`.env` loader、OAuth/device sign-in、workspace/cloud auth、command-backed bearer token、
keychain 或隐式 OpenAI 登录。

Runtime behavior:

- `model_provider` selects a built-in native id (`openai`, `gemini`, `claude`) or an id from `[openai_compatible.<id>]`.
- `model` is the provider model id.
- `api_key` is the direct BYOK key Darwin uses for model auth.
- `base_url` is owned by the user config. External gateways such as a company proxy or local proxy are just standard endpoints from Darwin Code's perspective.
- `wire_api` selects the provider's protocol path. Use `chat-completions` for providers such as DeepSeek that expose `/chat/completions`; use `responses` for OpenAI Responses-compatible gateways that expose `/responses`.
- Defining the same id in both `[openai_compatible.<id>]` and `[model_providers.<id>]` is rejected to avoid ambiguous provider merging.

### OpenAI official BYOK

Use a currently available model ID for your account. The examples below track the latest provider examples verified during docs updates.

```toml
model_provider = "openai"
model = "gpt-5.5"

[openai]
base_url = "https://api.openai.com/v1"
api_key = "你的 OpenAI API Key"
```

### DeepSeek / generic OpenAI-compatible

```toml
model_provider = "deepseek"
model = "deepseek-chat"

[openai_compatible.deepseek]
name = "DeepSeek"
base_url = "https://api.deepseek.com"
wire_api = "chat-completions"
api_key = "你的 DeepSeek API Key"
```

Generic gateway:

```toml
model_provider = "my-provider"
model = "your-model"

[openai_compatible.my-provider]
name = "My Provider"
base_url = "https://provider.example/v1"
wire_api = "responses" # 如果该网关实现 OpenAI Responses API；Chat-only provider 改用 "chat-completions"
api_key = "你的 Provider API Key"
```

### Gemini / Claude native target families

```toml
model_provider = "gemini"
model = "gemini-3.1-pro-preview"

[gemini]
base_url = "https://generativelanguage.googleapis.com/v1beta"
api_key = "你的 Gemini API Key"
```

```toml
model_provider = "claude"
model = "claude-opus-4-7"

[claude]
base_url = "https://api.anthropic.com"
api_key = "你的 Claude API Key"
```

As of this implementation slice, the runtime adapter projection is implemented for
`[openai_compatible.<id>]`. Selecting native `[gemini]` or `[claude]` returns an actionable
runtime-gap error instead of silently falling back to OpenAI. Until those native adapters land,
use an OpenAI-compatible gateway via `[openai_compatible.<id>]`.

Advanced users may still define custom providers under `[model_providers.<id>]`, but that
inherited compatibility surface is constrained by the BYOK boundary: use direct `api_key` or
`api_key_env`; do not configure hosted-account authentication.

## Product configuration boundary

The Darwin product configuration surface is intentionally small: BYOK providers,
model selection, permissions/sandboxing, MCP servers, plugins/skills, local
tooling, and stable feature switches.

Inherited Darwin Code product sections for analytics, feedback, app connectors,
external telemetry, realtime/audio, notice state, file-opener preferences, and
legacy experimental realtime/prompt/tool aliases are no longer valid user TOML
configuration. Some similarly named concepts can still appear in Rust runtime
types, protocol objects, tests, or feature identifiers while a deeper cleanup is
staged; those are internal implementation details and are not user-facing
configuration sections.

## Connecting to MCP servers

Darwin Code can connect to MCP servers configured in `~/.darwin/config.toml`.

MCP tools default to serialized calls. To mark every tool exposed by one server
as eligible for parallel tool calls, set `supports_parallel_tool_calls` on that
server:

```toml
[mcp_servers.docs]
command = "docs-server"
supports_parallel_tool_calls = true
```

Only enable parallel calls for MCP servers whose tools are safe to run at the
same time. If tools read and write shared state, files, databases, or external
resources, review those read/write race conditions before enabling this setting.

## MCP tool approvals

Darwin Code stores approval defaults and per-tool overrides for custom MCP servers
under `mcp_servers` in `~/.darwin/config.toml`. Set
`default_tools_approval_mode` on the server to apply a default to every tool,
and use per-tool `approval_mode` entries for exceptions:

```toml
[mcp_servers.docs]
command = "docs-server"
default_tools_approval_mode = "approve"

[mcp_servers.docs.tools.search]
approval_mode = "prompt"
```

## Notify

Darwin Code can run a notification hook when the agent finishes a turn.

When Darwin Code knows which client started the turn, the legacy notify JSON payload also includes a top-level `client` field. The TUI reports `darwin-code-tui`, and the app server reports the `clientInfo.name` value from `initialize`.

## JSON Schema

The generated JSON Schema for `config.toml` lives at `darwin-rs/core/config.schema.json`.

## SQLite State DB

Darwin Code stores the SQLite-backed state DB under `sqlite_home` (config key) or the
`DARWIN_CODE_SQLITE_HOME` environment variable. When unset, WorkspaceWrite sandbox
sessions default to a temp directory; other modes default to `DARWIN_CODE_HOME`.

## Custom CA Certificates

Darwin Code can trust a custom root CA bundle for outbound HTTPS connections when
enterprise proxies or gateways intercept TLS. Darwin model traffic is BYOK
HTTP/SSE, not hosted login or Responses WebSocket; this CA setting also applies
to other Darwin components that build reqwest clients and remote MCP connections
that use the shared `darwin-client` CA-loading path.

Set `DARWIN_CODE_CA_CERTIFICATE` to the path of a PEM file containing one or more
certificate blocks to use a Darwin Code-specific CA bundle. If
`DARWIN_CODE_CA_CERTIFICATE` is unset, Darwin Code falls back to `SSL_CERT_FILE`. If
neither variable is set, Darwin Code uses the system root certificates.

`DARWIN_CODE_CA_CERTIFICATE` takes precedence over `SSL_CERT_FILE`. Empty values are
treated as unset.

The PEM file may contain multiple certificates. Darwin Code also tolerates OpenSSL
`TRUSTED CERTIFICATE` labels and ignores well-formed `X509 CRL` sections in the
same bundle. If the file is empty, unreadable, or malformed, the affected Darwin HTTP
connection reports a user-facing error that points back to these environment
variables.

## Plan mode defaults

`plan_mode_reasoning_effort` lets you set a Plan-mode-specific default reasoning
effort override. When unset, Plan mode uses the built-in Plan preset default
(currently `medium`). When explicitly set (including `none`), it overrides the
Plan preset. The string value `none` means "no reasoning" (an explicit Plan
override), not "inherit the global default". There is currently no separate
config value for "follow the global default in Plan mode".

Ctrl+C/Ctrl+D quitting uses a ~1 second double-press hint (`ctrl + c again to quit`).
