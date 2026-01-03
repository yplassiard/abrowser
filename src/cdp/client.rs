// CDP WebSocket client

use futures_util::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio_tungstenite::{connect_async, tungstenite::Message};

#[derive(Debug)]
pub struct CdpClient {
    sender: mpsc::Sender<OutgoingMessage>,
    #[allow(dead_code)]
    pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>>,
    event_rx: Arc<Mutex<mpsc::Receiver<CdpEvent>>>,
    next_id: AtomicU64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CdpEvent {
    pub method: String,
    pub params: Value,
}

enum OutgoingMessage {
    Request {
        id: u64,
        method: String,
        params: Value,
        response_tx: oneshot::Sender<Value>,
    },
}

impl CdpClient {
    pub async fn connect(ws_url: &str) -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let url = url::Url::parse(ws_url)?;
        let (ws_stream, _) = connect_async(url).await?;
        let (mut write, mut read) = ws_stream.split();

        let pending: Arc<Mutex<HashMap<u64, oneshot::Sender<Value>>>> =
            Arc::new(Mutex::new(HashMap::new()));
        let pending_clone = pending.clone();

        let (tx, mut rx) = mpsc::channel::<OutgoingMessage>(100);
        let (event_tx, event_rx) = mpsc::channel::<CdpEvent>(100);

        // Spawn writer task
        tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                match msg {
                    OutgoingMessage::Request {
                        id,
                        method,
                        params,
                        response_tx,
                    } => {
                        let request = json!({
                            "id": id,
                            "method": method,
                            "params": params
                        });
                        pending_clone.lock().await.insert(id, response_tx);
                        if write
                            .send(Message::Text(request.to_string()))
                            .await
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            }
        });

        // Spawn reader task
        let pending_reader = pending.clone();
        tokio::spawn(async move {
            while let Some(msg) = read.next().await {
                if let Ok(Message::Text(text)) = msg {
                    if let Ok(value) = serde_json::from_str::<Value>(&text) {
                        // Check if it's a response (has id) or event (has method)
                        if let Some(id) = value.get("id").and_then(|v| v.as_u64()) {
                            // Response
                            let mut pending = pending_reader.lock().await;
                            if let Some(tx) = pending.remove(&id) {
                                let result = value.get("result").cloned().unwrap_or(Value::Null);
                                let _ = tx.send(result);
                            }
                        } else if let Some(method) = value.get("method").and_then(|v| v.as_str()) {
                            // Event
                            let event = CdpEvent {
                                method: method.to_string(),
                                params: value.get("params").cloned().unwrap_or(Value::Null),
                            };
                            let _ = event_tx.send(event).await;
                        }
                    }
                }
            }
        });

        Ok(Self {
            sender: tx,
            pending,
            event_rx: Arc::new(Mutex::new(event_rx)),
            next_id: AtomicU64::new(1),
        })
    }

    pub async fn call(
        &self,
        method: &str,
        params: Value,
    ) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
        let id = self.next_id.fetch_add(1, Ordering::SeqCst);
        let (response_tx, response_rx) = oneshot::channel();

        self.sender
            .send(OutgoingMessage::Request {
                id,
                method: method.to_string(),
                params,
                response_tx,
            })
            .await?;

        let result = response_rx.await?;
        Ok(result)
    }

    /// Send multiple CDP calls in parallel and wait for all responses
    pub async fn call_batch(
        &self,
        calls: Vec<(&str, Value)>,
    ) -> Result<Vec<Value>, Box<dyn std::error::Error + Send + Sync>> {
        let futures: Vec<_> = calls
            .into_iter()
            .map(|(method, params)| self.call(method, params))
            .collect();

        let results = futures_util::future::join_all(futures).await;
        results.into_iter().collect()
    }

    pub async fn recv_event(&self) -> Option<CdpEvent> {
        self.event_rx.lock().await.recv().await
    }

    /// Try to receive an event without blocking
    pub fn try_recv_event(&self) -> Option<CdpEvent> {
        if let Ok(mut rx) = self.event_rx.try_lock() {
            rx.try_recv().ok()
        } else {
            None
        }
    }
}
