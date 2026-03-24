use axum::extract::ws::{Message, WebSocket};
use futures::stream::{SplitSink, SplitStream};
use futures::{SinkExt, StreamExt};
use rmcp::service::{RxJsonRpcMessage, TxJsonRpcMessage};
use rmcp::transport::Transport;
use rmcp::RoleServer;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Adapter that bridges an axum WebSocket to the rmcp Transport trait.
pub struct WsTransport {
    sink: Arc<Mutex<SplitSink<WebSocket, Message>>>,
    stream: SplitStream<WebSocket>,
}

impl WsTransport {
    pub fn new(ws: WebSocket) -> Self {
        let (sink, stream) = ws.split();
        let sink = Arc::new(Mutex::new(sink));
        Self { sink, stream }
    }
}

impl Transport<RoleServer> for WsTransport {
    type Error = axum::Error;

    fn send(
        &mut self,
        item: TxJsonRpcMessage<RoleServer>,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send + 'static {
        let sink = self.sink.clone();
        async move {
            let json = serde_json::to_string(&item).map_err(axum::Error::new)?;
            let mut sink = sink.lock().await;
            sink.send(Message::text(json)).await.map_err(axum::Error::new)
        }
    }

    fn receive(&mut self) -> impl Future<Output = Option<RxJsonRpcMessage<RoleServer>>> + Send {
        async {
            loop {
                match self.stream.next().await {
                    Some(Ok(Message::Text(text))) => {
                        match serde_json::from_str::<RxJsonRpcMessage<RoleServer>>(&text) {
                            Ok(msg) => return Some(msg),
                            Err(e) => {
                                tracing::warn!("failed to parse JSON-RPC message: {e}");
                                continue;
                            }
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => return None,
                    Some(Ok(_)) => continue, // skip binary/ping/pong
                    Some(Err(e)) => {
                        tracing::warn!("WebSocket error: {e}");
                        return None;
                    }
                }
            }
        }
    }

    fn close(&mut self) -> impl Future<Output = Result<(), Self::Error>> + Send {
        let sink = self.sink.clone();
        async move {
            let mut sink = sink.lock().await;
            sink.close().await.map_err(axum::Error::new)
        }
    }
}
