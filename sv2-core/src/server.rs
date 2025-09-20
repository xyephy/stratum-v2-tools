// TCP server implementation for Stratum connections
use crate::{
    error::{Error, Result},
    protocol::{NetworkProtocolMessage, StratumMessage},
    types::{Connection, ConnectionId, Protocol},
};
use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::{Arc, atomic::{AtomicU64, Ordering}},
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    sync::{mpsc, RwLock},
    time::{timeout, Duration},
};
use tracing::{info, warn, error, debug};
use uuid::Uuid;

/// Connection handler for individual client connections
pub struct ConnectionHandler {
    connection_id: ConnectionId,
    stream: TcpStream,
    peer_addr: SocketAddr,
    protocol: Protocol,
    message_tx: mpsc::UnboundedSender<NetworkProtocolMessage>,
    shutdown_rx: mpsc::Receiver<()>,
}

impl ConnectionHandler {
    pub fn new(
        connection_id: ConnectionId,
        stream: TcpStream,
        peer_addr: SocketAddr,
        message_tx: mpsc::UnboundedSender<NetworkProtocolMessage>,
        shutdown_rx: mpsc::Receiver<()>,
    ) -> Self {
        Self {
            connection_id,
            stream,
            peer_addr,
            protocol: Protocol::StratumV1, // Default to V1, detect later
            message_tx,
            shutdown_rx,
        }
    }

    /// Handle the connection lifecycle
    pub async fn handle(self) -> Result<()> {
        info!("Handling connection from {}: {}", self.peer_addr, self.connection_id);
        
        let (mut reader, mut writer) = self.stream.into_split();
        let mut buffer = vec![0u8; 4096];
        let mut message_buffer = String::new();
        let mut shutdown_rx = self.shutdown_rx;
        let connection_id = self.connection_id;
        let message_tx = self.message_tx;
        let mut protocol = self.protocol;

        loop {
            tokio::select! {
                // Handle incoming data
                result = reader.read(&mut buffer) => {
                    match result {
                        Ok(0) => {
                            debug!("Connection closed by peer: {}", connection_id);
                            break;
                        }
                        Ok(n) => {
                            let data = String::from_utf8_lossy(&buffer[..n]);
                            message_buffer.push_str(&data);
                            
                            // Process complete messages (newline-delimited JSON)
                            while let Some(newline_pos) = message_buffer.find('\n') {
                                let message_str = message_buffer[..newline_pos].trim().to_string();
                                message_buffer.drain(..=newline_pos);
                                
                                if !message_str.is_empty() {
                                    match Self::process_message(
                                        &message_str, 
                                        &mut writer, 
                                        connection_id, 
                                        &message_tx, 
                                        &mut protocol
                                    ).await {
                                        Ok(()) => {
                                            debug!("Successfully processed message from {}", connection_id);
                                        }
                                        Err(e) => {
                                            error!("Error processing message from {}: {}", connection_id, e);
                                            // Send error response but continue handling connection
                                            let error_response = serde_json::json!({
                                                "id": null,
                                                "result": null,
                                                "error": {"code": -32700, "message": "Parse error"}
                                            });
                                            if let Err(send_err) = Self::send_response(&mut writer, &error_response.to_string()).await {
                                                error!("Failed to send error response: {}", send_err);
                                                break; // Break if we can't send responses
                                            }
                                        }
                                    }
                                }
                            }
                        }
                        Err(e) => {
                            error!("Error reading from connection {}: {}", connection_id, e);
                            break;
                        }
                    }
                }
                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Shutting down connection: {}", connection_id);
                    break;
                }
            }
        }

        Ok(())
    }

    /// Process a single message from the client
    async fn process_message(
        message_str: &str, 
        writer: &mut tokio::net::tcp::OwnedWriteHalf,
        connection_id: ConnectionId,
        message_tx: &mpsc::UnboundedSender<NetworkProtocolMessage>,
        protocol: &mut Protocol,
    ) -> Result<()> {
        debug!("Received message from {}: {}", connection_id, message_str);
        
        // Parse JSON message
        let json_value: serde_json::Value = serde_json::from_str(message_str)
            .map_err(|e| Error::Protocol(format!("Invalid JSON: {}", e)))?;

        // Detect protocol version based on message structure
        if json_value.get("method").is_some() {
            *protocol = Protocol::StratumV1;
        } else if json_value.get("msg_type").is_some() {
            *protocol = Protocol::StratumV2;
        }

        // Handle immediate responses for some messages
        if let Some(method) = json_value.get("method").and_then(|m| m.as_str()) {
            let id = json_value.get("id");
            
            match method {
                "mining.subscribe" => {
                    let response = serde_json::json!({
                        "id": id,
                        "result": [
                            [["mining.set_difficulty", "1"], ["mining.notify", "1"]],
                            "00000000",
                            4
                        ],
                        "error": null
                    });
                    Self::send_response(writer, &response.to_string()).await?;
                }
                "mining.authorize" => {
                    let response = serde_json::json!({
                        "id": id,
                        "result": true,
                        "error": null
                    });
                    Self::send_response(writer, &response.to_string()).await?;
                }
                "mining.submit" => {
                    // For now, accept all shares - the mode handler will do proper validation
                    let response = serde_json::json!({
                        "id": id,
                        "result": true,
                        "error": null
                    });
                    Self::send_response(writer, &response.to_string()).await?;
                }
                _ => {
                    // Unknown method
                    let response = serde_json::json!({
                        "id": id,
                        "result": null,
                        "error": {"code": -1, "message": "Unknown method"}
                    });
                    Self::send_response(writer, &response.to_string()).await?;
                }
            }
        }

        // Create protocol message for forwarding to mode handler
        let protocol_msg = match *protocol {
            Protocol::StratumV1 | Protocol::Sv1 => {
                let stratum_msg = StratumMessage::from_json(&json_value)?;
                NetworkProtocolMessage::StratumV1 {
                    connection_id,
                    message: stratum_msg,
                }
            }
            Protocol::StratumV2 | Protocol::Sv2 => {
                // For now, treat as raw message
                NetworkProtocolMessage::StratumV2 {
                    connection_id,
                    data: message_str.as_bytes().to_vec(),
                }
            }
        };

        // Send to message handler
        message_tx.send(protocol_msg)
            .map_err(|e| Error::Internal(format!("Failed to send message: {}", e)))?;

        Ok(())
    }

    /// Send a response back to the client
    async fn send_response(writer: &mut tokio::net::tcp::OwnedWriteHalf, response: &str) -> Result<()> {
        let response_with_newline = format!("{}\n", response);
        writer.write_all(response_with_newline.as_bytes()).await
            .map_err(|e| Error::Network(format!("Failed to send response: {}", e)))?;
        writer.flush().await
            .map_err(|e| Error::Network(format!("Failed to flush response: {}", e)))?;
        Ok(())
    }
}

/// TCP server for handling Stratum connections
pub struct StratumServer {
    bind_address: SocketAddr,
    connections: Arc<RwLock<HashMap<ConnectionId, mpsc::UnboundedSender<String>>>>,
    connection_counter: AtomicU64,
    message_tx: mpsc::UnboundedSender<NetworkProtocolMessage>,
    shutdown_tx: mpsc::Sender<()>,
    shutdown_rx: Option<mpsc::Receiver<()>>,
}

impl StratumServer {
    pub fn new(
        bind_address: SocketAddr,
        message_tx: mpsc::UnboundedSender<NetworkProtocolMessage>,
    ) -> Self {
        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);
        
        Self {
            bind_address,
            connections: Arc::new(RwLock::new(HashMap::new())),
            connection_counter: AtomicU64::new(0),
            message_tx,
            shutdown_tx,
            shutdown_rx: Some(shutdown_rx),
        }
    }

    /// Start the server
    pub async fn start(&mut self) -> Result<()> {
        let listener = TcpListener::bind(self.bind_address).await
            .map_err(|e| Error::Network(format!("Failed to bind to {}: {}", self.bind_address, e)))?;
        
        info!("Stratum server listening on {}", self.bind_address);

        let mut shutdown_rx = self.shutdown_rx.take()
            .ok_or_else(|| Error::Internal("Server already started".to_string()))?;

        loop {
            tokio::select! {
                // Accept new connections
                result = listener.accept() => {
                    match result {
                        Ok((stream, peer_addr)) => {
                            let connection_id = Uuid::new_v4();
                            
                            info!("Accepted connection from {}: {}", peer_addr, connection_id);

                            // Create connection handler
                            let (_conn_shutdown_tx, conn_shutdown_rx) = mpsc::channel(1);
                            let handler = ConnectionHandler::new(
                                connection_id,
                                stream,
                                peer_addr,
                                self.message_tx.clone(),
                                conn_shutdown_rx,
                            );

                            // Store connection for later communication
                            let (response_tx, _response_rx) = mpsc::unbounded_channel();
                            self.connections.write().await.insert(connection_id, response_tx);

                            // Spawn connection handler
                            let connections = Arc::clone(&self.connections);
                            let message_tx = self.message_tx.clone();
                            tokio::spawn(async move {
                                // Send connection established message
                                let connect_msg = NetworkProtocolMessage::Connect {
                                    connection_id,
                                    peer_addr,
                                    protocol: Protocol::StratumV1, // Will be updated when detected
                                };
                                if let Err(e) = message_tx.send(connect_msg) {
                                    error!("Failed to send connect message: {}", e);
                                }

                                // Handle the connection
                                if let Err(e) = handler.handle().await {
                                    error!("Connection handler error for {}: {}", connection_id, e);
                                }
                                
                                // Send disconnect message
                                let disconnect_msg = NetworkProtocolMessage::Disconnect {
                                    connection_id,
                                    reason: "Connection closed".to_string(),
                                };
                                let _ = message_tx.send(disconnect_msg);
                                
                                // Clean up connection
                                connections.write().await.remove(&connection_id);
                                info!("Connection {} cleaned up", connection_id);
                            });
                        }
                        Err(e) => {
                            error!("Failed to accept connection: {}", e);
                        }
                    }
                }
                // Handle shutdown signal
                _ = shutdown_rx.recv() => {
                    info!("Shutting down Stratum server");
                    break;
                }
            }
        }

        // Close all connections
        let connections = self.connections.read().await;
        for (connection_id, _) in connections.iter() {
            info!("Closing connection: {}", connection_id);
        }

        Ok(())
    }

    /// Send a message to a specific connection
    pub async fn send_to_connection(&self, connection_id: ConnectionId, message: &str) -> Result<()> {
        let connections = self.connections.read().await;
        if let Some(tx) = connections.get(&connection_id) {
            tx.send(message.to_string())
                .map_err(|e| Error::Network(format!("Failed to send to connection {}: {}", connection_id, e)))?;
        } else {
            return Err(Error::Network(format!("Connection not found: {}", connection_id)));
        }
        Ok(())
    }

    /// Get message sender for a connection
    pub async fn get_connection_sender(&self, connection_id: ConnectionId) -> Option<mpsc::UnboundedSender<String>> {
        let connections = self.connections.read().await;
        connections.get(&connection_id).cloned()
    }

    /// Broadcast a message to all connections
    pub async fn broadcast(&self, message: &str) -> Result<()> {
        let connections = self.connections.read().await;
        for (connection_id, tx) in connections.iter() {
            if let Err(e) = tx.send(message.to_string()) {
                warn!("Failed to send broadcast to {}: {}", connection_id, e);
            }
        }
        Ok(())
    }

    /// Get the number of active connections
    pub async fn connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Shutdown the server
    pub async fn shutdown(&self) -> Result<()> {
        self.shutdown_tx.send(()).await
            .map_err(|e| Error::Internal(format!("Failed to send shutdown signal: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpStream;

    #[tokio::test]
    async fn test_server_creation() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = StratumServer::new("127.0.0.1:0".parse().unwrap(), tx);
        assert_eq!(server.connection_counter.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn test_connection_count() {
        let (tx, _rx) = mpsc::unbounded_channel();
        let server = StratumServer::new("127.0.0.1:0".parse().unwrap(), tx);
        assert_eq!(server.connection_count().await, 0);
    }
}