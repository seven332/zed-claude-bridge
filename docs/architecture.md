# Architecture

## Problem

Claude Code CLI has built-in IDE integration (`/ide`) that works with VS Code via a local WebSocket MCP server. When running Claude Code in Zed's terminal, this integration is unavailable because Zed doesn't run the VS Code extension.

## Solution

`zed-claude-bridge` is a standalone process that:

1. Runs a WebSocket MCP server mimicking the VS Code extension's protocol
2. Writes a lock file to `~/.claude/ide/{port}.lock` so Claude Code auto-discovers it
3. Receives editor state from Zed via a lightweight IPC mechanism
4. Exposes MCP tools (`getCurrentSelection`, `getDiagnostics`, etc.) to Claude Code

```
┌──────────────┐     Zed Task / HTTP     ┌─────────────────────┐     WebSocket (MCP)    ┌──────────────┐
│              │    ──────────────────>   │                     │   <──────────────────   │              │
│   Zed Editor │    (selection, files)    │  zed-claude-bridge  │   (tools/call, etc.)   │  Claude Code │
│              │                          │                     │   ──────────────────>   │   CLI        │
└──────────────┘                          └─────────────────────┘   (tool results)       └──────────────┘
       │                                          │
       │  $ZED_SELECTED_TEXT                      │  ~/.claude/ide/{port}.lock
       │  $ZED_FILE                               │  ws://127.0.0.1:{port}
       │  $ZED_ROW / $ZED_COLUMN                  │
       │  $ZED_LANGUAGE                           │
       └──────────────────────────────────────────┘
```

## Components

### 1. Bridge Server (`src/server.ts`)

A Node.js process that:

- Starts an HTTP server on `127.0.0.1:{random_port}`
- Wraps it with a WebSocket server (using `ws` library)
- Implements MCP protocol (JSON-RPC 2.0) with the same tools as VS Code extension
- Manages lock file lifecycle (create on start, remove on exit)
- Authenticates connections via `x-claude-code-ide-authorization` header
- Also exposes a small HTTP API on the same port for Zed to push state updates

### 2. State Store (`src/state.ts`)

In-memory store holding current editor state received from Zed:

```typescript
interface EditorState {
  selection: {
    text: string;
    filePath: string;
    fileUrl: string;
    language: string;
    selection: {
      start: { line: number; character: number };
      end: { line: number; character: number };
      isEmpty: boolean;
    };
  } | null;
  openFiles: string[];       // paths of files currently open (from Zed tasks)
  workspaceFolders: string[]; // workspace root(s)
}
```

### 3. Zed Integration

#### Tasks (`.zed/tasks.json`)

Zed tasks push editor context to the bridge via HTTP POST:

- **Send Selection**: Triggered by keybinding, POSTs `$ZED_SELECTED_TEXT`, `$ZED_FILE`, `$ZED_ROW`, `$ZED_COLUMN`, `$ZED_LANGUAGE` to `http://127.0.0.1:{port}/api/selection`
- **Send Open File**: Triggered on file focus, POSTs `$ZED_FILE` to `http://127.0.0.1:{port}/api/open-file`

#### Keybindings (`keymap.json`)

```json
{
  "context": "Editor",
  "bindings": {
    "ctrl-shift-s": ["task::Spawn", { "task_name": "zed-claude: send selection" }]
  }
}
```

## Data Flow

### Selection Flow

1. User selects code in Zed
2. User presses `Ctrl+Shift+S` (or configured keybinding)
3. Zed task runs: `curl -s -X POST http://127.0.0.1:{port}/api/selection -H 'Content-Type: application/json' -d '...'`
4. Bridge server updates in-memory state
5. Bridge server pushes `selection_changed` notification to connected Claude Code CLI via WebSocket
6. Claude Code now knows what's selected (visible in context, usable via `getCurrentSelection` tool)

### Diagnostics Flow (Phase 2)

Since Zed's extension API doesn't expose diagnostics, we have two options:

**Option A: LSP Proxy** (complex but accurate)
- Bridge server runs its own LSP clients for the same language servers Zed uses
- Keeps diagnostics in sync independently
- Pro: Real diagnostics. Con: Duplicated LSP processes, may diverge from Zed.

**Option B: Zed Terminal + LSP CLI** (simpler)
- Use CLI lint tools (e.g., `tsc --noEmit`, `eslint`, `ruff check`) triggered periodically or on file save
- Parse their output into the MCP diagnostics format
- Pro: Simple. Con: Not real-time, different from Zed's UI.

**Option C: Wait for Zed API** (ideal)
- Zed is actively developing richer extension APIs
- When diagnostics become accessible, integrate directly

## Authentication

The bridge uses the same auth scheme as VS Code:

1. Generate a UUID v4 token on startup
2. Write it to the lock file
3. CLI reads lock file, sends token in `x-claude-code-ide-authorization` WebSocket header
4. For the HTTP API (Zed → bridge), use a separate local file token or rely on localhost-only binding

## File Structure

```
zed-claude-bridge/
├── src/
│   ├── index.ts          # Entry point
│   ├── server.ts         # WebSocket MCP server + HTTP API
│   ├── state.ts          # Editor state store
│   ├── lock.ts           # Lock file management
│   ├── tools.ts          # MCP tool implementations
│   └── notifications.ts  # Server-to-client notifications
├── zed/
│   ├── tasks.json        # Zed task definitions (copy to .zed/)
│   └── keymap.json       # Recommended keybindings
├── docs/
│   ├── protocol.md       # Reverse-engineered MCP protocol
│   └── architecture.md   # This file
├── package.json
├── tsconfig.json
└── README.md
```

## Limitations

| Feature | VS Code | zed-claude-bridge |
|---------|---------|-------------------|
| Selection | Auto (real-time) | Manual (keybinding trigger) |
| Diagnostics | Auto (real-time from LSP) | Phase 2 (CLI linters or LSP proxy) |
| Open editors | Auto | Partial (via Zed tasks) |
| Open/navigate file | Full API | Can use `zed` CLI (`zed path/to/file:line`) |
| Inline diffs | Native diff viewer | Not possible (terminal-only) |
| Jupyter execution | Full support | Not applicable |

## Future Improvements

- **File watcher**: Auto-detect active file changes instead of requiring manual keybinding
- **Zed CLI integration**: Use `zed` command for `openFile` tool (e.g., `zed file.ts:10`)
- **Diagnostics**: Integrate with CLI linters or LSP proxy
- **Auto-start**: Launch bridge automatically when Zed opens (via Zed task with `reveal: "never"`)
- **Zed extension**: If/when Zed exposes richer APIs, migrate to a proper WASM extension
