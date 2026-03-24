# zed-claude-bridge

A bridge that lets Claude Code CLI running in Zed's terminal use `/ide` features (selection, workspace folders, etc.) by mimicking the VS Code extension's local MCP server.

## How It Works

```
┌──────────────┐     HTTP POST          ┌─────────────────────┐     WebSocket (MCP)    ┌──────────────┐
│              │  ──────────────────>    │                     │   <──────────────────   │              │
│   Zed Editor │  (selection, files)    │  zed-claude-bridge  │   (tools/call, etc.)   │  Claude Code │
│              │                         │                     │   ──────────────────>   │   CLI        │
└──────────────┘                         └─────────────────────┘   (tool results,       └──────────────┘
       │                                         │                  notifications)
       │  Ctrl+Shift+S                           │  ~/.claude/ide/{port}.lock
       │  → zed-claude-bridge send-selection     │  ws://127.0.0.1:{port}
       └─────────────────────────────────────────┘
```

Claude Code CLI auto-discovers IDE integrations by reading lock files from `~/.claude/ide/`. This project runs a WebSocket MCP server that pretends to be an IDE extension, so Claude Code thinks it's running inside an IDE.

## Features

- **Selection sharing** — push selected code from Zed to Claude Code via keybinding
- **Workspace folders** — Claude Code knows which project you have open
- **Real-time notifications** — `selection_changed` pushed to Claude Code immediately (50ms debounce)
- **Auto lifecycle** — Zed extension auto-starts/stops the bridge with your workspace
- **Secure** — localhost only, UUID v4 auth token, lock file permissions 0600

## Installation

### 1. Install the bridge binary

```bash
cargo install --path .
```

### 2. Install the Zed extension

In Zed: **Extensions → Install Dev Extension** → select the `zed-extension/` directory.

The extension registers the bridge as a language server. It auto-starts when any file is opened and stops when the workspace closes.

### 3. Configure keybinding and task

Copy the global Zed configuration:

```bash
# Task (sends selection to bridge)
cp zed/tasks.json ~/.config/zed/tasks.json

# Keybinding (Ctrl+Shift+S triggers the task)
cp zed/keymap.json ~/.config/zed/keymap.json
```

Or merge into your existing config files.

## Usage

1. Open a project in Zed — the bridge starts automatically
2. Start Claude Code in Zed's terminal: `claude`
3. Claude Code auto-discovers the bridge via lock file
4. Select code in the editor, press **Ctrl+Shift+S** to push it
5. Ask Claude Code about your selection — it uses `getCurrentSelection` under the hood

## MCP Tools

| Tool | Description |
|------|-------------|
| `getCurrentSelection` | Get the current text selection in the active editor |
| `getLatestSelection` | Get the most recent text selection (persists after deselect) |
| `getWorkspaceFolders` | Get all workspace folders open in the IDE |

## Development

```bash
cargo build          # Build
cargo test           # Run tests (6 unit + 13 integration)
cargo run -- /path   # Run standalone (without Zed extension)
```

## Limitations

| Feature | VS Code Extension | zed-claude-bridge |
|---------|-------------------|-------------------|
| Selection | Auto (real-time) | Manual (Ctrl+Shift+S) |
| Diagnostics | Auto (from LSP) | Not yet |
| Open editors | Auto | Not yet |
| Open/navigate file | Full API | Not yet |

## License

MIT
