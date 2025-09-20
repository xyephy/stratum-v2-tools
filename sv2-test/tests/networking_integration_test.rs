// Standalone networking integration test
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

/// Test that Stratum server accepts TCP connections on port 4254
#[tokio::test]
async fn test_stratum_server_tcp_connectivity() -> Result<()> {
    let test_port = 34254;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
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
    
    println!("Successfully connected to Stratum server on port {}", test_port);
    
    // Verify connection is established
    assert!(stream.peer_addr().is_ok());
    
    // Check that server received connection message
    let connect_msg = timeout(Duration::from_millis(1000), message_rx.recv()).await
        .map_err(|_| sv2_core::Error::Network("Timeout waiting for connect message".to_string()))?
        .ok_or_else(|| sv2_core::Error::Network("No connect message received".to_string()))?;
    
    match connect_msg {
        NetworkProtocolMessage::Connect { peer_addr, protocol, .. } => {
            println!("Received connect message from {}, protocol: {:?}", peer_addr, protocol);
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
    let test_port = 38080;
    let database = Arc::new(MockDatabaseOps::new());
    let daemon_status = Arc::new(RwLock::new(DaemonStatus::default()));
    let mining_stats = Arc::new(RwLock::new(MiningStats::default()));
    
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let api_server = ApiServer::new(bind_address, database, daemon_status, mining_stats);
    
    println!("Starting API server on port {}", test_port);
    
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
    
    println!("API server health check response: {}", response.status());
    
    // Verify successful response
    assert!(response.status().is_success());
    
    let body = response.text().await
        .map_err(|e| sv2_core::Error::Network(format!("Failed to read response body: {}", e)))?;
    
    println!("API server response body: {}", body);
    assert!(body.contains("success") || body.contains("OK"));
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

/// Test complete solo mining workflow from miner connection to block submission
#[tokio::test]
async fn test_complete_solo_mining_workflow() -> Result<()> {
    let test_port = 35001;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    println!("Starting complete solo mining workflow test");
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(300)).await;
    
    // Connect miner
    let mut stream = TcpStream::connect(bind_address).await?;
    println!("Miner connected to server");
    
    // Step 1: Mining subscription
    let subscribe_msg = r#"{"id":1,"method":"mining.subscribe","params":["test_miner/1.0","session_id"]}"#;
    stream.write_all(format!("{}\n", subscribe_msg).as_bytes()).await?;
    
    let mut buffer = vec![0u8; 2048];
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Subscribe response: {}", response.trim());
    
    // Verify subscription response contains required fields
    assert!(response.contains("result"));
    assert!(response.contains("id"));
    
    // Step 2: Worker authorization
    let auth_msg = r#"{"id":2,"method":"mining.authorize","params":["worker1","password123"]}"#;
    stream.write_all(format!("{}\n", auth_msg).as_bytes()).await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Authorization response: {}", response.trim());
    
    // Verify authorization success
    assert!(response.contains("true"));
    
    // Step 3: Submit a share
    let submit_msg = r#"{"id":3,"method":"mining.submit","params":["worker1","job_001","00000000","507c7f00","12345678"]}"#;
    stream.write_all(format!("{}\n", submit_msg).as_bytes()).await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    println!("Share submission response: {}", response.trim());
    
    // Verify share acceptance (for now, server accepts all shares)
    assert!(response.contains("true"));
    
    // Verify protocol messages were received
    let mut connect_received = false;
    let mut stratum_messages = 0;
    
    for _ in 0..10 {
        if let Ok(Some(msg)) = timeout(Duration::from_millis(100), message_rx.recv()).await {
            match msg {
                NetworkProtocolMessage::Connect { .. } => {
                    connect_received = true;
                    println!("Received connect message");
                }
                NetworkProtocolMessage::StratumV1 { .. } => {
                    stratum_messages += 1;
                    println!("Received Stratum V1 message");
                }
                _ => {}
            }
        } else {
            break;
        }
    }
    
    assert!(connect_received, "Should receive connect message");
    assert!(stratum_messages >= 3, "Should receive subscribe, auth, and submit messages");
    
    println!("Complete solo mining workflow test completed successfully");
    
    // Clean up
    drop(stream);
    server_handle.abort();
    
    Ok(())
}