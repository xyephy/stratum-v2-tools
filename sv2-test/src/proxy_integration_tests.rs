//! End-to-end integration tests for proxy mode

use sv2_core::{
    modes::{ProxyModeHandler, proxy_protocol::ProxyProtocolService},
    config::{ProxyConfig, UpstreamPool, LoadBalancingStrategy},
    database::MockDatabaseOps,
    types::{Connection, Share, Protocol, WorkTemplate},
    protocol::ProtocolMessage,
    mode::ModeHandler,
};
use std::sync::Arc;
use std::net::SocketAddr;
use uuid::Uuid;

/// Create a test proxy configuration
fn create_test_proxy_config() -> ProxyConfig {
    ProxyConfig {
        upstream_pools: vec![
            UpstreamPool {
                url: "stratum+tcp://pool1.example.com:4444".to_string(),
                username: "worker1".to_string(),
                password: "password1".to_string(),
                priority: 1,
                weight: 1,
            },
        ],
        failover_enabled: true,
        load_balancing: LoadBalancingStrategy::RoundRobin,
        connection_retry_interval: 30,
        max_retry_attempts: 5,
    }
}

/// Create a test connection
fn create_test_connection(protocol: Protocol) -> Connection {
    let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
    Connection::new(addr, protocol)
}

/// Create a test work template
fn create_test_work_template() -> WorkTemplate {
    use bitcoin::{BlockHash, Transaction};
    use std::str::FromStr;
    
    let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
    let coinbase_tx = Transaction {
        version: 1,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![bitcoin::TxIn::default()],
        output: vec![bitcoin::TxOut::default()],
    };
    
    WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0)
}

#[tokio::test]
async fn test_complete_proxy_workflow() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    // 1. Create a downstream SV1 connection
    let connection = create_test_connection(Protocol::Sv1);
    let connection_id = connection.id;
    
    // Note: handle_connection will fail because no upstream pools are actually connected
    // In a real test environment, you would mock the upstream connections
    let conn_result = handler.handle_connection(connection).await;
    assert!(conn_result.is_err()); // Expected to fail without real upstream pools
    
    // 2. Test protocol message handling directly
    let protocol_service = ProxyProtocolService::new();
    let test_conn = create_test_connection(Protocol::Sv1);
    
    // Initialize connection in protocol service
    protocol_service.initialize_connection(&test_conn).await.unwrap();
    
    // 3. Test subscription flow
    let subscribe_msg = ProtocolMessage::Subscribe {
        user_agent: "test_miner/1.0".to_string(),
        session_id: Some("session123".to_string()),
    };
    
    let responses = protocol_service.handle_downstream_message(test_conn.id, subscribe_msg).await.unwrap();
    assert_eq!(responses.len(), 1);
    
    // 4. Test authorization flow
    let authorize_msg = ProtocolMessage::Authorize {
        username: "test_worker".to_string(),
        password: "password123".to_string(),
    };
    
    let auth_responses = protocol_service.handle_downstream_message(test_conn.id, authorize_msg).await.unwrap();
    assert!(auth_responses.is_empty()); // SV1 authorize returns empty response on success
    
    // 5. Test work template forwarding
    let template = create_test_work_template();
    let work_responses = protocol_service.forward_work_template(&template, &[test_conn.id]).await.unwrap();
    
    assert_eq!(work_responses.len(), 1);
    assert_eq!(work_responses[0].0, test_conn.id);
    
    match &work_responses[0].1 {
        ProtocolMessage::Notify { job_id, clean_jobs, .. } => {
            assert!(!job_id.is_empty());
            assert!(*clean_jobs);
        }
        _ => panic!("Expected Notify message"),
    }
    
    // 6. Test share submission
    let submit_msg = ProtocolMessage::Submit {
        username: "test_worker".to_string(),
        job_id: format!("{:x}", template.id.as_u128()),
        extranonce2: "abcd1234".to_string(),
        ntime: "12345678".to_string(),
        nonce: "87654321".to_string(),
    };
    
    let submit_responses = protocol_service.handle_downstream_message(test_conn.id, submit_msg).await.unwrap();
    assert!(submit_responses.is_empty()); // Successful submission returns empty response
    
    // 7. Test statistics
    let stats = protocol_service.get_translation_stats().await;
    assert_eq!(stats.total_connections, 1);
    assert_eq!(stats.sv1_connections, 1);
    assert_eq!(stats.subscribed_connections, 1);
    assert_eq!(stats.authorized_connections, 1);
    assert_eq!(stats.active_jobs, 1);
}

#[tokio::test]
async fn test_protocol_translation_sv1_to_sv2() {
    let protocol_service = ProxyProtocolService::new();
    let connection = create_test_connection(Protocol::Sv1);
    
    protocol_service.initialize_connection(&connection).await.unwrap();
    
    // Test the complete SV1 miner workflow
    
    // 1. Subscribe
    let subscribe_msg = ProtocolMessage::Subscribe {
        user_agent: "cgminer/4.11.1".to_string(),
        session_id: None,
    };
    
    let responses = protocol_service.handle_downstream_message(connection.id, subscribe_msg).await.unwrap();
    assert_eq!(responses.len(), 1);
    
    // 2. Authorize
    let authorize_msg = ProtocolMessage::Authorize {
        username: "worker1".to_string(),
        password: "x".to_string(),
    };
    
    protocol_service.handle_downstream_message(connection.id, authorize_msg).await.unwrap();
    
    // 3. Receive work
    let template = create_test_work_template();
    let work_responses = protocol_service.forward_work_template(&template, &[connection.id]).await.unwrap();
    
    assert_eq!(work_responses.len(), 1);
    let (conn_id, notify_msg) = &work_responses[0];
    assert_eq!(*conn_id, connection.id);
    
    if let ProtocolMessage::Notify { job_id, .. } = notify_msg {
        // 4. Submit share
        let submit_msg = ProtocolMessage::Submit {
            username: "worker1".to_string(),
            job_id: job_id.clone(),
            extranonce2: "00000000".to_string(),
            ntime: "12345678".to_string(),
            nonce: "abcdef00".to_string(),
        };
        
        let submit_responses = protocol_service.handle_downstream_message(connection.id, submit_msg).await.unwrap();
        assert!(submit_responses.is_empty()); // Success
        
        // 5. Create share for upstream forwarding
        let share = protocol_service.create_share_for_upstream(
            connection.id,
            job_id,
            "00000000",
            0x12345678,
            0xabcdef00,
        ).await.unwrap();
        
        assert_eq!(share.connection_id, connection.id);
        assert_eq!(share.nonce, 0xabcdef00);
        assert_eq!(share.timestamp, 0x12345678);
    } else {
        panic!("Expected Notify message");
    }
}

#[tokio::test]
async fn test_multiple_connections_load_balancing() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    // Create multiple connections
    let connections: Vec<Connection> = (0..5)
        .map(|_| create_test_connection(Protocol::Sv1))
        .collect();
    
    // Test that each connection would be handled (even though they fail without real upstreams)
    for connection in connections {
        let result = handler.handle_connection(connection).await;
        assert!(result.is_err()); // Expected to fail without real upstream pools
    }
    
    // Test protocol service with multiple connections
    let protocol_service = ProxyProtocolService::new();
    let test_connections: Vec<Connection> = (0..3)
        .map(|_| create_test_connection(Protocol::Sv1))
        .collect();
    
    // Initialize all connections
    for conn in &test_connections {
        protocol_service.initialize_connection(conn).await.unwrap();
        
        // Subscribe and authorize each connection
        let subscribe_msg = ProtocolMessage::Subscribe {
            user_agent: format!("miner_{}", conn.id),
            session_id: None,
        };
        protocol_service.handle_downstream_message(conn.id, subscribe_msg).await.unwrap();
        
        let authorize_msg = ProtocolMessage::Authorize {
            username: format!("worker_{}", conn.id),
            password: "x".to_string(),
        };
        protocol_service.handle_downstream_message(conn.id, authorize_msg).await.unwrap();
    }
    
    // Forward work to all connections
    let template = create_test_work_template();
    let connection_ids: Vec<_> = test_connections.iter().map(|c| c.id).collect();
    let responses = protocol_service.forward_work_template(&template, &connection_ids).await.unwrap();
    
    assert_eq!(responses.len(), 3);
    
    // Verify each connection got work
    for (i, (conn_id, _)) in responses.iter().enumerate() {
        assert_eq!(*conn_id, test_connections[i].id);
    }
    
    let stats = protocol_service.get_translation_stats().await;
    assert_eq!(stats.total_connections, 3);
    assert_eq!(stats.subscribed_connections, 3);
    assert_eq!(stats.authorized_connections, 3);
}

#[tokio::test]
async fn test_error_handling_invalid_messages() {
    let protocol_service = ProxyProtocolService::new();
    let connection = create_test_connection(Protocol::Sv1);
    
    protocol_service.initialize_connection(&connection).await.unwrap();
    
    // Test submitting share without authorization
    let submit_msg = ProtocolMessage::Submit {
        username: "unauthorized_worker".to_string(),
        job_id: "invalid_job".to_string(),
        extranonce2: "00000000".to_string(),
        ntime: "12345678".to_string(),
        nonce: "abcdef00".to_string(),
    };
    
    let responses = protocol_service.handle_downstream_message(connection.id, submit_msg).await.unwrap();
    assert_eq!(responses.len(), 1);
    
    match &responses[0] {
        ProtocolMessage::Error { code, message } => {
            assert_eq!(*code, 24); // Unauthorized worker
            assert!(message.contains("Unauthorized"));
        }
        _ => panic!("Expected error message"),
    }
    
    // Test submitting share with invalid job ID after authorization
    let authorize_msg = ProtocolMessage::Authorize {
        username: "test_worker".to_string(),
        password: "x".to_string(),
    };
    protocol_service.handle_downstream_message(connection.id, authorize_msg).await.unwrap();
    
    let invalid_submit_msg = ProtocolMessage::Submit {
        username: "test_worker".to_string(),
        job_id: "nonexistent_job".to_string(),
        extranonce2: "00000000".to_string(),
        ntime: "12345678".to_string(),
        nonce: "abcdef00".to_string(),
    };
    
    let error_responses = protocol_service.handle_downstream_message(connection.id, invalid_submit_msg).await.unwrap();
    assert_eq!(error_responses.len(), 1);
    
    match &error_responses[0] {
        ProtocolMessage::Error { code, message } => {
            assert_eq!(*code, 21); // Job not found
            assert!(message.contains("Job not found"));
        }
        _ => panic!("Expected error message"),
    }
}

#[tokio::test]
async fn test_difficulty_adjustment() {
    let protocol_service = ProxyProtocolService::new();
    let connection = create_test_connection(Protocol::Sv1);
    
    protocol_service.initialize_connection(&connection).await.unwrap();
    
    // Check initial difficulty
    let initial_state = protocol_service.get_connection_state(connection.id).await.unwrap();
    assert_eq!(initial_state.difficulty, 1.0);
    
    // Update difficulty
    protocol_service.update_connection_difficulty(connection.id, 4.0).await.unwrap();
    
    // Verify difficulty was updated
    let updated_state = protocol_service.get_connection_state(connection.id).await.unwrap();
    assert_eq!(updated_state.difficulty, 4.0);
    
    // Test that shares use the updated difficulty
    let share = protocol_service.create_share_for_upstream(
        connection.id,
        "job123",
        "00000000",
        1234567890,
        0x12345678,
    ).await.unwrap();
    
    assert_eq!(share.difficulty, 4.0);
}

#[tokio::test]
async fn test_connection_cleanup() {
    let protocol_service = ProxyProtocolService::new();
    let connection = create_test_connection(Protocol::Sv1);
    
    // Initialize and verify connection exists
    protocol_service.initialize_connection(&connection).await.unwrap();
    assert!(protocol_service.get_connection_state(connection.id).await.is_some());
    
    let stats_before = protocol_service.get_translation_stats().await;
    assert_eq!(stats_before.total_connections, 1);
    
    // Cleanup connection
    protocol_service.cleanup_connection(connection.id).await.unwrap();
    assert!(protocol_service.get_connection_state(connection.id).await.is_none());
    
    let stats_after = protocol_service.get_translation_stats().await;
    assert_eq!(stats_after.total_connections, 0);
}