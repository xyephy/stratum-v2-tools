use axum::{
    extract::{WebSocketUpgrade, State},
    response::Response,
};
use axum::extract::ws::{WebSocket, Message};
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{info, warn, error};
use uuid::Uuid;
use sv2_core::{
    DaemonStatus, ConnectionInfo, Share, PerformanceMetrics, Alert,
    types::MiningStats,
};
use crate::handlers::AppState;

/// WebSocket message types for real-time communication
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum WebSocketMessage {
    /// System status update
    Status(DaemonStatus),
    /// New connection event
    ConnectionAdded(ConnectionInfo),
    /// Connection updated event
    ConnectionUpdated(ConnectionInfo),
    /// Connection removed event
    ConnectionRemoved { id: Uuid },
    /// New share submitted
    ShareSubmitted(Share),
    /// Performance metrics update
    MetricsUpdate(PerformanceMetrics),
    /// Mining statistics update
    MiningStatsUpdate(MiningStats),
    /// New alert
    AlertCreated(Alert),
    /// Alert resolved
    AlertResolved { id: Uuid },
    /// Heartbeat/keepalive
    Heartbeat { timestamp: chrono::DateTime<chrono::Utc> },
    /// Error message
    Error { message: String },
    /// Subscription confirmation
    Subscribed { subscriptions: Vec<String> },
}

/// WebSocket subscription request
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "action")]
pub enum WebSocketRequest {
    /// Subscribe to specific event types
    Subscribe { events: Vec<String> },
    /// Unsubscribe from event types
    Unsubscribe { events: Vec<String> },
    /// Get current status
    GetStatus,
    /// Ping for connection testing
    Ping,
}

/// WebSocket client session
#[derive(Debug, Clone)]
pub struct WebSocketSession {
    pub id: Uuid,
    pub subscriptions: std::collections::HashSet<String>,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
}

impl WebSocketSession {
    pub fn new() -> Self {
        let now = chrono::Utc::now();
        Self {
            id: Uuid::new_v4(),
            subscriptions: std::collections::HashSet::new(),
            connected_at: now,
            last_activity: now,
        }
    }

    pub fn is_subscribed(&self, event_type: &str) -> bool {
        self.subscriptions.contains(event_type) || self.subscriptions.contains("*")
    }

    pub fn update_activity(&mut self) {
        self.last_activity = chrono::Utc::now();
    }
}

/// WebSocket event broadcaster
pub struct WebSocketBroadcaster {
    sender: broadcast::Sender<WebSocketMessage>,
}

impl WebSocketBroadcaster {
    pub fn new() -> Self {
        let (sender, _) = broadcast::channel(1000);
        Self { sender }
    }

    pub fn subscribe(&self) -> broadcast::Receiver<WebSocketMessage> {
        self.sender.subscribe()
    }

    pub fn broadcast(&self, message: WebSocketMessage) {
        if let Err(e) = self.sender.send(message) {
            warn!("Failed to broadcast WebSocket message: {}", e);
        }
    }

    pub fn broadcast_status(&self, status: DaemonStatus) {
        self.broadcast(WebSocketMessage::Status(status));
    }

    pub fn broadcast_connection_added(&self, connection: ConnectionInfo) {
        self.broadcast(WebSocketMessage::ConnectionAdded(connection));
    }

    pub fn broadcast_connection_updated(&self, connection: ConnectionInfo) {
        self.broadcast(WebSocketMessage::ConnectionUpdated(connection));
    }

    pub fn broadcast_connection_removed(&self, id: Uuid) {
        self.broadcast(WebSocketMessage::ConnectionRemoved { id });
    }

    pub fn broadcast_share_submitted(&self, share: Share) {
        self.broadcast(WebSocketMessage::ShareSubmitted(share));
    }

    pub fn broadcast_metrics_update(&self, metrics: PerformanceMetrics) {
        self.broadcast(WebSocketMessage::MetricsUpdate(metrics));
    }

    pub fn broadcast_mining_stats(&self, stats: MiningStats) {
        self.broadcast(WebSocketMessage::MiningStatsUpdate(stats));
    }

    pub fn broadcast_alert_created(&self, alert: Alert) {
        self.broadcast(WebSocketMessage::AlertCreated(alert));
    }

    pub fn broadcast_alert_resolved(&self, id: Uuid) {
        self.broadcast(WebSocketMessage::AlertResolved { id });
    }
}

/// Handle WebSocket connections for real-time updates
pub async fn websocket_handler(
    ws: WebSocketUpgrade,
    State(state): State<AppState>,
) -> Response {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

async fn handle_socket(socket: WebSocket, state: AppState) {
    let session = Arc::new(tokio::sync::RwLock::new(WebSocketSession::new()));
    let session_id = session.read().await.id;
    info!("WebSocket client connected: {}", session_id);

    // Create a broadcaster for this connection (in a real implementation, this would be shared)
    let broadcaster = Arc::new(WebSocketBroadcaster::new());
    let mut receiver = broadcaster.subscribe();

    // Split the socket into sender and receiver
    let (sender, mut receiver_ws) = socket.split();
    let sender = Arc::new(tokio::sync::Mutex::new(sender));

    // Start periodic status updates
    let broadcaster_clone = broadcaster.clone();
    let state_clone = state.clone();
    let status_task = tokio::spawn(async move {
        let mut interval = interval(Duration::from_secs(5));
        loop {
            interval.tick().await;
            
            // Get current status and broadcast it
            match get_current_status(&state_clone).await {
                Ok(status) => broadcaster_clone.broadcast_status(status),
                Err(e) => {
                    error!("Failed to get status for WebSocket broadcast: {}", e);
                    broadcaster_clone.broadcast(WebSocketMessage::Error {
                        message: format!("Failed to get status: {}", e),
                    });
                }
            }

            // Send heartbeat
            broadcaster_clone.broadcast(WebSocketMessage::Heartbeat {
                timestamp: chrono::Utc::now(),
            });
        }
    });

    // Handle incoming messages from client
    let broadcaster_clone = broadcaster.clone();
    let session_clone = session.clone();
    let sender_clone = sender.clone();
    let incoming_task = tokio::spawn(async move {
        while let Some(msg) = receiver_ws.next().await {
            match msg {
                Ok(Message::Text(text)) => {
                    {
                        let mut session_guard = session_clone.write().await;
                        session_guard.update_activity();
                    }
                    
                    match serde_json::from_str::<WebSocketRequest>(&text) {
                        Ok(request) => {
                            let mut session_guard = session_clone.write().await;
                            handle_websocket_request(request, &mut session_guard, &broadcaster_clone).await;
                        }
                        Err(e) => {
                            warn!("Invalid WebSocket request: {}", e);
                            broadcaster_clone.broadcast(WebSocketMessage::Error {
                                message: format!("Invalid request format: {}", e),
                            });
                        }
                    }
                }
                Ok(Message::Binary(_)) => {
                    // Binary messages not supported for now
                    broadcaster_clone.broadcast(WebSocketMessage::Error {
                        message: "Binary messages not supported".to_string(),
                    });
                }
                Ok(Message::Ping(data)) => {
                    // Respond to ping with pong
                    let mut sender_guard = sender_clone.lock().await;
                    if let Err(e) = sender_guard.send(Message::Pong(data)).await {
                        error!("Failed to send pong: {}", e);
                        break;
                    }
                }
                Ok(Message::Pong(_)) => {
                    // Pong received, update activity
                    let mut session_guard = session_clone.write().await;
                    session_guard.update_activity();
                }
                Ok(Message::Close(_)) => {
                    info!("WebSocket client disconnected: {}", session_id);
                    break;
                }
                Err(e) => {
                    error!("WebSocket error: {}", e);
                    break;
                }
            }
        }
    });

    // Handle outgoing messages to client
    let session_clone = session.clone();
    let sender_clone = sender.clone();
    let outgoing_task = tokio::spawn(async move {
        while let Ok(message) = receiver.recv().await {
            // Check if client is subscribed to this message type
            let message_type = match &message {
                WebSocketMessage::Status(_) => "status",
                WebSocketMessage::ConnectionAdded(_) => "connection",
                WebSocketMessage::ConnectionUpdated(_) => "connection",
                WebSocketMessage::ConnectionRemoved { .. } => "connection",
                WebSocketMessage::ShareSubmitted(_) => "share",
                WebSocketMessage::MetricsUpdate(_) => "metrics",
                WebSocketMessage::MiningStatsUpdate(_) => "mining_stats",
                WebSocketMessage::AlertCreated(_) => "alert",
                WebSocketMessage::AlertResolved { .. } => "alert",
                WebSocketMessage::Heartbeat { .. } => "heartbeat",
                WebSocketMessage::Error { .. } => "error",
                WebSocketMessage::Subscribed { .. } => "system",
            };

            let is_subscribed = {
                let session_guard = session_clone.read().await;
                session_guard.is_subscribed(message_type)
            };

            if is_subscribed {
                let json = match serde_json::to_string(&message) {
                    Ok(json) => json,
                    Err(e) => {
                        error!("Failed to serialize WebSocket message: {}", e);
                        continue;
                    }
                };

                let mut sender_guard = sender_clone.lock().await;
                if let Err(e) = sender_guard.send(Message::Text(json)).await {
                    error!("Failed to send WebSocket message: {}", e);
                    break;
                }
            }
        }
    });

    // Wait for any task to complete (indicating connection closed or error)
    tokio::select! {
        _ = status_task => {},
        _ = incoming_task => {},
        _ = outgoing_task => {},
    }

    info!("WebSocket connection closed: {}", session_id);
}

async fn handle_websocket_request(
    request: WebSocketRequest,
    session: &mut WebSocketSession,
    broadcaster: &WebSocketBroadcaster,
) {
    match request {
        WebSocketRequest::Subscribe { events } => {
            for event in &events {
                session.subscriptions.insert(event.clone());
            }
            broadcaster.broadcast(WebSocketMessage::Subscribed {
                subscriptions: session.subscriptions.iter().cloned().collect(),
            });
            info!("Client {} subscribed to events: {:?}", session.id, events);
        }
        WebSocketRequest::Unsubscribe { events } => {
            for event in &events {
                session.subscriptions.remove(event);
            }
            broadcaster.broadcast(WebSocketMessage::Subscribed {
                subscriptions: session.subscriptions.iter().cloned().collect(),
            });
            info!("Client {} unsubscribed from events: {:?}", session.id, events);
        }
        WebSocketRequest::GetStatus => {
            // This would typically get status from the daemon
            // For now, we'll send a mock status
            let status = DaemonStatus {
                running: true,
                uptime: std::time::Duration::from_secs(3600),
                active_connections: 0,
                total_connections: 0,
                mode: "Solo".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                total_shares: 0,
                valid_shares: 0,
                blocks_found: 0,
                current_difficulty: 1.0,
                hashrate: 0.0,
            };
            broadcaster.broadcast_status(status);
        }
        WebSocketRequest::Ping => {
            broadcaster.broadcast(WebSocketMessage::Heartbeat {
                timestamp: chrono::Utc::now(),
            });
        }
    }
}

async fn get_current_status(state: &AppState) -> Result<DaemonStatus, Box<dyn std::error::Error + Send + Sync>> {
    let share_stats = state.database.get_share_stats(None).await?;
    let connections = state.database.list_connections(None).await.unwrap_or_default();
    
    Ok(DaemonStatus {
        running: true,
        uptime: std::time::Duration::from_secs(3600), // Mock uptime
        active_connections: connections.len() as u64,
        total_connections: connections.len() as u64,
        mode: "Solo".to_string(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        total_shares: share_stats.total_shares,
        valid_shares: share_stats.valid_shares,
        blocks_found: share_stats.blocks_found,
        current_difficulty: 1.0,
        hashrate: share_stats.total_shares as f64 * 1e9, // Mock calculation
    })
}

/// Create a global WebSocket broadcaster that can be shared across the application
pub fn create_global_broadcaster() -> Arc<WebSocketBroadcaster> {
    Arc::new(WebSocketBroadcaster::new())
}

/// Example usage functions for broadcasting events from other parts of the application
impl WebSocketBroadcaster {
    /// Broadcast a new connection event
    pub fn notify_connection_added(&self, connection: ConnectionInfo) {
        self.broadcast_connection_added(connection);
    }

    /// Broadcast a connection update event
    pub fn notify_connection_updated(&self, connection: ConnectionInfo) {
        self.broadcast_connection_updated(connection);
    }

    /// Broadcast a connection removal event
    pub fn notify_connection_removed(&self, id: Uuid) {
        self.broadcast_connection_removed(id);
    }

    /// Broadcast a new share submission
    pub fn notify_share_submitted(&self, share: Share) {
        self.broadcast_share_submitted(share);
    }

    /// Broadcast performance metrics update
    pub fn notify_metrics_update(&self, metrics: PerformanceMetrics) {
        self.broadcast_metrics_update(metrics);
    }

    /// Broadcast mining statistics update
    pub fn notify_mining_stats_update(&self, stats: MiningStats) {
        self.broadcast_mining_stats(stats);
    }

    /// Broadcast a new alert
    pub fn notify_alert_created(&self, alert: Alert) {
        self.broadcast_alert_created(alert);
    }

    /// Broadcast alert resolution
    pub fn notify_alert_resolved(&self, id: Uuid) {
        self.broadcast_alert_resolved(id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_websocket_session_creation() {
        let session = WebSocketSession::new();
        assert!(session.subscriptions.is_empty());
        assert!(session.connected_at <= chrono::Utc::now());
    }

    #[test]
    fn test_websocket_session_subscriptions() {
        let mut session = WebSocketSession::new();
        
        // Initially not subscribed to anything
        assert!(!session.is_subscribed("status"));
        
        // Subscribe to specific event
        session.subscriptions.insert("status".to_string());
        assert!(session.is_subscribed("status"));
        assert!(!session.is_subscribed("connection"));
        
        // Subscribe to all events
        session.subscriptions.insert("*".to_string());
        assert!(session.is_subscribed("connection"));
        assert!(session.is_subscribed("share"));
    }

    #[tokio::test]
    async fn test_websocket_broadcaster() {
        let broadcaster = WebSocketBroadcaster::new();
        let mut receiver = broadcaster.subscribe();
        
        // Send a test message
        let test_status = DaemonStatus {
            running: true,
            uptime: std::time::Duration::from_secs(100),
            active_connections: 5,
            total_connections: 5,
            mode: "Solo".to_string(),
            version: "0.1.0".to_string(),
            total_shares: 1000,
            valid_shares: 950,
            blocks_found: 1,
            current_difficulty: 1.0,
            hashrate: 1e12,
        };
        
        broadcaster.broadcast_status(test_status.clone());
        
        // Receive the message
        let received = receiver.recv().await.unwrap();
        match received {
            WebSocketMessage::Status(status) => {
                assert_eq!(status.connections, test_status.connections);
                assert_eq!(status.total_shares, test_status.total_shares);
            }
            _ => panic!("Expected status message"),
        }
    }

    #[test]
    fn test_websocket_message_serialization() {
        let status = DaemonStatus {
            running: true,
            uptime: std::time::Duration::from_secs(100),
            active_connections: 5,
            total_connections: 5,
            mode: "Solo".to_string(),
            version: "0.1.0".to_string(),
            total_shares: 1000,
            valid_shares: 950,
            blocks_found: 1,
            current_difficulty: 1.0,
            hashrate: 1e12,
        };
        
        let message = WebSocketMessage::Status(status);
        let json = serde_json::to_string(&message).unwrap();
        let deserialized: WebSocketMessage = serde_json::from_str(&json).unwrap();
        
        match deserialized {
            WebSocketMessage::Status(s) => {
                assert_eq!(s.connections, 5);
                assert_eq!(s.total_shares, 1000);
            }
            _ => panic!("Expected status message"),
        }
    }
}