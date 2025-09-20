use futures::{SinkExt, StreamExt};
use serde_json::json;
use std::sync::Arc;
use tokio::time::{timeout, Duration};
use tokio_tungstenite::{connect_async, tungstenite::Message};
use uuid::Uuid;

use sv2_core::{
    config::DaemonConfig,
    database::{DatabasePool, DatabaseOps},
    types::{ConnectionInfo, Share, Alert, AlertLevel, Protocol, ConnectionState, MiningStats},
    DaemonStatus,
};
use sv2_web::websocket::{WebSocketMessage, WebSocketRequest, WebSocketBroadcaster};

async fn setup_test_database() -> Arc<dyn DatabaseOps> {
    let database = DatabasePool::new("sqlite::memory:", 10).await.unwrap();
    database.migrate().await.unwrap();
    Arc::new(database) as Arc<dyn DatabaseOps>
}

#[tokio::test]
async fn test_websocket_broadcaster() {
    let broadcaster = WebSocketBroadcaster::new();
    let mut receiver = broadcaster.subscribe();

    // Test status broadcast
    let status = DaemonStatus {
        uptime: Duration::from_secs(3600),
        connections: 5,
        total_shares: 1000,
        valid_shares: 950,
        blocks_found: 1,
        current_difficulty: 1.0,
        hashrate: 1e12,
    };

    broadcaster.broadcast_status(status.clone());

    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");

    match message {
        WebSocketMessage::Status(received_status) => {
            assert_eq!(received_status.connections, status.connections);
            assert_eq!(received_status.total_shares, status.total_shares);
            assert_eq!(received_status.valid_shares, status.valid_shares);
        }
        _ => panic!("Expected status message"),
    }
}

#[tokio::test]
async fn test_websocket_connection_events() {
    let broadcaster = WebSocketBroadcaster::new();
    let mut receiver = broadcaster.subscribe();

    // Test connection added event
    let connection = ConnectionInfo {
        id: Uuid::new_v4(),
        address: "127.0.0.1:3333".parse().unwrap(),
        protocol: Protocol::Sv2,
        state: ConnectionState::Connected,
        connected_at: chrono::Utc::now(),
        last_activity: chrono::Utc::now(),
        user_agent: Some("test-miner".to_string()),
        version: Some("1.0.0".to_string()),
        subscribed_difficulty: Some(1.0),
        extranonce1: Some("abcd".to_string()),
        extranonce2_size: Some(4),
        authorized_workers: vec!["worker1".to_string()],
        total_shares: 0,
        valid_shares: 0,
        invalid_shares: 0,
        blocks_found: 0,
    };

    broadcaster.notify_connection_added(connection.clone());

    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");

    match message {
        WebSocketMessage::ConnectionAdded(received_connection) => {
            assert_eq!(received_connection.id, connection.id);
            assert_eq!(received_connection.protocol, Protocol::Sv2);
            assert_eq!(received_connection.user_agent, Some("test-miner".to_string()));
        }
        _ => panic!("Expected connection added message"),
    }

    // Test connection updated event
    let mut updated_connection = connection.clone();
    updated_connection.total_shares = 100;
    updated_connection.valid_shares = 95;

    broadcaster.notify_connection_updated(updated_connection.clone());

    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");

    match message {
        WebSocketMessage::ConnectionUpdated(received_connection) => {
            assert_eq!(received_connection.id, connection.id);
            assert_eq!(received_connection.total_shares, 100);
            assert_eq!(received_connection.valid_shares, 95);
        }
        _ => panic!("Expected connection updated message"),
    }

    // Test connection removed event
    broadcaster.notify_connection_removed(connection.id);

    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");

    match message {
        WebSocketMessage::ConnectionRemoved { id } => {
            assert_eq!(id, connection.id);
        }
        _ => panic!("Expected connection removed message"),
    }
}

#[tokio::test]
async fn test_websocket_share_events() {
    let broadcaster = WebSocketBroadcaster::new();
    let mut receiver = broadcaster.subscribe();

    let share = Share {
        connection_id: Uuid::new_v4(),
        nonce: 12345,
        timestamp: chrono::Utc::now().timestamp() as u32,
        difficulty: 1.0,
        is_valid: true,
        block_hash: None,
        submitted_at: chrono::Utc::now(),
    };

    broadcaster.notify_share_submitted(share.clone());

    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");

    match message {
        WebSocketMessage::ShareSubmitted(received_share) => {
            assert_eq!(received_share.connection_id, share.connection_id);
            assert_eq!(received_share.nonce, share.nonce);
            assert_eq!(received_share.difficulty, share.difficulty);
            assert!(received_share.is_valid);
        }
        _ => panic!("Expected share submitted message"),
    }
}

#[tokio::test]
async fn test_websocket_alert_events() {
    let broadcaster = WebSocketBroadcaster::new();
    let mut receiver = broadcaster.subscribe();

    // Test alert created event
    let alert = Alert::new(
        AlertLevel::Critical,
        "High CPU Usage".to_string(),
        "CPU usage is above 90%".to_string(),
        "system_monitor".to_string(),
    );

    broadcaster.notify_alert_created(alert.clone());

    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");

    match message {
        WebSocketMessage::AlertCreated(received_alert) => {
            assert_eq!(received_alert.id, alert.id);
            assert_eq!(received_alert.level, AlertLevel::Critical);
            assert_eq!(received_alert.title, "High CPU Usage");
            assert_eq!(received_alert.component, "system_monitor");
        }
        _ => panic!("Expected alert created message"),
    }

    // Test alert resolved event
    broadcaster.notify_alert_resolved(alert.id);

    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");

    match message {
        WebSocketMessage::AlertResolved { id } => {
            assert_eq!(id, alert.id);
        }
        _ => panic!("Expected alert resolved message"),
    }
}

#[tokio::test]
async fn test_websocket_mining_stats_events() {
    let broadcaster = WebSocketBroadcaster::new();
    let mut receiver = broadcaster.subscribe();

    let mining_stats = MiningStats {
        hashrate: 5e12, // 5 TH/s
        shares_per_minute: 60.0,
        acceptance_rate: 95.5,
        efficiency: 98.2,
        uptime: Duration::from_secs(7200), // 2 hours
    };

    broadcaster.notify_mining_stats_update(mining_stats.clone());

    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");

    match message {
        WebSocketMessage::MiningStatsUpdate(received_stats) => {
            assert_eq!(received_stats.hashrate, mining_stats.hashrate);
            assert_eq!(received_stats.shares_per_minute, mining_stats.shares_per_minute);
            assert_eq!(received_stats.acceptance_rate, mining_stats.acceptance_rate);
            assert_eq!(received_stats.efficiency, mining_stats.efficiency);
        }
        _ => panic!("Expected mining stats update message"),
    }
}

#[tokio::test]
async fn test_websocket_message_serialization() {
    // Test all message types can be serialized and deserialized
    let messages = vec![
        WebSocketMessage::Status(DaemonStatus {
            uptime: Duration::from_secs(3600),
            connections: 5,
            total_shares: 1000,
            valid_shares: 950,
            blocks_found: 1,
            current_difficulty: 1.0,
            hashrate: 1e12,
        }),
        WebSocketMessage::ConnectionAdded(ConnectionInfo {
            id: Uuid::new_v4(),
            address: "127.0.0.1:3333".parse().unwrap(),
            protocol: Protocol::Sv1,
            state: ConnectionState::Connected,
            connected_at: chrono::Utc::now(),
            last_activity: chrono::Utc::now(),
            user_agent: None,
            version: None,
            subscribed_difficulty: None,
            extranonce1: None,
            extranonce2_size: None,
            authorized_workers: vec![],
            total_shares: 0,
            valid_shares: 0,
            invalid_shares: 0,
            blocks_found: 0,
        }),
        WebSocketMessage::ConnectionRemoved { id: Uuid::new_v4() },
        WebSocketMessage::ShareSubmitted(Share {
            connection_id: Uuid::new_v4(),
            nonce: 12345,
            timestamp: chrono::Utc::now().timestamp() as u32,
            difficulty: 1.0,
            is_valid: true,
            block_hash: None,
            submitted_at: chrono::Utc::now(),
        }),
        WebSocketMessage::AlertCreated(Alert::new(
            AlertLevel::Warning,
            "Test Alert".to_string(),
            "Test message".to_string(),
            "test".to_string(),
        )),
        WebSocketMessage::AlertResolved { id: Uuid::new_v4() },
        WebSocketMessage::Heartbeat { timestamp: chrono::Utc::now() },
        WebSocketMessage::Error { message: "Test error".to_string() },
        WebSocketMessage::Subscribed { subscriptions: vec!["status".to_string(), "connection".to_string()] },
    ];

    for message in messages {
        let json = serde_json::to_string(&message).expect("Should serialize");
        let deserialized: WebSocketMessage = serde_json::from_str(&json).expect("Should deserialize");
        
        // Basic type checking - more detailed checks would require implementing PartialEq
        match (&message, &deserialized) {
            (WebSocketMessage::Status(_), WebSocketMessage::Status(_)) => {},
            (WebSocketMessage::ConnectionAdded(_), WebSocketMessage::ConnectionAdded(_)) => {},
            (WebSocketMessage::ConnectionRemoved { .. }, WebSocketMessage::ConnectionRemoved { .. }) => {},
            (WebSocketMessage::ShareSubmitted(_), WebSocketMessage::ShareSubmitted(_)) => {},
            (WebSocketMessage::AlertCreated(_), WebSocketMessage::AlertCreated(_)) => {},
            (WebSocketMessage::AlertResolved { .. }, WebSocketMessage::AlertResolved { .. }) => {},
            (WebSocketMessage::Heartbeat { .. }, WebSocketMessage::Heartbeat { .. }) => {},
            (WebSocketMessage::Error { .. }, WebSocketMessage::Error { .. }) => {},
            (WebSocketMessage::Subscribed { .. }, WebSocketMessage::Subscribed { .. }) => {},
            _ => panic!("Message type mismatch after serialization/deserialization"),
        }
    }
}

#[tokio::test]
async fn test_websocket_request_serialization() {
    let requests = vec![
        WebSocketRequest::Subscribe { events: vec!["status".to_string(), "connection".to_string()] },
        WebSocketRequest::Unsubscribe { events: vec!["share".to_string()] },
        WebSocketRequest::GetStatus,
        WebSocketRequest::Ping,
    ];

    for request in requests {
        let json = serde_json::to_string(&request).expect("Should serialize");
        let deserialized: WebSocketRequest = serde_json::from_str(&json).expect("Should deserialize");
        
        // Basic type checking
        match (&request, &deserialized) {
            (WebSocketRequest::Subscribe { .. }, WebSocketRequest::Subscribe { .. }) => {},
            (WebSocketRequest::Unsubscribe { .. }, WebSocketRequest::Unsubscribe { .. }) => {},
            (WebSocketRequest::GetStatus, WebSocketRequest::GetStatus) => {},
            (WebSocketRequest::Ping, WebSocketRequest::Ping) => {},
            _ => panic!("Request type mismatch after serialization/deserialization"),
        }
    }
}

#[tokio::test]
async fn test_websocket_session_subscriptions() {
    use sv2_web::websocket::WebSocketSession;

    let mut session = WebSocketSession::new();
    
    // Initially not subscribed to anything
    assert!(!session.is_subscribed("status"));
    assert!(!session.is_subscribed("connection"));
    
    // Subscribe to specific events
    session.subscriptions.insert("status".to_string());
    session.subscriptions.insert("connection".to_string());
    
    assert!(session.is_subscribed("status"));
    assert!(session.is_subscribed("connection"));
    assert!(!session.is_subscribed("share"));
    
    // Subscribe to all events
    session.subscriptions.clear();
    session.subscriptions.insert("*".to_string());
    
    assert!(session.is_subscribed("status"));
    assert!(session.is_subscribed("connection"));
    assert!(session.is_subscribed("share"));
    assert!(session.is_subscribed("alert"));
}

#[tokio::test]
async fn test_websocket_session_activity_tracking() {
    use sv2_web::websocket::WebSocketSession;

    let mut session = WebSocketSession::new();
    let initial_activity = session.last_activity;
    
    // Wait a bit to ensure time difference
    tokio::time::sleep(Duration::from_millis(10)).await;
    
    session.update_activity();
    assert!(session.last_activity > initial_activity);
    assert!(session.last_activity >= session.connected_at);
}

#[tokio::test]
async fn test_multiple_websocket_subscribers() {
    let broadcaster = WebSocketBroadcaster::new();
    
    // Create multiple subscribers
    let mut receiver1 = broadcaster.subscribe();
    let mut receiver2 = broadcaster.subscribe();
    let mut receiver3 = broadcaster.subscribe();
    
    // Broadcast a message
    let status = DaemonStatus {
        uptime: Duration::from_secs(1800),
        connections: 3,
        total_shares: 500,
        valid_shares: 475,
        blocks_found: 0,
        current_difficulty: 1.0,
        hashrate: 5e11,
    };
    
    broadcaster.broadcast_status(status.clone());
    
    // All subscribers should receive the message
    for (i, receiver) in [&mut receiver1, &mut receiver2, &mut receiver3].iter_mut().enumerate() {
        let message = timeout(Duration::from_secs(1), receiver.recv())
            .await
            .expect(&format!("Subscriber {} should receive message within timeout", i + 1))
            .expect(&format!("Subscriber {} should receive valid message", i + 1));
        
        match message {
            WebSocketMessage::Status(received_status) => {
                assert_eq!(received_status.connections, status.connections);
                assert_eq!(received_status.total_shares, status.total_shares);
            }
            _ => panic!("Expected status message for subscriber {}", i + 1),
        }
    }
}

#[tokio::test]
async fn test_websocket_error_handling() {
    let broadcaster = WebSocketBroadcaster::new();
    let mut receiver = broadcaster.subscribe();
    
    // Test error message broadcasting
    let error_message = "Database connection failed";
    broadcaster.broadcast(WebSocketMessage::Error {
        message: error_message.to_string(),
    });
    
    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");
    
    match message {
        WebSocketMessage::Error { message } => {
            assert_eq!(message, error_message);
        }
        _ => panic!("Expected error message"),
    }
}

#[tokio::test]
async fn test_websocket_heartbeat() {
    let broadcaster = WebSocketBroadcaster::new();
    let mut receiver = broadcaster.subscribe();
    
    let timestamp = chrono::Utc::now();
    broadcaster.broadcast(WebSocketMessage::Heartbeat { timestamp });
    
    let message = timeout(Duration::from_secs(1), receiver.recv())
        .await
        .expect("Should receive message within timeout")
        .expect("Should receive valid message");
    
    match message {
        WebSocketMessage::Heartbeat { timestamp: received_timestamp } => {
            // Allow for small time differences due to processing
            let diff = (received_timestamp - timestamp).num_milliseconds().abs();
            assert!(diff < 1000, "Timestamp difference should be less than 1 second");
        }
        _ => panic!("Expected heartbeat message"),
    }
}

// Integration test that would require a running server
// This is commented out as it requires more complex setup
/*
#[tokio::test]
async fn test_websocket_end_to_end() {
    // This test would require starting the actual sv2-web server
    // and connecting to it via WebSocket client
    
    // Start server in background
    // let server_handle = tokio::spawn(async {
    //     sv2_web::main().await
    // });
    
    // Connect WebSocket client
    // let (ws_stream, _) = connect_async("ws://127.0.0.1:8080/ws").await.unwrap();
    // let (mut write, mut read) = ws_stream.split();
    
    // Send subscription request
    // let subscribe_request = json!({
    //     "action": "Subscribe",
    //     "events": ["status", "connection"]
    // });
    // write.send(Message::Text(subscribe_request.to_string())).await.unwrap();
    
    // Receive subscription confirmation
    // let message = read.next().await.unwrap().unwrap();
    // ... verify subscription confirmation
    
    // Receive periodic status updates
    // let status_message = read.next().await.unwrap().unwrap();
    // ... verify status message format
    
    // Clean up
    // server_handle.abort();
}
*/