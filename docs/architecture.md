# Darwin Code Architecture Diagrams

This document is a diagram-first overview of the current Darwin Code runtime in
`70-darwin-code`. It focuses on the runnable BYOK execution engine, not on
legacy Codex hosted-account compatibility.

## 1. System context

```mermaid
%%{init: {"theme": "base", "themeVariables": {"background": "#f5f5f7", "mainBkg": "#f5f5f7", "primaryColor": "#f5f5f7", "primaryTextColor": "#1d1d1f", "primaryBorderColor": "#86868b", "secondaryColor": "#e8e8ed", "secondaryTextColor": "#1d1d1f", "secondaryBorderColor": "#a1a1a6", "tertiaryColor": "#ffffff", "tertiaryTextColor": "#1d1d1f", "tertiaryBorderColor": "#d2d2d7", "clusterBkg": "#f5f5f7", "clusterBorder": "#d2d2d7", "lineColor": "#6e6e73", "edgeLabelBackground": "#ffffff", "actorBkg": "#f5f5f7", "actorBorder": "#86868b", "actorTextColor": "#1d1d1f", "activationBkgColor": "#e8e8ed", "activationBorderColor": "#86868b", "noteBkgColor": "#f5f5f7", "noteTextColor": "#1d1d1f", "fontFamily": "-apple-system, BlinkMacSystemFont, SF Pro Text, Segoe UI, sans-serif"}}}%%
flowchart LR
    user[Developer / operator]
    npm[darwin-cli<br/>Node launcher]
    cli[CLI<br/>darwin-rs/cli]
    tui[TUI / interactive session]
    exec[Non-interactive exec / review]
    app[App Server<br/>JSON-RPC control plane]
    mcpServer[MCP Server<br/>tools]
    core[Core runtime<br/>thread + turn loop]
    providers[BYOK model providers]
    tools[Tool execution boundary]
    state[Local state<br/>JSONL + SQLite]

    user --> npm --> cli
    cli --> tui --> core
    cli --> exec --> core
    cli --> app --> core
    cli --> mcpServer --> core

    core --> providers
    core --> tools
    core --> state
```

## 2. Repository module map

```mermaid
%%{init: {"theme": "base", "themeVariables": {"background": "#f5f5f7", "mainBkg": "#f5f5f7", "primaryColor": "#f5f5f7", "primaryTextColor": "#1d1d1f", "primaryBorderColor": "#86868b", "secondaryColor": "#e8e8ed", "secondaryTextColor": "#1d1d1f", "secondaryBorderColor": "#a1a1a6", "tertiaryColor": "#ffffff", "tertiaryTextColor": "#1d1d1f", "tertiaryBorderColor": "#d2d2d7", "clusterBkg": "#f5f5f7", "clusterBorder": "#d2d2d7", "lineColor": "#6e6e73", "edgeLabelBackground": "#ffffff", "actorBkg": "#f5f5f7", "actorBorder": "#86868b", "actorTextColor": "#1d1d1f", "activationBkgColor": "#e8e8ed", "activationBorderColor": "#86868b", "noteBkgColor": "#f5f5f7", "noteTextColor": "#1d1d1f", "fontFamily": "-apple-system, BlinkMacSystemFont, SF Pro Text, Segoe UI, sans-serif"}}}%%
flowchart TB
    subgraph packaging[Packaging and launch]
        node[Node launcher script]
        pkg[darwin-cli/package.json]
        vendor[packaged vendor binary]
    end

    subgraph entry[Runtime entry points]
        cli[cli]
        tui[tui]
        exec[exec]
        appServer[app-server]
        appProtocol[app-server-protocol]
        mcpServer[mcp-server]
    end

    subgraph coreLayer[Core execution engine]
        core[core]
        protocol[protocol]
        tools[tools]
        sandboxing[sandboxing]
        linuxSandbox[linux-sandbox]
    end

    subgraph providerLayer[Provider and model layer]
        providerInfo[model-provider-info]
        provider[model-provider]
        models[models-manager]
        config[config]
    end

    subgraph extensionLayer[Extension layer]
        mcpClient[MCP client]
        rmcp[rmcp-client]
        plugins[core-plugins / plugin]
        skills[core-skills / skills]
        hooks[hooks]
    end

    subgraph persistence[Persistence and observability]
        rollout[rollout]
        state[state]
        threadStore[thread-store]
        otel[otel]
        analytics[analytics]
    end

    node --> vendor --> cli
    pkg --> node
    cli --> tui
    cli --> exec
    cli --> appServer
    cli --> mcpServer

    tui --> core
    exec --> core
    appServer --> core
    mcpServer --> core
    appServer --> appProtocol

    core --> config
    core --> provider
    core --> models
    core --> tools
    core --> mcpClient
    core --> plugins
    core --> skills
    core --> hooks
    core --> rollout
    core --> state

    provider --> providerInfo
    models --> provider
    tools --> sandboxing
    sandboxing --> linuxSandbox
    rollout --> threadStore
    state --> threadStore
    core --> otel
    core --> analytics
    mcpClient --> rmcp
```

## 3. CLI dispatch shape

```mermaid
%%{init: {"theme": "base", "themeVariables": {"background": "#f5f5f7", "mainBkg": "#f5f5f7", "primaryColor": "#f5f5f7", "primaryTextColor": "#1d1d1f", "primaryBorderColor": "#86868b", "secondaryColor": "#e8e8ed", "secondaryTextColor": "#1d1d1f", "secondaryBorderColor": "#a1a1a6", "tertiaryColor": "#ffffff", "tertiaryTextColor": "#1d1d1f", "tertiaryBorderColor": "#d2d2d7", "clusterBkg": "#f5f5f7", "clusterBorder": "#d2d2d7", "lineColor": "#6e6e73", "edgeLabelBackground": "#ffffff", "actorBkg": "#f5f5f7", "actorBorder": "#86868b", "actorTextColor": "#1d1d1f", "activationBkgColor": "#e8e8ed", "activationBorderColor": "#86868b", "noteBkgColor": "#f5f5f7", "noteTextColor": "#1d1d1f", "fontFamily": "-apple-system, BlinkMacSystemFont, SF Pro Text, Segoe UI, sans-serif"}}}%%
flowchart LR
    cli[CLI]
    root[Root flags<br/>config overrides / features]
    interactive[Default interactive TUI]
    exec[exec]
    review[review]
    mcp[mcp client manager]
    mcpServer[mcp-server]
    appServer[app-server]
    sandbox[sandbox command]
    resume[resume / fork]
    responses[hidden responses command]
    execServer[exec-server]
    features[features]

    cli --> root
    root --> interactive
    root --> exec
    root --> review
    root --> mcp
    root --> mcpServer
    root --> appServer
    root --> sandbox
    root --> resume
    root --> responses
    root --> execServer
    root --> features
```

## 4. Turn runtime lifecycle

```mermaid
%%{init: {"theme": "base", "themeVariables": {"background": "#f5f5f7", "mainBkg": "#f5f5f7", "primaryColor": "#f5f5f7", "primaryTextColor": "#1d1d1f", "primaryBorderColor": "#86868b", "secondaryColor": "#e8e8ed", "secondaryTextColor": "#1d1d1f", "secondaryBorderColor": "#a1a1a6", "tertiaryColor": "#ffffff", "tertiaryTextColor": "#1d1d1f", "tertiaryBorderColor": "#d2d2d7", "clusterBkg": "#f5f5f7", "clusterBorder": "#d2d2d7", "lineColor": "#6e6e73", "edgeLabelBackground": "#ffffff", "actorBkg": "#f5f5f7", "actorBorder": "#86868b", "actorTextColor": "#1d1d1f", "activationBkgColor": "#e8e8ed", "activationBorderColor": "#86868b", "noteBkgColor": "#f5f5f7", "noteTextColor": "#1d1d1f", "fontFamily": "-apple-system, BlinkMacSystemFont, SF Pro Text, Segoe UI, sans-serif"}}}%%
sequenceDiagram
    autonumber
    participant User
    participant Entry as CLI/TUI/AppServer/MCP
    participant Core as core::session::turn
    participant Config as Config + model metadata
    participant Ext as Plugins/Skills/Hooks/MCP inventory
    participant Provider as ModelClientSession
    participant API as Provider HTTP/SSE API
    participant Tools as Tool executor + sandbox
    participant Store as Rollout + SQLite state

    User->>Entry: submit prompt or command
    Entry->>Core: create/resume thread and call run_turn
    Core->>Config: read turn-scoped model/provider/policy
    Core->>Core: optional pre-sampling compaction
    Core->>Store: record context updates
    Core->>Ext: load plugins, skills, hooks, MCP tools
    Ext-->>Core: injected context and tool inventory
    Core->>Provider: stream(provider, prompt, model_info, turn metadata)
    Provider->>API: call responses or chat/completions
    API-->>Provider: SSE response events
    Provider-->>Core: normalized response events
    Core->>Tools: schedule requested tool calls
    Tools->>Tools: apply approval and sandbox policy
    Tools-->>Core: tool output items
    Core->>Provider: follow-up model request if needed
    Core->>Store: persist response items, usage, metadata
    Core-->>Entry: stream UI/events/result
```

## 5. BYOK provider routing

```mermaid
%%{init: {"theme": "base", "themeVariables": {"background": "#f5f5f7", "mainBkg": "#f5f5f7", "primaryColor": "#f5f5f7", "primaryTextColor": "#1d1d1f", "primaryBorderColor": "#86868b", "secondaryColor": "#e8e8ed", "secondaryTextColor": "#1d1d1f", "secondaryBorderColor": "#a1a1a6", "tertiaryColor": "#ffffff", "tertiaryTextColor": "#1d1d1f", "tertiaryBorderColor": "#d2d2d7", "clusterBkg": "#f5f5f7", "clusterBorder": "#d2d2d7", "lineColor": "#6e6e73", "edgeLabelBackground": "#ffffff", "actorBkg": "#f5f5f7", "actorBorder": "#86868b", "actorTextColor": "#1d1d1f", "activationBkgColor": "#e8e8ed", "activationBorderColor": "#86868b", "noteBkgColor": "#f5f5f7", "noteTextColor": "#1d1d1f", "fontFamily": "-apple-system, BlinkMacSystemFont, SF Pro Text, Segoe UI, sans-serif"}}}%%
flowchart TD
    configToml[config.toml]
    native[openai / gemini / claude sections]
    compat[openai_compatible.<id> sections]
    generic[providers.<id> generic BYOK sections]
    providerInfo[ModelProviderInfo]
    validation[BYOK validation]
    auth[resolve_provider_auth]
    wire{wire_api}
    responses[Responses HTTP/SSE]
    chat[Chat Completions HTTP/SSE]
    gemini[Gemini native<br/>declared, runtime gap]
    claude[Claude native<br/>declared, runtime gap]

    configToml --> native
    configToml --> compat
    configToml --> generic
    native --> providerInfo
    compat --> providerInfo
    generic --> providerInfo
    providerInfo --> validation
    validation -->|api_key_env or direct api_key| auth
    validation -. rejects .-> legacy[command auth / experimental bearer token]
    auth --> wire
    wire -->|responses| responses
    wire -->|chat-completions| chat
    wire -->|gemini-generate| gemini
    wire -->|claude-messages| claude
```

## 6. Config, policy, and state surfaces

```mermaid
%%{init: {"theme": "base", "themeVariables": {"background": "#f5f5f7", "mainBkg": "#f5f5f7", "primaryColor": "#f5f5f7", "primaryTextColor": "#1d1d1f", "primaryBorderColor": "#86868b", "secondaryColor": "#e8e8ed", "secondaryTextColor": "#1d1d1f", "secondaryBorderColor": "#a1a1a6", "tertiaryColor": "#ffffff", "tertiaryTextColor": "#1d1d1f", "tertiaryBorderColor": "#d2d2d7", "clusterBkg": "#f5f5f7", "clusterBorder": "#d2d2d7", "lineColor": "#6e6e73", "edgeLabelBackground": "#ffffff", "actorBkg": "#f5f5f7", "actorBorder": "#86868b", "actorTextColor": "#1d1d1f", "activationBkgColor": "#e8e8ed", "activationBorderColor": "#86868b", "noteBkgColor": "#f5f5f7", "noteTextColor": "#1d1d1f", "fontFamily": "-apple-system, BlinkMacSystemFont, SF Pro Text, Segoe UI, sans-serif"}}}%%
flowchart LR
    subgraph inputs[Configuration inputs]
        defaultConfig[checked-in config.toml]
        envFile[ignored .env]
        home[DARWIN_CODE_HOME]
        env[DARWIN_CODE_SQLITE_HOME]
        overrides[CLI/UI runtime overrides]
    end

    subgraph configLayer[Config and policy]
        loader[core::config_loader]
        toml[config::ConfigToml]
        policies[approval / sandbox / tools / MCP / plugins]
        providers[provider and model selection]
    end

    subgraph runtime[Runtime]
        turn[TurnContext]
        model[ModelClientSession]
        tools[Tool orchestration]
    end

    subgraph persistence[Local persistence]
        jsonl[sessions/*.jsonl rollouts]
        stateDb[state_5.sqlite]
        logsDb[logs_2.sqlite]
        cache[models cache]
    end

    defaultConfig --> loader
    envFile --> loader
    home --> loader
    overrides --> loader
    loader --> toml
    toml --> policies
    toml --> providers
    policies --> turn
    providers --> turn
    turn --> model
    turn --> tools
    turn --> jsonl
    turn --> stateDb
    env --> stateDb
    env --> logsDb
    providers --> cache
```

## 7. Tool, extension, and sandbox boundary

```mermaid
%%{init: {"theme": "base", "themeVariables": {"background": "#f5f5f7", "mainBkg": "#f5f5f7", "primaryColor": "#f5f5f7", "primaryTextColor": "#1d1d1f", "primaryBorderColor": "#86868b", "secondaryColor": "#e8e8ed", "secondaryTextColor": "#1d1d1f", "secondaryBorderColor": "#a1a1a6", "tertiaryColor": "#ffffff", "tertiaryTextColor": "#1d1d1f", "tertiaryBorderColor": "#d2d2d7", "clusterBkg": "#f5f5f7", "clusterBorder": "#d2d2d7", "lineColor": "#6e6e73", "edgeLabelBackground": "#ffffff", "actorBkg": "#f5f5f7", "actorBorder": "#86868b", "actorTextColor": "#1d1d1f", "activationBkgColor": "#e8e8ed", "activationBorderColor": "#86868b", "noteBkgColor": "#f5f5f7", "noteTextColor": "#1d1d1f", "fontFamily": "-apple-system, BlinkMacSystemFont, SF Pro Text, Segoe UI, sans-serif"}}}%%
flowchart TB
    model[Model output items]
    core[Core turn loop]
    registry[Tool registry]
    approvals[Approval policy]
    sandbox[SandboxManager]
    shell[Shell command]
    fs[Filesystem tools]
    patch[Apply patch]
    mcp[MCP tools]
    hooks[Hooks]
    plugins[Plugins]
    skills[Skills]
    results[Tool results]

    model --> core --> registry
    registry --> approvals --> sandbox
    sandbox --> shell
    sandbox --> fs
    sandbox --> patch
    registry --> mcp
    hooks --> core
    plugins --> registry
    skills --> core
    shell --> results
    fs --> results
    patch --> results
    mcp --> results
    results --> core
```

## 8. Current implementation status

```mermaid
%%{init: {"theme": "base", "themeVariables": {"background": "#f5f5f7", "mainBkg": "#f5f5f7", "primaryColor": "#f5f5f7", "primaryTextColor": "#1d1d1f", "primaryBorderColor": "#86868b", "secondaryColor": "#e8e8ed", "secondaryTextColor": "#1d1d1f", "secondaryBorderColor": "#a1a1a6", "tertiaryColor": "#ffffff", "tertiaryTextColor": "#1d1d1f", "tertiaryBorderColor": "#d2d2d7", "clusterBkg": "#f5f5f7", "clusterBorder": "#d2d2d7", "lineColor": "#6e6e73", "edgeLabelBackground": "#ffffff", "actorBkg": "#f5f5f7", "actorBorder": "#86868b", "actorTextColor": "#1d1d1f", "activationBkgColor": "#e8e8ed", "activationBorderColor": "#86868b", "noteBkgColor": "#f5f5f7", "noteTextColor": "#1d1d1f", "fontFamily": "-apple-system, BlinkMacSystemFont, SF Pro Text, Segoe UI, sans-serif"}}}%%
flowchart LR
    done1[Runnable<br/>CLI/TUI/exec]
    done2[Runnable<br/>App Server]
    done3[Runnable<br/>MCP server + MCP client tools]
    done4[Runnable<br/>Responses wire API]
    done5[Runnable<br/>Chat Completions wire API]
    partial1[Partial<br/>Gemini native config/auth]
    partial2[Partial<br/>Claude native config/auth]
    gap1[Gap<br/>native Gemini stream]
    gap2[Gap<br/>native Claude stream]

    done1 --> core[Core runtime]
    done2 --> core
    done3 --> core
    core --> done4
    core --> done5
    core --> partial1 --> gap1
    core --> partial2 --> gap2
```

## Source anchors

| Area                                | Source anchors                                                                                   |
| ----------------------------------- | ------------------------------------------------------------------------------------------------ |
| Workspace member set                | `darwin-rs/Cargo.toml:1-78`                                                                      |
| Node launcher                       | `darwin-cli/bin/darwin-code.js:1-123`                                                            |
| CLI command surface                 | `darwin-rs/cli/src/main.rs:84-138`, `darwin-rs/cli/src/main.rs:600-740`                          |
| Turn loop                           | `darwin-rs/core/src/session/turn.rs:129-226`                                                     |
| Provider stream dispatch            | `darwin-rs/core/src/client.rs:664-713`                                                           |
| OpenAI-compatible provider config   | `darwin-rs/config/src/config_toml.rs:147-217`                                                    |
| Generic BYOK provider config        | `darwin-rs/config/src/config_toml.rs:255-359`                                                    |
| Wire APIs and BYOK validation       | `darwin-rs/model-provider-info/src/lib.rs:36-168`                                                |
| Provider auth headers               | `darwin-rs/model-provider/src/auth.rs:23-51`                                                     |
| App Server processor                | `darwin-rs/app-server/src/lib.rs:106-128`, `darwin-rs/app-server/src/message_processor.rs:81-93` |
| MCP server tools                    | `darwin-rs/mcp-server/src/message_processor.rs:311-350`                                          |
| Sandbox selection and transform     | `darwin-rs/sandboxing/src/manager.rs:23-87`, `darwin-rs/sandboxing/src/manager.rs:135-174`       |
| Local state and rollout persistence | `darwin-rs/state/src/lib.rs:57-63`, `darwin-rs/rollout/src/recorder.rs:68-84`                    |
