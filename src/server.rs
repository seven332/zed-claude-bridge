use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::ws::WebSocket;
use axum::extract::{State, WebSocketUpgrade};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use rmcp::model::{CustomNotification, ServerNotification};
use rmcp::service::Peer;
use rmcp::RoleServer;
use tokio::net::TcpListener;

use crate::state::EditorState;
use crate::tools::BridgeHandler;
use crate::transport::WsTransport;
use crate::types::{SelectionInput, SelectionState};

#[derive(Clone)]
struct AppState {
    auth_token: String,
    editor_state: Arc<EditorState>,
}

#[allow(dead_code)]
pub struct ServerHandle {
    pub port: u16,
    pub auth_token: String,
    pub state: Arc<EditorState>,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
}

#[allow(dead_code)]
impl ServerHandle {
    pub fn close(self) {
        let _ = self.shutdown_tx.send(());
    }
}

pub async fn start_server(workspace_folders: Vec<String>) -> ServerHandle {
    let auth_token = uuid::Uuid::new_v4().to_string();
    let editor_state = Arc::new(EditorState::new(workspace_folders));

    let app_state = AppState {
        auth_token: auth_token.clone(),
        editor_state: editor_state.clone(),
    };

    let app = Router::new()
        .route("/api/selection", post(handle_selection))
        .route("/", get(handle_ws_upgrade))
        .with_state(app_state);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let port = listener.local_addr().unwrap().port();

    let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel::<()>();

    tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async {
                let _ = shutdown_rx.await;
            })
            .await
            .unwrap();
    });

    ServerHandle {
        port,
        auth_token,
        state: editor_state,
        shutdown_tx,
    }
}

async fn handle_selection(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let token = headers
        .get("x-claude-code-ide-authorization")
        .and_then(|v| v.to_str().ok());
    if token != Some(&state.auth_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }
    let input: SelectionInput = match serde_json::from_slice(&body) {
        Ok(v) => v,
        Err(_) => return StatusCode::BAD_REQUEST.into_response(),
    };
    state.editor_state.update_selection(input).await;
    (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
}

async fn handle_ws_upgrade(
    State(state): State<AppState>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> impl IntoResponse {
    let token = headers
        .get("x-claude-code-ide-authorization")
        .and_then(|v| v.to_str().ok());

    if token != Some(&state.auth_token) {
        return StatusCode::UNAUTHORIZED.into_response();
    }

    let editor_state = state.editor_state.clone();
    ws.on_upgrade(move |socket| handle_ws_connection(socket, editor_state))
        .into_response()
}

async fn handle_ws_connection(ws: WebSocket, editor_state: Arc<EditorState>) {
    let transport = WsTransport::new(ws);
    let handler = BridgeHandler {
        state: editor_state.clone(),
    };

    let running = match rmcp::serve_server(handler, transport).await {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("MCP server init failed: {e}");
            return;
        }
    };

    let peer = running.peer().clone();

    // Send initial selection after 500ms
    let peer_clone = peer.clone();
    let state_clone = editor_state.clone();
    let init_task = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        if let Some(sel) = state_clone.current_selection.read().await.as_ref() {
            let _ = send_selection_via_peer(&peer_clone, sel).await;
        }
    });

    // Push selection_changed notifications via MCP peer
    let mut rx = editor_state.selection_tx.subscribe();
    let notification_task = tokio::spawn(async move {
        loop {
            match rx.recv().await {
                Ok(sel) => {
                    // Debounce 50ms
                    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
                    let mut latest = sel;
                    while let Ok(newer) = rx.try_recv() {
                        latest = newer;
                    }
                    if send_selection_via_peer(&peer, &latest).await.is_err() {
                        break;
                    }
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(_) => break,
            }
        }
    });

    // Wait for MCP session to end
    let _ = running.waiting().await;
    init_task.abort();
    notification_task.abort();
}

fn selection_params(sel: &SelectionState) -> serde_json::Value {
    serde_json::json!({
        "text": sel.text,
        "filePath": sel.file_path,
        "fileUrl": sel.file_url,
        "selection": sel.selection,
    })
}

async fn send_selection_via_peer(
    peer: &Peer<RoleServer>,
    sel: &SelectionState,
) -> Result<(), rmcp::ServiceError> {
    peer.send_notification(ServerNotification::CustomNotification(
        CustomNotification::new("selection_changed", Some(selection_params(sel))),
    ))
    .await
}
