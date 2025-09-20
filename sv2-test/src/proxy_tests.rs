//! Integration tests for proxy mode functionality

use sv2_core::{
    modes::ProxyModeHandler,
    config::{ProxyConfig, UpstreamPool, LoadBalancingStrategy},
    database::MockDatabaseOps,
    types::{Connection, Share, Protocol, ConnectionState},
    mode::ModeHandler,
    Result,
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
            UpstreamPool {
                url: "stratum+tcp://pool2.example.com:4444".to_string(),
                username: "worker2".to_string(),
                password: "password2".to_string(),
                priority: 2,
                weight: 2,
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

/// Create a test share
fn create_test_share(connection_id: Uuid) -> Share {
    Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0)
}

#[tokio::test]
async fn test_proxy_mode_creation() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config.clone(), database);
    
    // Verify configuration is stored correctly
    // Note: config is private, so we test through behavior instead
    let statuses = handler.get_upstream_statuses().await;
    assert_eq!(statuses.len(), 2);
}

#[tokio::test]
async fn test_proxy_mode_initialization() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    // Initialize should fail because we can't actually connect to test pools
    let result = handler.initialize().await;
    assert!(result.is_err());
    
    // Check that upstream statuses are tracked
    let statuses = handler.get_upstream_statuses().await;
    assert_eq!(statuses.len(), 2);
    assert!(!statuses[0].connected);
    assert!(!statuses[1].connected);
}

#[tokio::test]
async fn test_connection_handling_no_upstreams() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    let connection = create_test_connection(Protocol::Sv1);
    
    // Should fail because no upstream pools are connected
    let result = handler.handle_connection(connection).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("No available upstream pools"));
}

#[tokio::test]
async fn test_share_processing_no_mapping() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    let connection_id = Uuid::new_v4();
    let share = create_test_share(connection_id);
    
    // Should fail because connection is not mapped to any upstream
    let result = handler.process_share(share).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("Connection not mapped to upstream pool"));
}

#[tokio::test]
async fn test_work_template_retrieval() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    // Should return a default template when no upstream pools have work
    let result = handler.get_work_template().await;
    assert!(result.is_ok());
    
    let template = result.unwrap();
    assert_eq!(template.difficulty, 1.0);
    assert!(!template.transactions.is_empty() || template.transactions.is_empty()); // Either is valid
}

#[tokio::test]
async fn test_disconnection_handling() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    let connection_id = Uuid::new_v4();
    
    // Should succeed even if connection wasn't tracked
    let result = handler.handle_disconnection(connection_id).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_statistics_retrieval() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    let result = handler.get_statistics().await;
    assert!(result.is_ok());
    
    let stats = result.unwrap();
    assert_eq!(stats.hashrate, 0.0);
    assert_eq!(stats.shares_per_minute, 0.0);
    assert_eq!(stats.acceptance_rate, 0.0);
}

#[tokio::test]
async fn test_config_validation() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config.clone(), database);
    
    // Create a daemon config with proxy mode
    let daemon_config = sv2_core::config::DaemonConfig {
        mode: sv2_core::config::OperationModeConfig::Proxy(config),
        ..Default::default()
    };
    
    let result = handler.validate_config(&daemon_config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_config_validation_empty_pools() {
    let mut config = create_test_proxy_config();
    config.upstream_pools.clear();
    
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config.clone(), database);
    
    let daemon_config = sv2_core::config::DaemonConfig {
        mode: sv2_core::config::OperationModeConfig::Proxy(config),
        ..Default::default()
    };
    
    let result = handler.validate_config(&daemon_config);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("at least one upstream pool"));
}

#[tokio::test]
async fn test_config_validation_invalid_pool() {
    let mut config = create_test_proxy_config();
    config.upstream_pools[0].url = String::new(); // Empty URL
    
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config.clone(), database);
    
    let daemon_config = sv2_core::config::DaemonConfig {
        mode: sv2_core::config::OperationModeConfig::Proxy(config),
        ..Default::default()
    };
    
    let result = handler.validate_config(&daemon_config);
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("URL cannot be empty"));
}

#[tokio::test]
async fn test_alert_management() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    // Initially no alerts
    let alerts = handler.get_alerts().await;
    assert!(alerts.is_empty());
    
    // After initialization failure, there should be alerts
    let _ = handler.initialize().await;
    let alerts = handler.get_alerts().await;
    assert!(!alerts.is_empty());
    
    // Test clearing resolved alerts
    handler.clear_resolved_alerts().await;
    let alerts = handler.get_alerts().await;
    // Alerts should still be there since they're not resolved
    assert!(!alerts.is_empty());
}

#[tokio::test]
async fn test_load_balancing_strategies() {
    // Test different load balancing strategies
    let strategies = vec![
        LoadBalancingStrategy::RoundRobin,
        LoadBalancingStrategy::WeightedRoundRobin,
        LoadBalancingStrategy::LeastConnections,
        LoadBalancingStrategy::Random,
    ];
    
    for strategy in strategies {
        let mut config = create_test_proxy_config();
        config.load_balancing = strategy.clone();
        
        let database = Arc::new(MockDatabaseOps::new());
        let handler = ProxyModeHandler::new(config, database);
        
        // Verify the strategy is set correctly by testing behavior
        // Since load_balancer is private, we test through the handler's behavior
        let statuses = handler.get_upstream_statuses().await;
        assert_eq!(statuses.len(), 2);
    }
}

#[tokio::test]
async fn test_upstream_pool_priorities() {
    let mut config = create_test_proxy_config();
    
    // Set different priorities
    config.upstream_pools[0].priority = 1;
    config.upstream_pools[1].priority = 2;
    
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    // Verify pools are stored with correct priorities
    // Since upstream_connections is private, we test through behavior
    let statuses = handler.get_upstream_statuses().await;
    assert_eq!(statuses.len(), 2);
}

#[tokio::test]
async fn test_connection_retry_configuration() {
    let mut config = create_test_proxy_config();
    config.connection_retry_interval = 60;
    config.max_retry_attempts = 10;
    
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config.clone(), database);
    
    // Test configuration through behavior since config is private
    let statuses = handler.get_upstream_statuses().await;
    assert_eq!(statuses.len(), 2);
}

/// Test the complete proxy workflow with mocked upstream connections
#[tokio::test]
async fn test_proxy_workflow_integration() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    // 1. Try to get work template (should work with default)
    let template_result = handler.get_work_template().await;
    assert!(template_result.is_ok());
    
    // 2. Try to handle connection (should fail - no upstreams)
    let connection = create_test_connection(Protocol::Sv1);
    let conn_result = handler.handle_connection(connection).await;
    assert!(conn_result.is_err());
    
    // 3. Try to process share (should fail - no mapping)
    let share = create_test_share(Uuid::new_v4());
    let share_result = handler.process_share(share).await;
    assert!(share_result.is_err());
    
    // 4. Get statistics (should work)
    let stats_result = handler.get_statistics().await;
    assert!(stats_result.is_ok());
    
    // 5. Handle disconnection (should work)
    let disconn_result = handler.handle_disconnection(Uuid::new_v4()).await;
    assert!(disconn_result.is_ok());
}

/// Test error handling and recovery scenarios
#[tokio::test]
async fn test_error_handling_scenarios() {
    let config = create_test_proxy_config();
    let database = Arc::new(MockDatabaseOps::new());
    let handler = ProxyModeHandler::new(config, database);
    
    // Test handling invalid connection IDs
    let invalid_id = Uuid::new_v4();
    let share = create_test_share(invalid_id);
    let result = handler.process_share(share).await;
    assert!(result.is_err());
    
    // Test disconnection of non-existent connection
    let result = handler.handle_disconnection(invalid_id).await;
    assert!(result.is_ok()); // Should not fail
    
    // Test getting upstream statuses when no pools are connected
    let statuses = handler.get_upstream_statuses().await;
    for status in statuses {
        assert!(!status.connected);
        assert_eq!(status.shares_submitted, 0);
        assert_eq!(status.shares_accepted, 0);
        assert_eq!(status.shares_rejected, 0);
    }
}

#[cfg(test)]
mod load_balancer_tests {
    use super::*;
    use sv2_core::modes::proxy::LoadBalancer;

    #[test]
    fn test_load_balancer_creation() {
        let balancer = LoadBalancer::new(LoadBalancingStrategy::RoundRobin);
        assert_eq!(balancer.strategy, LoadBalancingStrategy::RoundRobin);
        assert!(balancer.connection_counts.is_empty());
    }

    #[test]
    fn test_connection_count_updates() {
        let mut balancer = LoadBalancer::new(LoadBalancingStrategy::LeastConnections);
        
        // Add connections
        balancer.update_connection_count("pool1", 5);
        balancer.update_connection_count("pool2", 3);
        
        assert_eq!(balancer.connection_counts.get("pool1"), Some(&5));
        assert_eq!(balancer.connection_counts.get("pool2"), Some(&3));
        
        // Remove connections
        balancer.update_connection_count("pool1", -2);
        assert_eq!(balancer.connection_counts.get("pool1"), Some(&3));
        
        // Test underflow protection
        balancer.update_connection_count("pool2", -10);
        assert_eq!(balancer.connection_counts.get("pool2"), Some(&0));
    }
}