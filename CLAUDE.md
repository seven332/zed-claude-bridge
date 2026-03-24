# zed-claude-bridge

A bridge that lets Claude Code CLI running in Zed's terminal use `/ide` features (selection, diagnostics, etc.) by mimicking the VS Code extension's local MCP server.

## How It Works

Claude Code CLI auto-discovers IDE integrations by reading lock files from `~/.claude/ide/`. This project runs a WebSocket MCP server that pretends to be an IDE extension, so Claude Code thinks it's running inside an IDE.

Read these docs before implementing:

- `docs/protocol.md` — Full reverse-engineered protocol (lock file format, WebSocket auth, MCP tool schemas, notifications)
- `docs/architecture.md` — System design, data flow, component breakdown, limitations

## Tech Stack

- **Language**: Rust
- **Async runtime**: tokio
- **HTTP/WebSocket**: axum (with WebSocket upgrade)
- **MCP**: rmcp (official Rust MCP SDK)
- **Build**: cargo

## Project Structure

```
src/
  main.rs          -- CLI entry point (clap, --stdio mode)
  lib.rs           -- Library root (re-exports modules)
  server.rs        -- axum HTTP + WebSocket server
  state.rs         -- Editor state store (Arc<RwLock>)
  lock.rs          -- Lock file management + signal cleanup
  lsp.rs           -- Minimal LSP server on stdio (for Zed extension)
  tools.rs         -- MCP tool handler (ServerHandler impl)
  transport.rs     -- axum WebSocket <-> rmcp Transport adapter
  types.rs         -- Shared types (SelectionInput, SelectionState, etc.)
tests/
  integration.rs   -- End-to-end tests (HTTP, WebSocket, MCP protocol)
zed/
  tasks.json       -- Zed task definition (global: ~/.config/zed/tasks.json)
  keymap.json      -- Keybinding (global: ~/.config/zed/keymap.json)
zed-extension/     -- Zed extension (WASM, registers bridge as language server)
  extension.toml   -- Extension manifest
  Cargo.toml       -- WASM build config
  src/lib.rs       -- Extension implementation
```

## Phase 1 (MVP) — Done

1. Lock file management (write/cleanup on signals/panic)
2. WebSocket MCP server + HTTP API on same port
3. `POST /api/selection` to receive editor state (auth required)
4. MCP tools: `getCurrentSelection`, `getLatestSelection`, `getWorkspaceFolders`
5. `selection_changed` notification via rmcp `CustomNotification` (50ms debounce)
6. `send-selection` subcommand (replaces shell script, no jq/curl dependency)
7. Zed task + keybinding config for pushing selection

## Zed Extension — Done

The bridge can run as a Zed extension for automatic lifecycle management:

1. In Zed: Extensions → Install Dev Extension → select `zed-extension/` directory
2. The extension auto-downloads the binary from GitHub releases
3. The bridge auto-starts when any file is opened (registered for common languages)
4. Selection pushing still requires the Zed task + keybinding (Zed API limitation)

The extension registers the bridge as a "language server" with empty capabilities. Zed spawns the bridge with `--stdio`, which runs a minimal LSP on stdin/stdout (to keep Zed happy) alongside the MCP WebSocket server (for Claude Code).

## Phase 2: Enhanced

- `getDiagnostics` via CLI linters (tsc, eslint, ruff)
- `getOpenEditors` tracking
- `openFile` via `zed` CLI command

## Key Protocol Details

- Lock file: `~/.claude/ide/{port}.lock`, permissions `0600`, dir `0700`
- Auth: `x-claude-code-ide-authorization` WebSocket header = lock file's `authToken` (UUID v4)
- The CLI also checks env var `CLAUDE_CODE_SSE_PORT` to find the port
- All MCP tool responses are wrapped in `{ content: [{ type: "text", text: JSON.stringify(result) }] }`
- Server name should be `"zed-claude-bridge"`

## Commit Convention

Use [Conventional Commits](https://www.conventionalcommits.org/). Format:

```
<type>(<scope>): <description>
```

Types: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`, `ci`

Examples:
- `docs: add protocol and architecture specs`
- `feat(server): implement WebSocket MCP server`
- `fix(lock): clean up lock file on SIGTERM`

## Development

```bash
# Build
cargo build

# Run (standalone mode)
cargo run -- /path/to/workspace

# Run (stdio mode, used by Zed extension)
cargo run -- --stdio /path/to/workspace

# Run tests
cargo test

# Build release
cargo build --release

# Build Zed extension
cd zed-extension && cargo build --target wasm32-wasip1 --release
```

## Testing

### Standalone mode
1. Start the bridge: `cargo run -- /path/to/workspace`
2. Verify lock file exists: `cat ~/.claude/ide/*.lock`
3. Start Claude Code in same terminal: `claude`
4. Claude Code should detect the IDE connection
5. In Zed, select some code, trigger the task
6. In Claude Code, ask "what code did I select?" — it should use `getCurrentSelection`

### Extension mode
1. In Zed: Extensions → Install Dev Extension → select `zed-extension/`
2. Open any file — bridge auto-starts (binary auto-downloaded from GitHub releases)
3. Start `claude` in any terminal — it discovers via lock file
4. Use Zed task to push selection (binary symlinked to `~/.claude/bin/` on first start)
