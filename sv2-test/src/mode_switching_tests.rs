use sv2_core::{
    Result,
    mode_factory::{ModeHandlerFactory, ModeRouter, ModeState},
    config::{DaemonConfig, OperationModeConfig, SoloConfig, PoolConfig, NetworkConfig, DatabaseConfig, BitcoinConfig, MonitoringConfig, LoggingConfig, SecurityConfig, MetricsConfig, HealthConfig, AlertThresholds, LogFormat, LogOutput},
    database::DatabasePool,
};
use std::collections::HashMap;
use tempfile::tempdir;
use std::sync::Arc;

fn create_test_database_config() -> DatabaseConfig {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    DatabaseConfig {
        url: format!("sqlite://{}", db_path.display()),
        max_connections: 5,
        connection_timeout: 30,
        enable_migrations: true,
    }
}

fn create_test_config(mode: OperationModeConfig) -> DaemonConfig {
    DaemonConfig {
        mode,
        network: NetworkConfig {
            bind_address: "127.0.0.1:0".parse().unwrap(),
            max_connections: 100,
            connection_timeout: 30,
            keepalive_interval: 60,
        },
        bitcoin: BitcoinConfig {
            rpc_url: "http://localhost:18443".to_string(),
            rpc_user: "test".to_string(),
            rpc_password: "test".to_string(),
            network: sv2_core::config::BitcoinNetwork::Regtest,
            coinbase_address: Some("bcrt1qtest".to_string()),
            block_template_timeout: 30,
        },
        database: create_test_database_config(),
        monitoring: MonitoringConfig {
            enable_metrics: true,
            metrics_bind_address: "127.0.0.1:0".parse().unwrap(),
            enable_health_checks: true,
            health_check_interval: 60,
            metrics: MetricsConfig {
                enabled: true,
                collection_interval: 60,
                prometheus_port: 9090,
                system_monitoring: true,
                labels: HashMap::new(),
            },
            health: HealthConfig {
                enabled: true,
                check_interval: 60,
                check_timeout: 10,
                alert_thresholds: AlertThresholds {
                    cpu_usage: 80.0,
                    memory_usage: 80.0,
                    connection_count: 1000,
                    rejection_rate: 10.0,
                    response_time: 1000,
                    database_connections: 10,
                },
            },
        },
        logging: LoggingConfig {
            level: "info".to_string(),
            component_levels: HashMap::new(),
            format: LogFormat::Pretty,
            output: LogOutput::Stdout,
            enable_correlation_ids: false,
            redact_sensitive_data: true,
            max_file_size_mb: None,
            max_files: None,
        },
        security: SecurityConfig {
            enable_authentication: false,
            api_key: None,
            rate_limit_per_minute: 60,
            enable_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
            auth: sv2_core::auth::AuthConfig::default(),
        },
    }
}

async fn create_test_database() -> Result<Arc<DatabasePool>> {
    let dir = tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db_url = format!("sqlite://{}", db_path.display());
    
    let pool = DatabasePool::new(&db_url, 5).await?;
    pool.migrate().await?;
    Ok(Arc::new(pool))
}

#[tokio::test]
async fn test_mode_handler_factory_create_solo() {
    let config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let database = create_test_database().await.unwrap();
    
    let handler = ModeHandlerFactory::create_handler(&config, database);
    assert!(handler.is_ok());
}

#[tokio::test]
async fn test_mode_handler_factory_create_pool() {
    let config = create_test_config(OperationModeConfig::Pool(PoolConfig::default()));
    let database = create_test_database().await.unwrap();
    
    let handler = ModeHandlerFactory::create_handler(&config, database);
    assert!(handler.is_ok());
}

#[tokio::test]
async fn test_validate_allowed_mode_transitions() {
    let solo_config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let pool_config = create_test_config(OperationModeConfig::Pool(PoolConfig::default()));

    // Solo to Pool should be allowed
    let result = ModeHandlerFactory::validate_mode_switch(&solo_config, &pool_config);
    assert!(result.is_ok());

    // Pool to Solo should be allowed
    let result = ModeHandlerFactory::validate_mode_switch(&pool_config, &solo_config);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_disallowed_mode_transitions() {
    let solo_config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let proxy_config = create_test_config(OperationModeConfig::Proxy(sv2_core::config::ProxyConfig::default()));

    // Solo to Proxy should not be allowed
    let result = ModeHandlerFactory::validate_mode_switch(&solo_config, &proxy_config);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validate_same_mode_transition() {
    let solo_config1 = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let solo_config2 = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));

    // Same mode should always be allowed
    let result = ModeHandlerFactory::validate_mode_switch(&solo_config1, &solo_config2);
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_validate_config_compatibility() {
    let config1 = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let mut config2 = config1.clone();
    
    // Same config should be compatible
    let result = ModeHandlerFactory::validate_config_compatibility(&config1, &config2);
    assert!(result.is_ok());

    // Different database config should not be compatible
    config2.database.url = "sqlite:///different.db".to_string();
    let result = ModeHandlerFactory::validate_config_compatibility(&config1, &config2);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_validate_bind_address_compatibility() {
    let config1 = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let mut config2 = config1.clone();
    
    // Different bind address should not be compatible
    config2.network.bind_address = "127.0.0.1:9999".parse().unwrap();
    let result = ModeHandlerFactory::validate_config_compatibility(&config1, &config2);
    assert!(result.is_err());
}

#[tokio::test]
async fn test_mode_router_initialization() {
    let database = create_test_database().await.unwrap();
    let config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    
    let mut router = ModeRouter::new(database);
    let result = router.initialize(config).await;
    assert!(result.is_ok());
    
    // Should have a handler and config after initialization
    assert!(router.get_handler().is_some());
    assert!(router.get_config().is_some());
    
    // Cleanup
    router.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_mode_router_switch_allowed() {
    let database = create_test_database().await.unwrap();
    let solo_config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let pool_config = create_test_config(OperationModeConfig::Pool(PoolConfig::default()));
    
    let mut router = ModeRouter::new(database);
    
    // Initialize with solo mode
    router.initialize(solo_config).await.unwrap();
    assert!(router.get_config().unwrap().get_mode_type() == sv2_core::mode::OperationMode::Solo);
    
    // Switch to pool mode (should be allowed)
    let result = router.switch_mode(pool_config).await;
    assert!(result.is_ok());
    assert!(router.get_config().unwrap().get_mode_type() == sv2_core::mode::OperationMode::Pool);
    
    // Cleanup
    router.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_mode_router_switch_disallowed() {
    let database = create_test_database().await.unwrap();
    let solo_config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let proxy_config = create_test_config(OperationModeConfig::Proxy(sv2_core::config::ProxyConfig::default()));
    
    let mut router = ModeRouter::new(database);
    
    // Initialize with solo mode
    router.initialize(solo_config).await.unwrap();
    
    // Try to switch to proxy mode (should be disallowed)
    let result = router.switch_mode(proxy_config).await;
    assert!(result.is_err());
    
    // Should still be in solo mode
    assert!(router.get_config().unwrap().get_mode_type() == sv2_core::mode::OperationMode::Solo);
    
    // Cleanup
    router.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_mode_router_update_config_same_mode() {
    let database = create_test_database().await.unwrap();
    let solo_config1 = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let mut solo_config2 = solo_config1.clone();
    
    // Modify some non-critical config
    solo_config2.bitcoin.rpc_timeout = 60;
    
    let mut router = ModeRouter::new(database);
    
    // Initialize with first config
    router.initialize(solo_config1).await.unwrap();
    
    // Update config (should work)
    let result = router.update_config(solo_config2).await;
    assert!(result.is_ok());
    
    // Should still be in solo mode with updated config
    assert!(router.get_config().unwrap().get_mode_type() == sv2_core::mode::OperationMode::Solo);
    assert_eq!(router.get_config().unwrap().bitcoin.rpc_timeout, 60);
    
    // Cleanup
    router.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_mode_router_update_config_mode_change() {
    let database = create_test_database().await.unwrap();
    let solo_config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    let pool_config = create_test_config(OperationModeConfig::Pool(PoolConfig::default()));
    
    let mut router = ModeRouter::new(database);
    
    // Initialize with solo mode
    router.initialize(solo_config).await.unwrap();
    
    // Update config with different mode (should trigger mode switch)
    let result = router.update_config(pool_config).await;
    assert!(result.is_ok());
    
    // Should now be in pool mode
    assert!(router.get_config().unwrap().get_mode_type() == sv2_core::mode::OperationMode::Pool);
    
    // Cleanup
    router.shutdown().await.unwrap();
}

#[tokio::test]
async fn test_mode_router_shutdown() {
    let database = create_test_database().await.unwrap();
    let config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    
    let mut router = ModeRouter::new(database);
    router.initialize(config).await.unwrap();
    
    // Should have handler before shutdown
    assert!(router.get_handler().is_some());
    assert!(router.get_config().is_some());
    
    // Shutdown
    router.shutdown().await.unwrap();
    
    // Should not have handler after shutdown
    assert!(router.get_handler().is_none());
    assert!(router.get_config().is_none());
}

#[tokio::test]
async fn test_mode_router_uninitialized_operations() {
    let database = create_test_database().await.unwrap();
    let config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    
    let mut router = ModeRouter::new(database);
    
    // Operations on uninitialized router should fail
    let result = router.switch_mode(config.clone()).await;
    assert!(result.is_err());
    
    let result = router.update_config(config).await;
    assert!(result.is_err());
    
    // Should not have handler or config
    assert!(router.get_handler().is_none());
    assert!(router.get_config().is_none());
}

#[tokio::test]
async fn test_mode_state_preservation() {
    // This test would require a more complex setup with actual connections and shares
    // For now, we'll just test that the state preservation functions don't panic
    
    let database = create_test_database().await.unwrap();
    let config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
    
    let handler = ModeHandlerFactory::create_handler(&config, Arc::clone(&database)).unwrap();
    
    // Test state preservation (should not panic)
    let result = ModeHandlerFactory::preserve_mode_state(handler.as_ref(), Arc::clone(&database)).await;
    assert!(result.is_ok());
    
    let state = result.unwrap();
    
    // Test state restoration (should not panic)
    let result = ModeHandlerFactory::restore_mode_state(handler.as_ref(), state, database).await;
    assert!(result.is_ok());
}