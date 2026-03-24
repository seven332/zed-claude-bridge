# Architecture

## Problem

Claude Code CLI has built-in IDE integration (`/ide`) that works with VS Code via a local WebSocket MCP server. When running Claude Code in Zed's terminal, this integration is unavailable because Zed doesn't run the VS Code extension.

## Solution

`zed-claude-bridge` is a Rust process that:

1. Runs a WebSocket MCP server mimicking the VS Code extension's protocol
2. Writes a lock file to `~/.claude/ide/{port}.lock` so Claude Code auto-discovers it
3. Receives editor state from Zed via HTTP POST (triggered by Zed task or `send-selection` subcommand)
4. Sends `selection_changed` notifications to Claude Code via MCP
5. Exposes MCP tools (`getCurrentSelection`, `getLatestSelection`, `getWorkspaceFolders`) to Claude Code

```
┌──────────────┐     Zed Task / HTTP     ┌─────────────────────┐     WebSocket (MCP)    ┌──────────────┐
│              │    ──────────────────>   │                     │   <──────────────────   │              │
│   Zed Editor │    (selection, files)    │  zed-claude-bridge  │   (tools/call, etc.)   │  Claude Code │
│              │                          │                     │   ──────────────────>   │   CLI        │
└──────────────┘                          └─────────────────────┘   (tool results,       └──────────────┘
       │                                          │                  notifications)
       │  $ZED_SELECTED_TEXT                      │  ~/.claude/ide/{port}.lock
       │  $ZED_FILE                               │  ws://127.0.0.1:{port}
       │  $ZED_ROW / $ZED_COLUMN                  │
       │  $ZED_LANGUAGE                           │
       └──────────────────────────────────────────┘
```

## Components

### 1. Bridge Server (`src/server.rs`)

An axum + rmcp process that:

- Starts an HTTP + WebSocket server on `127.0.0.1:{random_port}`
- Implements MCP protocol (JSON-RPC 2.0) via rmcp with the same tools as VS Code extension
- Manages lock file lifecycle (create on start, remove on exit/signal/panic)
- Authenticates both HTTP and WebSocket connections via `x-claude-code-ide-authorization` header
- Sends `selection_changed` notifications via rmcp `Peer::send_notification(CustomNotification)`

### 2. State Store (`src/state.rs`)

In-memory store holding current editor state received from Zed:

- `current_selection` / `latest_selection` — protected by `RwLock`, updated atomically (both locks held simultaneously)
- `workspace_folders` — set at startup
- `selection_tx` — `broadcast::Sender` for notifying WebSocket connections of changes

### 3. Minimal LSP (`src/lsp.rs`)

Async LSP server on stdio for Zed extension mode (`--stdio`). Handles only `initialize`, `shutdown`, `exit`. Empty capabilities — exists solely to keep Zed's language server manager happy.

### 4. WebSocket Transport (`src/transport.rs`)

Adapter bridging axum's `WebSocket` to rmcp's `Transport<RoleServer>` trait. Splits into shared `SplitSink` (via `Arc<Mutex>`) and owned `SplitStream`.

### 5. MCP Tools (`src/tools.rs`)

Implements `ServerHandler` trait with three tools:

- `getCurrentSelection` — returns current editor selection
- `getLatestSelection` — returns most recent selection (persists after deselect)
- `getWorkspaceFolders` — returns workspace folder paths

### 6. Send Selection Subcommand (`send-selection`)

Built-in Rust subcommand (`zed-claude-bridge send-selection`) that:

- Reads `$ZED_SELECTED_TEXT`, `$ZED_FILE`, `$ZED_ROW`, `$ZED_COLUMN`, `$ZED_LANGUAGE` env vars
- Finds the latest lock file in `~/.claude/ide/`
- POSTs the selection data to the bridge's HTTP endpoint with auth

Replaces the previous shell script approach — no dependency on `jq` or `curl`.

### 7. Zed Extension (`zed-extension/`)

WASM extension that registers the bridge as a "language server" for common languages. Zed auto-starts/stops the bridge when a workspace opens/closes. The extension auto-downloads the binary from GitHub releases, cached per version.

### 8. Zed Integration (`zed/`)

Global Zed configuration:

- **`tasks.json`**: Task that runs `~/.claude/bin/zed-claude-bridge send-selection`
- **`keymap.json`**: Binds `shift-cmd-l` to trigger the send selection task

## Data Flow

### Selection Flow

1. User selects code in Zed
2. User presses `Shift+Cmd+L` (configured keybinding)
3. Zed task runs `zed-claude-bridge send-selection` (reads env vars, POSTs to bridge)
4. Bridge updates in-memory state (both locks held atomically)
5. Bridge broadcasts change; notification loop sends `selection_changed` via rmcp `CustomNotification`
6. Claude Code receives notification immediately — selection visible in context

### Startup Flow (Extension Mode)

1. User opens a file in Zed
2. Zed extension's `language_server_command` returns `zed-claude-bridge --stdio <workspace>`
3. Bridge starts: WebSocket MCP server + async LSP on stdio
4. Lock file written to `~/.claude/ide/{port}.lock`
5. Claude Code discovers bridge via lock file, connects via WebSocket
6. MCP handshake completes, tools available

## Authentication

Both HTTP and WebSocket endpoints require the same auth token:

1. UUID v4 token generated on startup
2. Written to lock file
3. Claude Code reads lock file, sends token in `x-claude-code-ide-authorization` WebSocket header
4. `send-selection` subcommand reads token from lock file for HTTP POST

## Limitations

| Feature | VS Code | zed-claude-bridge |
|---------|---------|-------------------|
| Selection | Auto (real-time) | Manual (keybinding trigger) |
| Diagnostics | Auto (real-time from LSP) | Not yet (Phase 2) |
| Open editors | Auto | Not yet (Phase 2) |
| Open/navigate file | Full API | Not yet (`zed` CLI possible) |
| Inline diffs | Native diff viewer | Not possible (terminal-only) |
| Jupyter execution | Full support | Not applicable |

## Future Improvements

- **`getDiagnostics`**: Integrate with CLI linters (tsc, eslint, ruff) or LSP proxy
- **`getOpenEditors`**: Track open files
- **`openFile`**: Use `zed` CLI command (`zed file.ts:10`)
- **Zed extension API**: When Zed exposes selection events, eliminate the task trigger step
