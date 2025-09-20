use crate::{
    server::StratumServer,
    protocol::NetworkProtocolMessage,
    types::Protocol,
    Result,
};
use std::net::SocketAddr;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpStream,
    sync::mpsc,
    time::{sleep, Duration},
};

#[tokio::test]
async fn test_stratum_server_startup_and_connection() -> Result<()> {
    // Use a fixed port for testing
    let test_port = 14254;
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
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
            if let Err(e) = stream.write_all(format!("{}\n", subscribe_msg).as_bytes()).await {
                println!("Failed to send message: {}", e);
            } else {
                println!("Message sent successfully");
                
                // Try to read response with timeout
                let mut buffer = vec![0u8; 1024];
                match tokio::time::timeout(Duration::from_millis(1000), stream.read(&mut buffer)).await {
                    Ok(Ok(n)) => {
                        if n > 0 {
                            let response = String::from_utf8_lossy(&buffer[..n]);
                            println!("Server response: {}", response);
                            assert!(response.contains("result") || response.contains("id"));
                        } else {
                            println!("Connection closed by server");
                        }
                    }
                    Ok(Err(e)) => {
                        println!("Read error: {}", e);
                    }
                    Err(_) => {
                        println!("Read timeout");
                    }
                }
            }
        }
        Err(e) => {
            panic!("Failed to connect to server: {}", e);
        }
    }
    
    // Check that we received connection message
    let mut received_connect = false;
    for _ in 0..5 {
        if let Ok(Some(msg)) = tokio::time::timeout(Duration::from_millis(100), message_rx.recv()).await {
            match msg {
                NetworkProtocolMessage::Connect { protocol, .. } => {
                    println!("Received connect message with protocol: {:?}", protocol);
                    received_connect = true;
                    break;
                }
                other => {
                    println!("Received other message: {:?}", other);
                }
            }
        }
    }
    
    assert!(received_connect, "Should have received connect message");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

#[tokio::test]
async fn test_stratum_server_mining_workflow() -> Result<()> {
    let test_port = 14255; // Use different port
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(200)).await;
    
    // Connect to server
    let mut stream = TcpStream::connect(bind_address).await
        .map_err(|e| crate::Error::Network(format!("Failed to connect: {}", e)))?;
    
    // Test mining workflow: subscribe -> authorize -> submit
    
    // 1. Subscribe
    let subscribe_msg = r#"{"id":1,"method":"mining.subscribe","params":["test_miner/1.0"]}"#;
    stream.write_all(format!("{}\n", subscribe_msg).as_bytes()).await?;
    
    let mut buffer = vec![0u8; 1024];
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    assert!(response.contains("result"));
    
    // 2. Authorize
    let auth_msg = r#"{"id":2,"method":"mining.authorize","params":["worker1","password"]}"#;
    stream.write_all(format!("{}\n", auth_msg).as_bytes()).await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    assert!(response.contains("true"));
    
    // 3. Submit share
    let submit_msg = r#"{"id":3,"method":"mining.submit","params":["worker1","job1","00000000","507c7f00","12345678"]}"#;
    stream.write_all(format!("{}\n", submit_msg).as_bytes()).await?;
    
    let n = stream.read(&mut buffer).await?;
    let response = String::from_utf8_lossy(&buffer[..n]);
    assert!(response.contains("true"));
    
    // Verify we received the expected protocol messages
    let mut connect_received = false;
    let mut stratum_messages = 0;
    
    // Collect messages with timeout
    for _ in 0..10 {
        if let Ok(Some(msg)) = tokio::time::timeout(Duration::from_millis(50), message_rx.recv()).await {
            match msg {
                NetworkProtocolMessage::Connect { .. } => {
                    connect_received = true;
                }
                NetworkProtocolMessage::StratumV1 { .. } => {
                    stratum_messages += 1;
                }
                _ => {}
            }
        } else {
            break;
        }
    }
    
    assert!(connect_received, "Should have received connect message");
    assert!(stratum_messages >= 3, "Should have received at least 3 Stratum messages");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}

#[tokio::test]
async fn test_stratum_server_multiple_connections() -> Result<()> {
    let test_port = 14256; // Use different port
    let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();
    let bind_address: SocketAddr = format!("127.0.0.1:{}", test_port).parse().unwrap();
    let mut server = StratumServer::new(bind_address, message_tx);
    
    // Start server
    let server_handle = tokio::spawn(async move {
        if let Err(e) = server.start().await {
            eprintln!("Server error: {}", e);
        }
    });
    
    // Give server time to start
    sleep(Duration::from_millis(200)).await;
    
    // Create multiple connections
    let mut connections = Vec::new();
    for i in 0..3 {
        let mut stream = TcpStream::connect(bind_address).await?;
        
        // Send subscribe message
        let subscribe_msg = format!(r#"{{"id":1,"method":"mining.subscribe","params":["miner_{}/1.0"]}}"#, i);
        stream.write_all(format!("{}\n", subscribe_msg).as_bytes()).await?;
        
        connections.push(stream);
    }
    
    // Verify all connections are handled
    let mut connect_count = 0;
    for _ in 0..10 {
        if let Ok(Some(msg)) = tokio::time::timeout(Duration::from_millis(100), message_rx.recv()).await {
            if matches!(msg, NetworkProtocolMessage::Connect { .. }) {
                connect_count += 1;
            }
        } else {
            break;
        }
    }
    
    assert_eq!(connect_count, 3, "Should have received 3 connect messages");
    
    // Clean up
    server_handle.abort();
    
    Ok(())
}