use futures::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio_tungstenite::tungstenite::{client::IntoClientRequest, http::HeaderValue, Message};
use zed_claude_bridge::server::{start_server, ServerHandle};

fn http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .http1_only()
        .no_proxy()
        .build()
        .unwrap()
}

async fn start_bridge() -> ServerHandle {
    start_server(vec!["/tmp".into()]).await
}

async fn connect_ws(
    handle: &ServerHandle,
) -> tokio_tungstenite::WebSocketStream<
    tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
> {
    let url = format!("ws://127.0.0.1:{}/", handle.port);
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        "x-claude-code-ide-authorization",
        HeaderValue::from_str(&handle.auth_token).unwrap(),
    );
    let (ws, _) = tokio_tungstenite::connect_async(request).await.unwrap();
    ws
}

async fn mcp_init(
    ws: &mut (impl SinkExt<Message, Error = impl std::fmt::Debug>
              + StreamExt<Item = Result<Message, impl std::fmt::Debug>>
              + Unpin),
) {
    ws.send(Message::text(
        json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": { "name": "test", "version": "0.1" }
            }
        })
        .to_string(),
    ))
    .await
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    ws.send(Message::text(
        json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        })
        .to_string(),
    ))
    .await
    .unwrap();

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    // Drain init response messages
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(100), ws.next()).await {
            Ok(Some(Ok(Message::Text(_)))) => continue,
            _ => break,
        }
    }
}

async fn send_and_recv(
    ws: &mut (impl SinkExt<Message, Error = impl std::fmt::Debug>
              + StreamExt<Item = Result<Message, impl std::fmt::Debug>>
              + Unpin),
    msg: Value,
) -> Value {
    ws.send(Message::text(msg.to_string())).await.unwrap();
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(1000), ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v.get("id").is_some() {
                    return v;
                }
            }
            _ => panic!("no response received"),
        }
    }
}

async fn post_selection(handle: &ServerHandle, text: &str) {
    let res = http_client()
        .post(format!("http://127.0.0.1:{}/api/selection", handle.port))
        .header("x-claude-code-ide-authorization", &handle.auth_token)
        .json(&json!({
            "text": text,
            "filePath": "/tmp/test.ts",
            "row": 1,
            "column": 1,
            "language": "typescript",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

// --- Tests ---

#[tokio::test]
async fn test_post_selection_ok() {
    let handle = start_bridge().await;
    let res = http_client()
        .post(format!("http://127.0.0.1:{}/api/selection", handle.port))
        .header("x-claude-code-ide-authorization", &handle.auth_token)
        .json(&json!({
            "text": "hello",
            "filePath": "/tmp/test.ts",
            "row": 1,
            "column": 1,
            "language": "typescript",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
}

#[tokio::test]
async fn test_post_selection_no_auth() {
    let handle = start_bridge().await;
    let res = http_client()
        .post(format!("http://127.0.0.1:{}/api/selection", handle.port))
        .json(&json!({
            "text": "hello",
            "filePath": "/tmp/test.ts",
            "row": 1,
            "column": 1,
            "language": "typescript",
        }))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 401);
}

#[tokio::test]
async fn test_post_selection_bad_json() {
    let handle = start_bridge().await;
    let res = http_client()
        .post(format!("http://127.0.0.1:{}/api/selection", handle.port))
        .header("content-type", "application/json")
        .header("x-claude-code-ide-authorization", &handle.auth_token)
        .body("not json")
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 400);
}

#[tokio::test]
async fn test_unknown_route_404() {
    let handle = start_bridge().await;
    let res = http_client()
        .get(format!("http://127.0.0.1:{}/unknown", handle.port))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
}

#[tokio::test]
async fn test_ws_valid_auth() {
    let handle = start_bridge().await;
    let _ws = connect_ws(&handle).await;
}

#[tokio::test]
async fn test_ws_invalid_auth() {
    let handle = start_bridge().await;
    let url = format!("ws://127.0.0.1:{}/", handle.port);
    let mut request = url.into_client_request().unwrap();
    request.headers_mut().insert(
        "x-claude-code-ide-authorization",
        HeaderValue::from_str("wrong-token").unwrap(),
    );
    let result = tokio_tungstenite::connect_async(request).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_tools_list() {
    let handle = start_bridge().await;
    let mut ws = connect_ws(&handle).await;
    mcp_init(&mut ws).await;

    let resp = send_and_recv(
        &mut ws,
        json!({"jsonrpc": "2.0", "id": 2, "method": "tools/list"}),
    )
    .await;
    let tools = resp["result"]["tools"].as_array().unwrap();
    let names: Vec<&str> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(names.contains(&"getCurrentSelection"));
    assert!(names.contains(&"getLatestSelection"));
    assert!(names.contains(&"getWorkspaceFolders"));
}

#[tokio::test]
async fn test_get_current_selection() {
    let handle = start_bridge().await;
    post_selection(&handle, "const x = 1;").await;

    let mut ws = connect_ws(&handle).await;
    mcp_init(&mut ws).await;

    let resp = send_and_recv(
        &mut ws,
        json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "getCurrentSelection", "arguments": {} }
        }),
    )
    .await;
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(data["success"], true);
    assert_eq!(data["text"], "const x = 1;");
}

#[tokio::test]
async fn test_get_current_selection_empty() {
    let handle = start_bridge().await;
    let mut ws = connect_ws(&handle).await;
    mcp_init(&mut ws).await;

    let resp = send_and_recv(
        &mut ws,
        json!({
            "jsonrpc": "2.0", "id": 3, "method": "tools/call",
            "params": { "name": "getCurrentSelection", "arguments": {} }
        }),
    )
    .await;
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(data["success"], false);
}

#[tokio::test]
async fn test_get_workspace_folders() {
    let handle = start_bridge().await;
    let mut ws = connect_ws(&handle).await;
    mcp_init(&mut ws).await;

    let resp = send_and_recv(
        &mut ws,
        json!({
            "jsonrpc": "2.0", "id": 4, "method": "tools/call",
            "params": { "name": "getWorkspaceFolders", "arguments": {} }
        }),
    )
    .await;
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(data["success"], true);
    assert_eq!(data["folders"][0]["path"], "/tmp");
}

#[tokio::test]
async fn test_selection_changed_notification() {
    let handle = start_bridge().await;
    let mut ws = connect_ws(&handle).await;
    mcp_init(&mut ws).await;

    post_selection(&handle, "notify me").await;

    // Wait for debounce (50ms) + buffer
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;

    let mut found = false;
    loop {
        match tokio::time::timeout(std::time::Duration::from_millis(500), ws.next()).await {
            Ok(Some(Ok(Message::Text(t)))) => {
                let v: Value = serde_json::from_str(&t).unwrap();
                if v["method"] == "selection_changed" {
                    assert_eq!(v["params"]["text"], "notify me");
                    found = true;
                    break;
                }
            }
            _ => break,
        }
    }
    assert!(found, "selection_changed notification not received");
}

#[tokio::test]
async fn test_get_latest_selection() {
    let handle = start_bridge().await;
    post_selection(&handle, "latest text").await;

    let mut ws = connect_ws(&handle).await;
    mcp_init(&mut ws).await;

    let resp = send_and_recv(
        &mut ws,
        json!({
            "jsonrpc": "2.0", "id": 5, "method": "tools/call",
            "params": { "name": "getLatestSelection", "arguments": {} }
        }),
    )
    .await;
    let text = resp["result"]["content"][0]["text"].as_str().unwrap();
    let data: Value = serde_json::from_str(text).unwrap();
    assert_eq!(data["success"], true);
    assert_eq!(data["text"], "latest text");
}

#[tokio::test]
async fn test_call_unknown_tool() {
    let handle = start_bridge().await;
    let mut ws = connect_ws(&handle).await;
    mcp_init(&mut ws).await;

    let resp = send_and_recv(
        &mut ws,
        json!({
            "jsonrpc": "2.0", "id": 6, "method": "tools/call",
            "params": { "name": "nonExistentTool", "arguments": {} }
        }),
    )
    .await;
    assert!(resp.get("error").is_some(), "expected error for unknown tool");
}
