use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

use serde_json::Value;

/// Read one LSP message from stdin (Content-Length framing).
async fn read_message(reader: &mut (impl AsyncBufReadExt + Unpin)) -> Option<Value> {
    let mut content_length: Option<usize> = None;
    let mut header_line = String::new();

    // Read headers
    loop {
        header_line.clear();
        let n = reader.read_line(&mut header_line).await.ok()?;
        if n == 0 {
            return None; // EOF
        }
        let trimmed = header_line.trim();
        if trimmed.is_empty() {
            break; // End of headers
        }
        if let Some(val) = trimmed.strip_prefix("Content-Length:") {
            content_length = val.trim().parse().ok();
        }
    }

    let len = content_length?;
    let mut body = vec![0u8; len];
    reader.read_exact(&mut body).await.ok()?;
    serde_json::from_slice(&body).ok()
}

/// Write one LSP message to stdout (Content-Length framing).
async fn write_message(writer: &mut (impl AsyncWriteExt + Unpin), msg: &Value) -> bool {
    let body = serde_json::to_string(msg).unwrap();
    let header = format!("Content-Length: {}\r\n\r\n", body.len());
    if writer.write_all(header.as_bytes()).await.is_err() {
        return false;
    }
    if writer.write_all(body.as_bytes()).await.is_err() {
        return false;
    }
    writer.flush().await.is_ok()
}

/// Run a minimal LSP server on stdio. Returns when `exit` notification or EOF.
pub async fn run_stdio_lsp() {
    let stdin = tokio::io::stdin();
    let mut reader = BufReader::new(stdin);
    let mut writer = tokio::io::stdout();

    loop {
        let msg = match read_message(&mut reader).await {
            Some(msg) => msg,
            None => break,
        };

        let method = msg.get("method").and_then(|m| m.as_str()).unwrap_or("");
        let id = msg.get("id");

        match method {
            "initialize" => {
                if let Some(id) = id {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "capabilities": {},
                            "serverInfo": {
                                "name": "zed-claude-bridge",
                                "version": "0.1.0"
                            }
                        }
                    });
                    if !write_message(&mut writer, &resp).await {
                        break;
                    }
                }
            }
            "shutdown" => {
                if let Some(id) = id {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": null
                    });
                    if !write_message(&mut writer, &resp).await {
                        break;
                    }
                }
            }
            "exit" => break,
            _ => {
                if let Some(id) = id {
                    let resp = serde_json::json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {
                            "code": -32601,
                            "message": "method not found"
                        }
                    });
                    if !write_message(&mut writer, &resp).await {
                        break;
                    }
                }
            }
        }
    }
}
