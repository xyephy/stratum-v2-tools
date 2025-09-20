use sv2_core::{
    database::{DatabasePool, DatabaseOps, ShareStats, ConfigHistoryEntry},
    types::{ConnectionInfo, Share, WorkTemplate, Alert, AlertLevel, PerformanceMetrics, Protocol, ConnectionState},
    Result,
};
use tempfile::tempdir;
use uuid::Uuid;
use bitcoin::{BlockHash, Transaction, TxIn, TxOut};
use std::str::FromStr;

async fn create_test_database() -> Result<DatabasePool> {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db_url = format!("sqlite://{}", db_path.display());
    
    let pool = DatabasePool::new(&db_url, 5).await?;
    pool.migrate().await?;
    Ok(pool)
}

fn create_test_connection_info() -> ConnectionInfo {
    ConnectionInfo {
        id: Uuid::new_v4(),
        address: "127.0.0.1:3333".parse().unwrap(),
        protocol: Protocol::Sv2,
        state: ConnectionState::Connected,
        connected_at: chrono::Utc::now(),
        last_activity: chrono::Utc::now(),
        user_agent: Some("test-miner/1.0".to_string()),
        version: Some("1.0.0".to_string()),
        subscribed_difficulty: Some(1.0),
        extranonce1: Some("abcd1234".to_string()),
        extranonce2_size: Some(4),
        authorized_workers: vec!["worker1".to_string()],
        total_shares: 10,
        valid_shares: 9,
        invalid_shares: 1,
        blocks_found: 0,
    }
}

fn create_test_share(connection_id: Uuid) -> Share {
    Share {
        connection_id,
        nonce: 12345,
        timestamp: chrono::Utc::now().timestamp() as u32,
        difficulty: 1.0,
        is_valid: true,
        block_hash: None,
        submitted_at: chrono::Utc::now(),
    }
}

fn create_test_work_template() -> WorkTemplate {
    let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
    let coinbase_tx = Transaction {
        version: 1,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![TxIn::default()],
        output: vec![TxOut::default()],
    };
    
    WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0)
}

fn create_test_alert() -> Alert {
    Alert::new(
        AlertLevel::Warning,
        "Test Alert".to_string(),
        "This is a test alert message".to_string(),
        "database_test".to_string(),
    )
}

fn create_test_performance_metrics() -> PerformanceMetrics {
    PerformanceMetrics {
        cpu_usage: 25.5,
        memory_usage: 1024 * 1024 * 512, // 512 MB
        memory_total: 1024 * 1024 * 1024 * 8, // 8 GB
        network_rx_bytes: 1024 * 100,
        network_tx_bytes: 1024 * 50,
        disk_usage: 1024 * 1024 * 1024 * 10, // 10 GB
        disk_total: 1024 * 1024 * 1024 * 100, // 100 GB
        open_connections: 5,
        database_connections: 2,
        timestamp: chrono::Utc::now(),
    }
}

#[tokio::test]
async fn test_connection_crud_operations() {
    let db = create_test_database().await.unwrap();
    let conn_info = create_test_connection_info();
    
    // Test create
    db.create_connection(&conn_info).await.unwrap();
    
    // Test get
    let retrieved = db.get_connection(conn_info.id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, conn_info.id);
    assert_eq!(retrieved.address, conn_info.address);
    assert_eq!(retrieved.protocol, conn_info.protocol);
    
    // Test update
    let mut updated_conn = conn_info.clone();
    updated_conn.total_shares = 20;
    updated_conn.valid_shares = 18;
    db.update_connection(&updated_conn).await.unwrap();
    
    let retrieved = db.get_connection(conn_info.id).await.unwrap().unwrap();
    assert_eq!(retrieved.total_shares, 20);
    assert_eq!(retrieved.valid_shares, 18);
    
    // Test list
    let connections = db.list_connections(Some(10)).await.unwrap();
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].id, conn_info.id);
    
    // Test delete
    db.delete_connection(conn_info.id).await.unwrap();
    let retrieved = db.get_connection(conn_info.id).await.unwrap();
    assert!(retrieved.is_none());
}

#[tokio::test]
async fn test_share_operations() {
    let db = create_test_database().await.unwrap();
    let conn_info = create_test_connection_info();
    db.create_connection(&conn_info).await.unwrap();
    
    let share1 = create_test_share(conn_info.id);
    let mut share2 = create_test_share(conn_info.id);
    share2.is_valid = false;
    let mut share3 = create_test_share(conn_info.id);
    share3.block_hash = Some(BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000001").unwrap());
    
    // Test create shares
    db.create_share(&share1).await.unwrap();
    db.create_share(&share2).await.unwrap();
    db.create_share(&share3).await.unwrap();
    
    // Test get shares
    let shares = db.get_shares(Some(conn_info.id), Some(10)).await.unwrap();
    assert_eq!(shares.len(), 3);
    
    let all_shares = db.get_shares(None, Some(10)).await.unwrap();
    assert_eq!(all_shares.len(), 3);
    
    // Test share stats
    let stats = db.get_share_stats(Some(conn_info.id)).await.unwrap();
    assert_eq!(stats.total_shares, 3);
    assert_eq!(stats.valid_shares, 2);
    assert_eq!(stats.invalid_shares, 1);
    assert_eq!(stats.blocks_found, 1);
    assert!((stats.acceptance_rate - 66.66666666666667).abs() < 0.01);
    
    let global_stats = db.get_share_stats(None).await.unwrap();
    assert_eq!(global_stats.total_shares, 3);
}

#[tokio::test]
async fn test_work_template_operations() {
    let db = create_test_database().await.unwrap();
    let template = create_test_work_template();
    
    // Test create
    db.create_work_template(&template).await.unwrap();
    
    // Test get
    let retrieved = db.get_work_template(template.id).await.unwrap();
    assert!(retrieved.is_some());
    let retrieved = retrieved.unwrap();
    assert_eq!(retrieved.id, template.id);
    assert_eq!(retrieved.previous_hash, template.previous_hash);
    assert_eq!(retrieved.difficulty, template.difficulty);
    
    // Test list
    let templates = db.list_work_templates(Some(10)).await.unwrap();
    assert_eq!(templates.len(), 1);
    assert_eq!(templates[0].id, template.id);
    
    // Test delete expired (should not delete as template is not expired)
    let deleted_count = db.delete_expired_templates().await.unwrap();
    assert_eq!(deleted_count, 0);
    
    let templates = db.list_work_templates(Some(10)).await.unwrap();
    assert_eq!(templates.len(), 1);
}

#[tokio::test]
async fn test_alert_operations() {
    let db = create_test_database().await.unwrap();
    let mut alert = create_test_alert();
    
    // Test create
    db.create_alert(&alert).await.unwrap();
    
    // Test get unresolved alerts
    let unresolved = db.get_alerts(Some(false), Some(10)).await.unwrap();
    assert_eq!(unresolved.len(), 1);
    assert_eq!(unresolved[0].id, alert.id);
    assert!(!unresolved[0].is_resolved());
    
    // Test update (resolve alert)
    alert.resolve();
    db.update_alert(&alert).await.unwrap();
    
    // Test get resolved alerts
    let resolved = db.get_alerts(Some(true), Some(10)).await.unwrap();
    assert_eq!(resolved.len(), 1);
    assert!(resolved[0].is_resolved());
    
    // Test get all alerts
    let all_alerts = db.get_alerts(None, Some(10)).await.unwrap();
    assert_eq!(all_alerts.len(), 1);
}

#[tokio::test]
async fn test_performance_metrics_operations() {
    let db = create_test_database().await.unwrap();
    let metrics1 = create_test_performance_metrics();
    let mut metrics2 = create_test_performance_metrics();
    metrics2.cpu_usage = 50.0;
    metrics2.timestamp = chrono::Utc::now() + chrono::Duration::minutes(1);
    
    // Test store metrics
    db.store_performance_metrics(&metrics1).await.unwrap();
    db.store_performance_metrics(&metrics2).await.unwrap();
    
    // Test get metrics
    let retrieved = db.get_performance_metrics(Some(10)).await.unwrap();
    assert_eq!(retrieved.len(), 2);
    
    // Should be ordered by timestamp DESC
    assert_eq!(retrieved[0].cpu_usage, 50.0);
    assert_eq!(retrieved[1].cpu_usage, 25.5);
    
    // Test limited results
    let limited = db.get_performance_metrics(Some(1)).await.unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].cpu_usage, 50.0);
}

#[tokio::test]
async fn test_config_history_operations() {
    let db = create_test_database().await.unwrap();
    
    let config1 = r#"{"mode": "solo", "bitcoin_rpc": "http://localhost:8332"}"#;
    let config2 = r#"{"mode": "pool", "listen_address": "0.0.0.0:3333"}"#;
    
    // Test store config history
    db.store_config_history(config1, "admin").await.unwrap();
    tokio::time::sleep(tokio::time::Duration::from_millis(10)).await; // Ensure different timestamps
    db.store_config_history(config2, "user").await.unwrap();
    
    // Test get config history
    let history = db.get_config_history(Some(10)).await.unwrap();
    assert_eq!(history.len(), 2);
    
    // Should be ordered by applied_at DESC
    assert_eq!(history[0].config_data, config2);
    assert_eq!(history[0].applied_by, "user");
    assert_eq!(history[1].config_data, config1);
    assert_eq!(history[1].applied_by, "admin");
    
    // Test limited results
    let limited = db.get_config_history(Some(1)).await.unwrap();
    assert_eq!(limited.len(), 1);
    assert_eq!(limited[0].config_data, config2);
}

#[tokio::test]
async fn test_connection_status_update() {
    let db = create_test_database().await.unwrap();
    let conn_info = create_test_connection_info();
    db.create_connection(&conn_info).await.unwrap();
    
    // Test update connection status
    db.update_connection_status(conn_info.id, ConnectionState::Disconnected).await.unwrap();
    
    let retrieved = db.get_connection(conn_info.id).await.unwrap().unwrap();
    assert!(matches!(retrieved.state, ConnectionState::Disconnected));
}

#[tokio::test]
async fn test_database_health_check() {
    let db = create_test_database().await.unwrap();
    
    // Health check should pass for a working database
    db.health_check().await.unwrap();
}

#[tokio::test]
async fn test_database_stats() {
    let db = create_test_database().await.unwrap();
    
    // Initially empty
    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.total_connections, 0);
    assert_eq!(stats.total_shares, 0);
    
    // Add some data
    let conn_info = create_test_connection_info();
    db.create_connection(&conn_info).await.unwrap();
    
    let share = create_test_share(conn_info.id);
    db.create_share(&share).await.unwrap();
    
    // Check updated stats
    let stats = db.get_stats().await.unwrap();
    assert_eq!(stats.total_connections, 1);
    assert_eq!(stats.total_shares, 1);
}

#[tokio::test]
async fn test_concurrent_operations() {
    let db = create_test_database().await.unwrap();
    
    // Test concurrent connection creation
    let mut handles = Vec::new();
    for i in 0..10 {
        let db_clone = db.clone();
        let handle = tokio::spawn(async move {
            let mut conn_info = create_test_connection_info();
            conn_info.id = Uuid::new_v4();
            conn_info.address = format!("127.0.0.1:{}", 3333 + i).parse().unwrap();
            db_clone.create_connection(&conn_info).await.unwrap();
            conn_info.id
        });
        handles.push(handle);
    }
    
    let mut connection_ids = Vec::new();
    for handle in handles {
        connection_ids.push(handle.await.unwrap());
    }
    
    // Verify all connections were created
    let connections = db.list_connections(Some(20)).await.unwrap();
    assert_eq!(connections.len(), 10);
    
    // Test concurrent share creation
    let mut handles = Vec::new();
    for connection_id in connection_ids {
        let db_clone = db.clone();
        let handle = tokio::spawn(async move {
            for _ in 0..5 {
                let share = create_test_share(connection_id);
                db_clone.create_share(&share).await.unwrap();
            }
        });
        handles.push(handle);
    }
    
    for handle in handles {
        handle.await.unwrap();
    }
    
    // Verify all shares were created
    let shares = db.get_shares(None, Some(100)).await.unwrap();
    assert_eq!(shares.len(), 50); // 10 connections * 5 shares each
    
    let stats = db.get_share_stats(None).await.unwrap();
    assert_eq!(stats.total_shares, 50);
}

#[tokio::test]
async fn test_large_data_handling() {
    let db = create_test_database().await.unwrap();
    
    // Create a connection
    let conn_info = create_test_connection_info();
    db.create_connection(&conn_info).await.unwrap();
    
    // Create many shares
    for i in 0..1000 {
        let mut share = create_test_share(conn_info.id);
        share.nonce = i;
        share.is_valid = i % 10 != 0; // 90% valid shares
        if i % 100 == 0 {
            share.block_hash = Some(BlockHash::from_str(&format!("{:064x}", i)).unwrap());
        }
        db.create_share(&share).await.unwrap();
    }
    
    // Test pagination
    let first_page = db.get_shares(Some(conn_info.id), Some(100)).await.unwrap();
    assert_eq!(first_page.len(), 100);
    
    let stats = db.get_share_stats(Some(conn_info.id)).await.unwrap();
    assert_eq!(stats.total_shares, 1000);
    assert_eq!(stats.valid_shares, 900);
    assert_eq!(stats.invalid_shares, 100);
    assert_eq!(stats.blocks_found, 10);
    assert_eq!(stats.acceptance_rate, 90.0);
}

#[tokio::test]
async fn test_error_handling() {
    let db = create_test_database().await.unwrap();
    
    // Test getting non-existent connection
    let non_existent_id = Uuid::new_v4();
    let result = db.get_connection(non_existent_id).await.unwrap();
    assert!(result.is_none());
    
    // Test getting non-existent work template
    let result = db.get_work_template(non_existent_id).await.unwrap();
    assert!(result.is_none());
    
    // Test share stats for non-existent connection
    let stats = db.get_share_stats(Some(non_existent_id)).await.unwrap();
    assert_eq!(stats.total_shares, 0);
    assert_eq!(stats.acceptance_rate, 0.0);
}

#[tokio::test]
async fn test_data_integrity() {
    let db = create_test_database().await.unwrap();
    
    // Create connection and shares
    let conn_info = create_test_connection_info();
    db.create_connection(&conn_info).await.unwrap();
    
    let share = create_test_share(conn_info.id);
    db.create_share(&share).await.unwrap();
    
    // Delete connection (should cascade delete shares due to foreign key)
    db.delete_connection(conn_info.id).await.unwrap();
    
    // Verify shares are also deleted
    let shares = db.get_shares(Some(conn_info.id), None).await.unwrap();
    assert_eq!(shares.len(), 0);
}

#[cfg(test)]
mod mock_tests {
    use super::*;
    use sv2_core::database::MockDatabaseOps;

    #[tokio::test]
    async fn test_mock_database_operations() {
        let mock_db = MockDatabaseOps::new();
        let conn_info = create_test_connection_info();
        
        // Test mock connection operations
        mock_db.create_connection(&conn_info).await.unwrap();
        
        let retrieved = mock_db.get_connection(conn_info.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, conn_info.id);
        
        let connections = mock_db.list_connections(Some(10)).await.unwrap();
        assert_eq!(connections.len(), 1);
        
        // Test mock share operations
        let share = create_test_share(conn_info.id);
        mock_db.create_share(&share).await.unwrap();
        
        let shares = mock_db.get_shares(Some(conn_info.id), Some(10)).await.unwrap();
        assert_eq!(shares.len(), 1);
        
        let stats = mock_db.get_share_stats(Some(conn_info.id)).await.unwrap();
        assert_eq!(stats.total_shares, 1);
        assert_eq!(stats.valid_shares, 1);
        assert_eq!(stats.acceptance_rate, 100.0);
        
        // Test mock template operations
        let template = create_test_work_template();
        mock_db.create_work_template(&template).await.unwrap();
        
        let retrieved = mock_db.get_work_template(template.id).await.unwrap();
        assert!(retrieved.is_some());
        assert_eq!(retrieved.unwrap().id, template.id);
    }
}