# Claude Code IDE MCP Protocol

Reverse-engineered from VS Code extension `anthropic.claude-code-2.1.81-darwin-arm64`.

## Overview

Claude Code CLI auto-discovers IDE integrations by reading lock files from `~/.claude/ide/`. The IDE extension runs a **WebSocket MCP server** on localhost, and the CLI connects to it on startup.

## Lock File

**Path**: `~/.claude/ide/{port}.lock`
**Permissions**: `0600` (file), `0700` (directory)

```json
{
  "pid": 82040,
  "workspaceFolders": ["/Users/user/project"],
  "ideName": "Visual Studio Code",
  "transport": "ws",
  "runningInWindows": false,
  "authToken": "bfcb364f-8900-41e5-aad3-f52a21bfd328"
}
```

| Field | Type | Description |
|-------|------|-------------|
| `pid` | number | Process ID of the IDE |
| `workspaceFolders` | string[] | Open workspace folder paths |
| `ideName` | string | IDE display name (e.g. `"Visual Studio Code"`, `"Zed"`) |
| `transport` | string | Always `"ws"` |
| `runningInWindows` | boolean | `false` on macOS/Linux |
| `authToken` | string | UUID v4, generated per session |

The lock file name is `{port}.lock` where `port` is the WebSocket server's listening port.

## WebSocket Connection

- **URL**: `ws://127.0.0.1:{port}`
- **Auth Header**: `x-claude-code-ide-authorization: {authToken}`
- **Protocol**: JSON-RPC 2.0 (MCP standard)

The CLI reads the lock file, extracts the port and auth token, then connects via WebSocket with the auth token in a custom header.

If the auth token doesn't match, the server responds with `close(1008, "Unauthorized")`.

## Environment Variable

After the MCP server starts, the extension sets:

```
CLAUDE_CODE_SSE_PORT={port}
```

This is set in the IDE's terminal environment so that Claude Code CLI instances spawned from the IDE terminal can discover the server.

## MCP Tools

The server implements the standard MCP protocol (JSON-RPC 2.0 over WebSocket). Below are the tools exposed to Claude Code.

### `getDiagnostics`

Get language-server diagnostics (errors, warnings) from the IDE.

**Input Schema**:
```json
{
  "type": "object",
  "properties": {
    "uri": {
      "type": "string",
      "description": "Optional file URI to get diagnostics for. If not provided, gets diagnostics for all files."
    }
  }
}
```

**Response** (in `content[0].text`, JSON-stringified):
```json
[
  {
    "uri": "file:///path/to/file.ts",
    "linesInFile": 150,
    "diagnostics": [
      {
        "message": "Property 'foo' does not exist on type 'Bar'",
        "severity": "Error",
        "range": {
          "start": { "line": 10, "character": 5 },
          "end": { "line": 10, "character": 8 }
        },
        "source": "ts",
        "code": "2339"
      }
    ]
  }
]
```

### `getCurrentSelection`

Get the current text selection in the active editor.

**Input Schema**: `{}` (no parameters)

**Response** (in `content[0].text`, JSON-stringified):

Success:
```json
{
  "success": true,
  "text": "selected text here",
  "filePath": "/absolute/path/to/file.ts",
  "fileUrl": "file:///absolute/path/to/file.ts",
  "selection": {
    "start": { "line": 5, "character": 0 },
    "end": { "line": 10, "character": 20 },
    "isEmpty": false
  }
}
```

No active editor:
```json
{
  "success": false,
  "message": "No active editor found"
}
```

### `getLatestSelection`

Get the most recent text selection, even if no editor is currently active.

**Input Schema**: `{}` (no parameters)

**Response**: Same format as `getCurrentSelection`. Returns `{ "success": false, "message": "No selection available" }` if no selection has been made.

### `getOpenEditors`

Get information about currently open editor tabs.

**Input Schema**: `{}` (no parameters)

**Response** (in `content[0].text`, JSON-stringified):
```json
{
  "tabs": [
    {
      "uri": "file:///path/to/file.ts",
      "isActive": true,
      "isPinned": false,
      "isPreview": false,
      "isDirty": false,
      "label": "file.ts",
      "groupIndex": 0,
      "viewColumn": 1,
      "isGroupActive": true,
      "fileName": "/path/to/file.ts",
      "languageId": "typescript",
      "lineCount": 150,
      "isUntitled": false,
      "selection": {
        "start": { "line": 5, "character": 0 },
        "end": { "line": 5, "character": 0 },
        "isReversed": false
      }
    }
  ]
}
```

### `getWorkspaceFolders`

Get all workspace folders currently open in the IDE.

**Input Schema**: `{}` (no parameters)

**Response** (in `content[0].text`, JSON-stringified):
```json
{
  "success": true,
  "folders": [
    {
      "name": "my-project",
      "uri": "file:///Users/user/my-project",
      "path": "/Users/user/my-project",
      "index": 0
    }
  ],
  "rootPath": "/Users/user/my-project",
  "workspaceFile": null
}
```

### `openFile`

Open a file in the editor and optionally select a range.

**Input Schema**:
```json
{
  "type": "object",
  "properties": {
    "filePath": { "type": "string", "description": "Path to the file to open" },
    "preview": { "type": "boolean", "default": false },
    "startText": { "type": "string", "description": "Text pattern to find start of selection" },
    "endText": { "type": "string", "description": "Text pattern to find end of selection" },
    "selectToEndOfLine": { "type": "boolean", "default": false },
    "makeFrontmost": { "type": "boolean", "default": true }
  },
  "required": ["filePath"]
}
```

### `checkDocumentDirty`

Check if a document has unsaved changes.

**Input Schema**:
```json
{
  "type": "object",
  "properties": {
    "filePath": { "type": "string", "description": "Path to the file to check" }
  },
  "required": ["filePath"]
}
```

### `saveDocument`

Save a document with unsaved changes.

**Input Schema**:
```json
{
  "type": "object",
  "properties": {
    "filePath": { "type": "string", "description": "Path to the file to save" }
  },
  "required": ["filePath"]
}
```

### `close_tab`

Close an editor tab by name.

**Input Schema**:
```json
{
  "type": "object",
  "properties": {
    "tab_name": { "type": "string" }
  },
  "required": ["tab_name"]
}
```

### `closeAllDiffTabs`

Close all diff tabs in the editor.

**Input Schema**: `{}` (no parameters)

### `executeCode`

Execute Python code in the Jupyter kernel for the current notebook. Requires user confirmation via dialog.

**Input Schema**:
```json
{
  "type": "object",
  "properties": {
    "code": { "type": "string", "description": "The code to be executed on the kernel." }
  },
  "required": ["code"]
}
```

## Server-to-Client Notifications (JSON-RPC)

These are pushed from the server to the CLI without being requested.

### `selection_changed`

Sent when the user changes their text selection in the editor.

```json
{
  "jsonrpc": "2.0",
  "method": "selection_changed",
  "params": {
    "text": "selected text",
    "filePath": "/path/to/file.ts",
    "fileUrl": "file:///path/to/file.ts",
    "selection": {
      "start": { "line": 5, "character": 0 },
      "end": { "line": 10, "character": 20 },
      "isEmpty": false
    }
  }
}
```

The extension debounces this notification (300ms delay).

### `diagnostics_changed`

Sent when language-server diagnostics change.

```json
{
  "jsonrpc": "2.0",
  "method": "diagnostics_changed",
  "params": {
    "uris": ["file:///path/to/file.ts"]
  }
}
```

### `at_mentioned`

Sent when the user triggers an @-mention action.

```json
{
  "jsonrpc": "2.0",
  "method": "at_mentioned",
  "params": {
    "filePath": "/path/to/file.ts",
    "lineStart": 5,
    "lineEnd": 10
  }
}
```

## Server Lifecycle

1. HTTP server created, listens on `127.0.0.1:{random_port}`
2. WebSocket server (`ws` library) wraps the HTTP server
3. Lock file written to `~/.claude/ide/{port}.lock`
4. Environment variable `CLAUDE_CODE_SSE_PORT` set in IDE terminal
5. On WebSocket connection:
   - Verify `x-claude-code-ide-authorization` header
   - Create MCP transport, connect to MCP server instance
   - Register diagnostic streaming client
   - Send initial selection state (after 500ms delay)
6. On WebSocket disconnect:
   - Unregister diagnostic client
   - Clean up transport
7. On IDE shutdown:
   - Remove lock file
   - Close WebSocket server

## MCP Protocol Messages

Standard MCP JSON-RPC 2.0 flow over WebSocket:

### Client → Server

```json
{"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "claude-code", "version": "2.1.81"}}}
```

```json
{"jsonrpc": "2.0", "method": "notifications/initialized"}
```

```json
{"jsonrpc": "2.0", "id": 2, "method": "tools/list"}
```

```json
{"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "getCurrentSelection", "arguments": {}}}
```

### Server → Client

```json
{"jsonrpc": "2.0", "id": 1, "result": {"protocolVersion": "2024-11-05", "capabilities": {"tools": {}}, "serverInfo": {"name": "claude-vscode-extension", "version": "2.1.81"}}}
```

```json
{"jsonrpc": "2.0", "id": 2, "result": {"tools": [{"name": "getDiagnostics", "description": "...", "inputSchema": {...}}, ...]}}
```
