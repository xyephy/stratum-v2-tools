use crate::{
    server::StratumServer,
    protocol::NetworkProtocolMessage,
    types::{DaemonStatus, MiningStats},
    Result,
};
use std::{net::SocketAddr, sync::Arc, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, RwLock},
    time::sleep,
};

#[tokio::test]
async fn test_protocol_message_routing() -> Result<()> {
    // Create test dependencies
    let daemon_status = Arc::new(RwLock::new(DaemonStatus::default()));
    let mining_stats = Arc::new(RwLock::new(MiningStats::default()));
    
    // Use a test port
    let test_port = 15254;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Collect messages for verification
    let messages_received = Arc::new(RwLock::new(Vec::new()));
    let message_processor_handle = tokio::spawn({
        let daemon_status = Arc::clone(&daemon_status);
        let mining_stats = Arc::clone(&mining_stats);
        let messages_received = Arc::clone(&messages_received);
        
        async move {
            while let Some(message) = message_rx.recv().await {
                // Store message type for verification
                let msg_type = match &message {
                    NetworkProtocolMessage::Connect { .. } => "Connect",
                    NetworkProtocolMessage::Disconnect { .. } => "Disconnect", 
                    NetworkProtocolMessage::StratumV1 { .. } => "StratumV1",
                    NetworkProtocolMessage::StratumV2 { .. } => "StratumV2",
                    NetworkProtocolMessage::SendResponse { .. } => "SendResponse",
                    NetworkProtocolMessage::SendWork { .. } => "SendWork",
                };
                messages_received.write().await.push(msg_type.to_string());
                
                if let Err(e) = process_test_message(
                    message,
                    &daemon_status,
                    &mining_stats,
                ).await {
                    eprintln!("Error processing message: {}", e);
                }
            }
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    // Test basic connection
    match TcpStream::connect(bind_address).await {
        Ok(mut stream) => {
            println!("Successfully connected to server");
            
            // Send a simple message
            let subscribe_msg = r#"{"id":1,"method":"mining.subscribe","params":["test_miner/1.0"]}"#;
            if let Ok(()) = stream.write_all(format!("{}\n", subscribe_msg).as_bytes()).await {
                println!("Message sent successfully");
                
                // Try to read response
                let mut buffer = vec![0u8; 1024];
                match tokio::time::timeout(Duration::from_millis(500), stream.read(&mut buffer)).await {
                    Ok(Ok(n)) if n > 0 => {
                        let response = String::from_utf8_lossy(&buffer[..n]);
                        println!("Server response: {}", response);
                    }
                    _ => {
                        println!("No response or timeout");
                    }
                }
            }
        }
        Err(e) => {
            panic!("Failed to connect to server: {}", e);
        }
    }
    
    // Give time for message processing
    sleep(Duration::from_millis(300)).await;
    
    // Verify we received protocol messages
    {
        let messages = messages_received.read().await;
        println!("Messages received: {:?}", *messages);
        assert!(messages.contains(&"Connect".to_string()), "Should have received Connect message");
        // We might also receive Disconnect and StratumV1 messages
    }
    
    // Verify daemon status was updated
    {
        let status = daemon_status.read().await;
        println!("Final daemon status: active_connections={}, total_connections={}", 
                 status.active_connections, status.total_connections);
        assert!(status.total_connections > 0, "Should have recorded at least one connection");
    }
    
    // Clean up
    server_handle.abort();
    message_processor_handle.abort();
    
    Ok(())
}

// Simplified message processor for testing
async fn process_test_message(
    message: NetworkProtocolMessage,
    daemon_status: &Arc<RwLock<DaemonStatus>>,
    mining_stats: &Arc<RwLock<MiningStats>>,
) -> Result<()> {
    match message {
        NetworkProtocolMessage::Connect { connection_id, peer_addr, protocol } => {
            println!("Test processor: New connection {} from {} using {:?}", connection_id, peer_addr, protocol);
            
            // Update daemon status
            {
                let mut status = daemon_status.write().await;
                status.active_connections += 1;
                status.total_connections += 1;
            }
        }
        NetworkProtocolMessage::Disconnect { connection_id, .. } => {
            println!("Test processor: Connection disconnected: {}", connection_id);
            
            // Update daemon status
            {
                let mut status = daemon_status.write().await;
                status.active_connections = status.active_connections.saturating_sub(1);
            }
        }
        NetworkProtocolMessage::StratumV1 { connection_id, message } => {
            println!("Test processor: Stratum V1 message from {}: {:?}", connection_id, message);
            
            // Handle share submissions (simplified)
            if let Some(method) = &message.method {
                if method == "mining.submit" {
                    if let Some(params) = &message.params {
                        if let Some(params_array) = params.as_array() {
                            if params_array.len() >= 5 {
                                // Simulate share processing
                                println!("Processing share submission from {}", connection_id);
                                
                                // For testing, accept most shares
                                let mut stats = mining_stats.write().await;
                                stats.shares_accepted += 1;
                            }
                        }
                    }
                }
            }
        }
        _ => {
            // Handle other message types
        }
    }
    Ok(())
}

