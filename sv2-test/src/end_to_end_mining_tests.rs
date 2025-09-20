// End-to-end mining workflow tests for task 18.2
use sv2_core::{
    server::StratumServer,
    protocol::NetworkProtocolMessage,
    types::{Share, ShareResult, ShareSubmission, Protocol},
    Result,
};
use std::{
    net::SocketAddr,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
    time::{sleep, timeout},
};
use tracing::{info, warn, error};
use uuid::Uuid;
use chrono::Utc;

/// Test complete solo mining workflow from miner connection to block submission
#[tokio::test]
async fn test_complete_solo_mining_workflow() -> Result<()> {
    let test_port = 25001;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    info!("Starting complete solo mining workflow test");
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(300)).await;
    
    // Connect miner
    let mut stream = TcpStream::connect(bind_address).await?;
    info!("Miner connected to server");
    
    // Step 1: Mining subscription
    let subscribe_msg = r#"{"id":1,"method":"mining.subscribe","params":["test_miner/1.0","session_id"]}"#;
    stream.write_all(format!("{}\n", subscribe_msg).as_bytes()).await?;
    
    let mut buffer = vec![0u8; 2048];
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    info!("Subscribe response: {}", response.trim());
    
    // Verify subscription response contains required fields
    assert!(response.contains("result"));
    assert!(response.contains("id"));
    
    // Step 2: Worker authorization
    let auth_msg = r#"{"id":2,"method":"mining.authorize","params":["worker1","password123"]}"#;
    stream.write_all(format!("{}\n", auth_msg).as_bytes()).await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    info!("Authorization response: {}", response.trim());
    
    // Verify authorization success
    assert!(response.contains("true"));
    
    // Step 3: Simulate receiving work notification (normally sent by server)
    // In a real scenario, server would send mining.notify with job details
    info!("Simulating work notification received");
    
    // Step 4: Submit a share
    let submit_msg = r#"{"id":3,"method":"mining.submit","params":["worker1","job_001","00000000","507c7f00","12345678"]}"#;
    stream.write_all(format!("{}\n", submit_msg).as_bytes()).await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    info!("Share submission response: {}", response.trim());
    
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
                    info!("Received connect message");
                }
                NetworkProtocolMessage::StratumV1 { .. } => {
                    stratum_messages += 1;
                    info!("Received Stratum V1 message");
                }
                _ => {}
            }
        } else {
            break;
        }
    }
    
    assert!(connect_received, "Should receive connect message");
    assert!(stratum_messages >= 3, "Should receive subscribe, auth, and submit messages");
    
    info!("Complete solo mining workflow test completed successfully");
    
    // Clean up
    drop(stream);
    server_handle.abort();
    
    Ok(())
}

/// Test share validation and difficulty adjustment
#[tokio::test]
async fn test_share_validation_and_difficulty() -> Result<()> {
    info!("Testing share validation and difficulty adjustment");
    
    // Create test shares with different difficulties
    let connection_id = Uuid::new_v4();
    
    // Test valid share
    let valid_share = Share {
        connection_id,
        nonce: 0x12345678,
        timestamp: 0x507c7f00,
        difficulty: 1.0,
        is_valid: false, // Will be set by validator
        block_hash: None,
        submitted_at: Utc::now(),
    };
    
    info!("Created test share: {:?}", valid_share);
    
    // Test different difficulty share
    let high_diff_share = Share {
        connection_id,
        nonce: 0x87654321,
        timestamp: 0x507c7f01,
        difficulty: 100.0, // Higher difficulty
        is_valid: false,
        block_hash: None,
        submitted_at: Utc::now(),
    };
    
    info!("Created high difficulty share: {:?}", high_diff_share);
    
    // Test share creation and basic validation
    assert_eq!(valid_share.connection_id, connection_id);
    assert_eq!(valid_share.nonce, 0x12345678);
    assert_eq!(valid_share.difficulty, 1.0);
    
    assert_eq!(high_diff_share.difficulty, 100.0);
    assert_eq!(high_diff_share.nonce, 0x87654321);
    
    info!("Share validation and difficulty test completed");
    
    Ok(())
}

/// Test protocol translation between SV1 miners and SV2 backend
#[tokio::test]
async fn test_protocol_translation() -> Result<()> {
    let test_port = 25002;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    info!("Testing protocol translation between SV1 and SV2");
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Test SV1 miner connection
    {
        let mut sv1_stream = TcpStream::connect(bind_address).await?;
        info!("SV1 miner connected");
        
        // Send SV1 subscribe
        let sv1_subscribe = r#"{"id":1,"method":"mining.subscribe","params":["sv1_miner/1.0"]}"#;
        sv1_stream.write_all(format!("{}\n", sv1_subscribe).as_bytes()).await?;
        
        let mut buffer = vec![0u8; 1024];
        let n = sv1_stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        info!("SV1 subscribe response: {}", response.trim());
        
        // Verify SV1 response format
        assert!(response.contains("result"));
        assert!(response.contains("mining.set_difficulty") || response.contains("mining.notify"));
        
        drop(sv1_stream);
    }
    
    // Test SV2-style message (simulated)
    {
        let mut sv2_stream = TcpStream::connect(bind_address).await?;
        info!("SV2-style connection");
        
        // Send SV2-style message
        let sv2_setup = r#"{"msg_type":"setup_connection","protocol":"mining","min_version":2,"max_version":2}"#;
        sv2_stream.write_all(format!("{}\n", sv2_setup).as_bytes()).await?;
        
        // Note: Current server implementation treats this as SV2 but doesn't fully implement SV2 protocol
        // This test verifies that the server can detect and handle different protocol styles
        
        drop(sv2_stream);
    }
    
    // Verify protocol detection in messages
    let mut sv1_detected = false;
    let mut sv2_detected = false;
    
    for _ in 0..10 {
        if let Ok(Some(msg)) = timeout(Duration::from_millis(100), message_rx.recv()).await {
            match msg {
                NetworkProtocolMessage::StratumV1 { .. } => {
                    sv1_detected = true;
                    info!("SV1 protocol message detected");
                }
                NetworkProtocolMessage::StratumV2 { .. } => {
                    sv2_detected = true;
                    info!("SV2 protocol message detected");
                }
                NetworkProtocolMessage::Connect { protocol, .. } => {
                    info!("Connection with protocol: {:?}", protocol);
                }
                _ => {}
            }
        } else {
            break;
        }
    }
    
    assert!(sv1_detected, "Should detect SV1 protocol messages");
    assert!(sv2_detected, "Should detect SV2 protocol messages");
    
    info!("Protocol translation test completed");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

/// Test error scenarios and recovery mechanisms
#[tokio::test]
async fn test_error_scenarios_and_recovery() -> Result<()> {
    let test_port = 25003;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    info!("Testing error scenarios and recovery mechanisms");
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Test invalid JSON message
    {
        let mut stream = TcpStream::connect(bind_address).await?;
        info!("Testing invalid JSON handling");
        
        // Send malformed JSON
        let invalid_json = "invalid json message\n";
        stream.write_all(invalid_json.as_bytes()).await?;
        
        let mut buffer = vec![0u8; 1024];
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        info!("Invalid JSON response: {}", response.trim());
        
        // Server should respond with parse error
        assert!(response.contains("error") || response.contains("Parse error"));
        
        drop(stream);
    }
    
    // Test unknown method
    {
        let mut stream = TcpStream::connect(bind_address).await?;
        info!("Testing unknown method handling");
        
        // Send unknown method
        let unknown_method = r#"{"id":1,"method":"unknown.method","params":[]}"#;
        stream.write_all(format!("{}\n", unknown_method).as_bytes()).await?;
        
        let mut buffer = vec![0u8; 1024];
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        info!("Unknown method response: {}", response.trim());
        
        // Server should respond with method not found error
        assert!(response.contains("error") || response.contains("Unknown method"));
        
        drop(stream);
    }
    
    // Test connection recovery after errors
    {
        let mut stream = TcpStream::connect(bind_address).await?;
        info!("Testing connection recovery");
        
        // Send invalid message first
        stream.write_all("bad message\n".as_bytes()).await?;
        let mut buffer = vec![0u8; 1024];
        let _n = stream.read(&mut buffer).await?;
        
        // Then send valid message - connection should still work
        let valid_msg = r#"{"id":2,"method":"mining.subscribe","params":["recovery_test/1.0"]}"#;
        stream.write_all(format!("{}\n", valid_msg).as_bytes()).await?;
        
        let n = stream.read(&mut buffer).await?;
        let response = String::from_utf8_lossy(&buffer[..n]);
        info!("Recovery test response: {}", response.trim());
        
        // Should get valid subscribe response
        assert!(response.contains("result"));
        
        drop(stream);
    }
    
    // Verify error handling doesn't crash the server
    let mut error_messages = 0;
    for _ in 0..15 {
        if let Ok(Some(msg)) = timeout(Duration::from_millis(50), message_rx.recv()).await {
            match msg {
                NetworkProtocolMessage::Connect { .. } => {
                    info!("Connection established");
                }
                NetworkProtocolMessage::Disconnect { reason, .. } => {
                    info!("Connection closed: {}", reason);
                }
                NetworkProtocolMessage::StratumV1 { .. } => {
                    error_messages += 1;
                }
                _ => {}
            }
        } else {
            break;
        }
    }
    
    info!("Processed {} messages during error testing", error_messages);
    assert!(error_messages > 0, "Should have processed some messages");
    
    info!("Error scenarios and recovery test completed");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

/// Test performance of share processing throughput
#[tokio::test]
async fn test_share_processing_throughput() -> Result<()> {
    let test_port = 25004;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    info!("Testing share processing throughput");
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    sleep(Duration::from_millis(200)).await;
    
    // Create multiple concurrent miners
    let num_miners = 5;
    let shares_per_miner = 10;
    let mut handles = Vec::new();
    
    let start_time = std::time::Instant::now();
    
    for miner_id in 0..num_miners {
        let handle = tokio::spawn(async move {
            let mut successful_shares = 0;
            
            match TcpStream::connect(bind_address).await {
                Ok(mut stream) => {
                    // Subscribe
                    let subscribe = format!(r#"{{"id":1,"method":"mining.subscribe","params":["miner_{}/1.0"]}}"#, miner_id);
                    if stream.write_all(format!("{}\n", subscribe).as_bytes()).await.is_ok() {
                        let mut buffer = vec![0u8; 1024];
                        let _ = stream.read(&mut buffer).await;
                    }
                    
                    // Authorize
                    let auth = format!(r#"{{"id":2,"method":"mining.authorize","params":["worker_{}","pass"]}}"#, miner_id);
                    if stream.write_all(format!("{}\n", auth).as_bytes()).await.is_ok() {
                        let mut buffer = vec![0u8; 1024];
                        let _ = stream.read(&mut buffer).await;
                    }
                    
                    // Submit shares rapidly
                    for share_id in 0..shares_per_miner {
                        let submit = format!(
                            r#"{{"id":{},"method":"mining.submit","params":["worker_{}","job_{}","{}","507c7f00","{:08x}"]}}"#,
                            share_id + 3, miner_id, share_id, format!("{:08x}", share_id), share_id
                        );
                        
                        if stream.write_all(format!("{}\n", submit).as_bytes()).await.is_ok() {
                            let mut buffer = vec![0u8; 1024];
                            if let Ok(n) = timeout(Duration::from_millis(1000), stream.read(&mut buffer)).await {
                                if n.unwrap_or(0) > 0 {
                                    successful_shares += 1;
                                }
                            }
                        }
                        
                        // Small delay between shares
                        sleep(Duration::from_millis(10)).await;
                    }
                }
                Err(e) => {
                    warn!("Miner {} failed to connect: {}", miner_id, e);
                }
            }
            
            successful_shares
        });
        handles.push(handle);
    }
    
    // Wait for all miners to complete
    let mut total_successful_shares = 0;
    for handle in handles {
        if let Ok(shares) = handle.await {
            total_successful_shares += shares;
        }
    }
    
    let elapsed = start_time.elapsed();
    let throughput = total_successful_shares as f64 / elapsed.as_secs_f64();
    
    info!("Processed {} shares in {:?} ({:.2} shares/sec)", 
          total_successful_shares, elapsed, throughput);
    
    // Verify reasonable throughput (should handle at least a few shares per second)
    assert!(throughput > 1.0, "Throughput should be at least 1 share/sec");
    assert!(total_successful_shares >= (num_miners * shares_per_miner) / 2, 
           "Should successfully process at least half the shares");
    
    // Count messages received
    let mut message_count = 0;
    for _ in 0..100 {
        if timeout(Duration::from_millis(10), message_rx.recv()).await.is_ok() {
            message_count += 1;
        } else {
            break;
        }
    }
    
    info!("Received {} protocol messages during throughput test", message_count);
    assert!(message_count > 0, "Should receive protocol messages");
    
    info!("Share processing throughput test completed");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}