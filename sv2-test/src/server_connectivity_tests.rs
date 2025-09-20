// Server connectivity tests for task 18.1
use sv2_core::{
    server::StratumServer,
    api_server::ApiServer,
    protocol::NetworkProtocolMessage,
    types::{DaemonStatus, MiningStats},
    database::MockDatabaseOps,
    Result,
};
use std::{
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::{mpsc, RwLock},
    time::{sleep, timeout},
};
use tracing::{info, warn};

/// Test that Stratum server accepts TCP connections on port 4254
#[tokio::test]
async fn test_stratum_server_tcp_connectivity() -> Result<()> {
    let test_port = 24254;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    info!("Starting Stratum server on port {}", test_port);
    
    // Start server in background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Give server time to bind to port
    sleep(Duration::from_millis(500)).await;
    
    // Test basic TCP connectivity
    let stream = TcpStream::connect(bind_address).await
        .map_err(|e| sv2_core::Error::Network(format!("Failed to connect to Stratum server: {}", e)))?;
    
    info!("Successfully connected to Stratum server on port {}", test_port);
    
    // Verify connection is established
    assert!(stream.peer_addr().is_ok());
    
    // Check that server received connection message
    let connect_msg = timeout(Duration::from_millis(1000), message_rx.recv()).await
        .map_err(|_| sv2_core::Error::Network("Timeout waiting for connect message".to_string()))?
        .ok_or_else(|| sv2_core::Error::Network("No connect message received".to_string()))?;
    
    match connect_msg {
        NetworkProtocolMessage::Connect { peer_addr, protocol, .. } => {
            info!("Received connect message from {}, protocol: {:?}", peer_addr, protocol);
            assert_eq!(peer_addr.port(), stream.local_addr().unwrap().port());
        }
        _ => return Err(sv2_core::Error::Network("Expected connect message".to_string())),
    }
    
    // Clean up
    drop(stream);
    server_handle.abort();
    
    Ok(())
}

/// Test that HTTP API server responds to requests on port 8080
#[tokio::test]
async fn test_api_server_http_connectivity() -> Result<()> {
    let test_port = 28080;
    let database = Arc::new(MockDatabaseOps::new());
    let daemon_status = Arc::new(RwLock::new(DaemonStatus::default()));
    let mining_stats = Arc::new(RwLock::new(MiningStats::default()));
    
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let api_server = ApiServer::new(bind_address, database, daemon_status, mining_stats);
    
    info!("Starting API server on port {}", test_port);
    
    // Start API server in background
    let server_handle = tokio::spawn(async move {
        if let Err(e) = api_server.start().await {
            eprintln!("API server error: {}", e);
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(500)).await;
    
    // Test HTTP connectivity with health endpoint
    let client = reqwest::Client::new();
    let health_url = format!("http://127.0.0.1:{}/api/v1/health", test_port);
    
    let response = client.get(&health_url).send().await
        .map_err(|e| sv2_core::Error::Network(format!("Failed to connect to API server: {}", e)))?;
    
    info!("API server health check response: {}", response.status());
    
    // Verify successful response
    assert!(response.status().is_success());
    
    let body = response.text().await
        .map_err(|e| sv2_core::Error::Network(format!("Failed to read response body: {}", e)))?;
    
    info!("API server response body: {}", body);
    assert!(body.contains("success") || body.contains("OK"));
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

/// Test proper protocol detection for SV1 vs SV2 connections
#[tokio::test]
async fn test_protocol_detection() -> Result<()> {
    let test_port = 24255;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Test SV1 protocol detection
    {
        let mut stream = TcpStream::connect(bind_address).await?;
        
        // Send SV1 mining.subscribe message
        let sv1_msg = r#"{"id":1,"method":"mining.subscribe","params":["test_miner/1.0"]}"#;
        stream.write_all(format!("{}\n", sv1_msg).as_bytes()).await?;
        
        // Wait for protocol messages
        let mut sv1_detected = false;
        for _ in 0..5 {
            if let Ok(Some(msg)) = timeout(Duration::from_millis(200), message_rx.recv()).await {
                match msg {
                    NetworkProtocolMessage::Connect { protocol, .. } => {
                        info!("Initial protocol detection: {:?}", protocol);
                    }
                    NetworkProtocolMessage::StratumV1 { .. } => {
                        info!("SV1 protocol message detected");
                        sv1_detected = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
        
        assert!(sv1_detected, "SV1 protocol should be detected");
        drop(stream);
    }
    
    // Test SV2 protocol detection (simulated)
    {
        let mut stream = TcpStream::connect(bind_address).await?;
        
        // Send SV2-style message (with msg_type field)
        let sv2_msg = r#"{"msg_type":"setup_connection","protocol":"mining"}"#;
        stream.write_all(format!("{}\n", sv2_msg).as_bytes()).await?;
        
        // Wait for protocol messages
        let mut sv2_detected = false;
        for _ in 0..5 {
            if let Ok(Some(msg)) = timeout(Duration::from_millis(200), message_rx.recv()).await {
                match msg {
                    NetworkProtocolMessage::StratumV2 { .. } => {
                        info!("SV2 protocol message detected");
                        sv2_detected = true;
                        break;
                    }
                    _ => {}
                }
            }
        }
        
        assert!(sv2_detected, "SV2 protocol should be detected");
        drop(stream);
    }
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

/// Test connection cleanup and resource management
#[tokio::test]
async fn test_connection_cleanup() -> Result<()> {
    let test_port = 24256;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let server = StratumServer::new(bind_address, message_tx);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        let mut server = server;
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Create and immediately drop connections
    let mut connection_ids = Vec::new();
    
    for i in 0..3 {
        let stream = TcpStream::connect(bind_address).await?;
        info!("Created connection {}", i);
        
        // Collect connection ID from connect message
        if let Ok(Some(msg)) = timeout(Duration::from_millis(500), message_rx.recv()).await {
            if let NetworkProtocolMessage::Connect { connection_id, .. } = msg {
                connection_ids.push(connection_id);
            }
        }
        
        // Drop connection immediately
        drop(stream);
        info!("Dropped connection {}", i);
    }
    
    // Wait for disconnect messages
    let mut disconnect_count = 0;
    for _ in 0..10 {
        if let Ok(Some(msg)) = timeout(Duration::from_millis(200), message_rx.recv()).await {
            if let NetworkProtocolMessage::Disconnect { connection_id, reason } = msg {
                info!("Received disconnect for {}: {}", connection_id, reason);
                disconnect_count += 1;
                if disconnect_count >= 3 {
                    break;
                }
            }
        } else {
            break;
        }
    }
    
    assert_eq!(disconnect_count, 3, "Should receive disconnect messages for all connections");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

/// Load test for multiple concurrent connections
#[tokio::test]
async fn test_concurrent_connections_load() -> Result<()> {
    let test_port = 24257;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let server = StratumServer::new(bind_address, message_tx);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        let mut server = server;
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Create multiple concurrent connections
    let connection_count = 50; // Reduced for CI stability
    let mut handles = Vec::new();
    
    info!("Creating {} concurrent connections", connection_count);
    
    for i in 0..connection_count {
        let handle = tokio::spawn(async move {
            match TcpStream::connect(bind_address).await {
                Ok(mut stream) => {
                    // Send a subscribe message
                    let msg = format!(r#"{{"id":{},"method":"mining.subscribe","params":["miner_{}/1.0"]}}"#, i, i);
                    if let Err(e) = stream.write_all(format!("{}\n", msg).as_bytes()).await {
                        warn!("Failed to send message on connection {}: {}", i, e);
                        return false;
                    }
                    
                    // Try to read response
                    let mut buffer = vec![0u8; 1024];
                    match timeout(Duration::from_millis(2000), stream.read(&mut buffer)).await {
                        Ok(Ok(n)) if n > 0 => {
                            let response = String::from_utf8_lossy(&buffer[..n]);
                            info!("Connection {} received response: {}", i, response.trim());
                            true
                        }
                        _ => {
                            warn!("Connection {} failed to receive response", i);
                            false
                        }
                    }
                }
                Err(e) => {
                    warn!("Failed to create connection {}: {}", i, e);
                    false
                }
            }
        });
        handles.push(handle);
    }
    
    // Wait for all connections to complete
    let mut successful_connections = 0;
    for handle in handles {
        if let Ok(success) = handle.await {
            if success {
                successful_connections += 1;
            }
        }
    }
    
    info!("Successful connections: {}/{}", successful_connections, connection_count);
    
    // Verify that most connections succeeded (allow some failures in CI)
    assert!(successful_connections >= connection_count * 8 / 10, 
           "At least 80% of connections should succeed");
    
    // Count connect messages received
    let mut connect_messages = 0;
    for _ in 0..connection_count * 2 {
        if let Ok(Some(msg)) = timeout(Duration::from_millis(50), message_rx.recv()).await {
            if matches!(msg, NetworkProtocolMessage::Connect { .. }) {
                connect_messages += 1;
            }
        } else {
            break;
        }
    }
    
    info!("Connect messages received: {}", connect_messages);
    assert!(connect_messages >= successful_connections * 8 / 10,
           "Should receive connect messages for most successful connections");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

/// Test API server endpoint availability
#[tokio::test]
async fn test_api_endpoints_availability() -> Result<()> {
    let test_port = 28081;
    let database = Arc::new(MockDatabaseOps::new());
    let daemon_status = Arc::new(RwLock::new(DaemonStatus::default()));
    let mining_stats = Arc::new(RwLock::new(MiningStats::default()));
    
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let api_server = ApiServer::new(bind_address, database, daemon_status, mining_stats);
    
    // Start API server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = api_server.start().await {
            eprintln!("API server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(500)).await;
    
    let client = reqwest::Client::new();
    let base_url = format!("http://127.0.0.1:{}", test_port);
    
    // Test all major API endpoints
    let endpoints = vec![
        "/api/v1/health",
        "/api/v1/status", 
        "/api/v1/connections",
        "/api/v1/mining/stats",
        "/api/v1/mining/templates",
    ];
    
    for endpoint in endpoints {
        let url = format!("{}{}", base_url, endpoint);
        info!("Testing endpoint: {}", url);
        
        let response = client.get(&url).send().await
            .map_err(|e| sv2_core::Error::Network(format!("Failed to request {}: {}", endpoint, e)))?;
        
        info!("Endpoint {} returned status: {}", endpoint, response.status());
        
        // All endpoints should return success or at least not server error
        assert!(response.status().is_success() || response.status().is_client_error(),
               "Endpoint {} should not return server error", endpoint);
    }
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

/// Test server resource limits and graceful handling
#[tokio::test]
async fn test_server_resource_limits() -> Result<()> {
    let test_port = 24258;
    let (message_tx, _message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let server = StratumServer::new(bind_address, message_tx);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        let mut server = server;
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Test rapid connection creation and closure
    let rapid_connections = 20;
    let mut successful_rapid = 0;
    
    for _i in 0..rapid_connections {
        if let Ok(stream) = TcpStream::connect(bind_address).await {
            successful_rapid += 1;
            drop(stream); // Immediately close
        }
        // Small delay to avoid overwhelming
        sleep(Duration::from_millis(10)).await;
    }
    
    info!("Rapid connections successful: {}/{}", successful_rapid, rapid_connections);
    assert!(successful_rapid >= rapid_connections / 2, 
           "Should handle at least half of rapid connections");
    
    // Test that server is still responsive after rapid connections
    sleep(Duration::from_millis(500)).await;
    
    let mut stream = TcpStream::connect(bind_address).await?;
    let msg = r#"{"id":1,"method":"mining.subscribe","params":["test_miner/1.0"]}"#;
    stream.write_all(format!("{}\n", msg).as_bytes()).await?;
    
    let mut buffer = vec![0u8; 1024];
    let n = match timeout(Duration::from_millis(2000), stream.read(&mut buffer)).await {
        Ok(Ok(n)) => n,
        Ok(Err(e)) => return Err(sv2_core::Error::Network(format!("Read error: {}", e))),
        Err(_) => return Err(sv2_core::Error::Network("Read timeout".to_string())),
    };
    
    assert!(n > 0, "Server should still be responsive after rapid connections");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}