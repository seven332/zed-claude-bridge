# zed-claude-bridge

A bridge that lets Claude Code CLI running in Zed's terminal use `/ide` features (selection, diagnostics, etc.) by mimicking the VS Code extension's local MCP server.

## How It Works

Claude Code CLI auto-discovers IDE integrations by reading lock files from `~/.claude/ide/`. This project runs a WebSocket MCP server that pretends to be an IDE extension, so Claude Code thinks it's running inside an IDE.

Read these docs before implementing:

- `docs/protocol.md` — Full reverse-engineered protocol (lock file format, WebSocket auth, MCP tool schemas, notifications)
- `docs/architecture.md` — System design, data flow, component breakdown, limitations

## Tech Stack

- **Runtime**: Node.js (TypeScript)
- **WebSocket**: `ws` library (same as VS Code extension uses)
- **MCP**: `@modelcontextprotocol/sdk` for protocol implementation
- **Build**: tsup or tsc

## Implementation Priority

### Phase 1: Core (MVP)

1. **Lock file management** — Write `~/.claude/ide/{port}.lock` on start, clean up on exit (SIGINT, SIGTERM, uncaughtException). Format in `docs/protocol.md#lock-file`.
2. **WebSocket MCP server** — HTTP server on `127.0.0.1:{random_port}`, WebSocket upgrade with auth via `x-claude-code-ide-authorization` header. Use `@modelcontextprotocol/sdk` McpServer class with WebSocket transport.
3. **HTTP API for Zed** — On the same HTTP server, handle `POST /api/selection` to receive editor state from Zed tasks. Store in memory.
4. **MCP tools** — Implement at minimum: `getCurrentSelection`, `getLatestSelection`, `getWorkspaceFolders`. Return data from in-memory state.
5. **Notifications** — Push `selection_changed` via WebSocket when state updates.
6. **Zed tasks** — Provide `.zed/tasks.json` with a task that POSTs `$ZED_SELECTED_TEXT`, `$ZED_FILE`, `$ZED_ROW`, `$ZED_COLUMN`, `$ZED_LANGUAGE` to the bridge.

### Phase 2: Enhanced

- `getDiagnostics` via CLI linters (tsc, eslint, ruff)
- `getOpenEditors` tracking
- `openFile` via `zed` CLI command
- Auto-start bridge when Zed opens

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

## Testing

1. Start the bridge: `npx tsx src/index.ts /path/to/workspace`
2. Verify lock file exists: `cat ~/.claude/ide/*.lock`
3. Start Claude Code in same terminal: `claude`
4. Claude Code should detect the IDE connection
5. In Zed, select some code, trigger the task
6. In Claude Code, ask "what code did I select?" — it should use `getCurrentSelection`
