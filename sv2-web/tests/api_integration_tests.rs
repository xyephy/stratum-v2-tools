use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;
use uuid::Uuid;

use sv2_core::{
    config::DaemonConfig,
    database::{DatabasePool, DatabaseOps},
    types::{ConnectionInfo, Share, WorkTemplate, Alert, AlertLevel, Protocol, ConnectionState},
};
use sv2_web::handlers::{AppState, ApiError};

async fn setup_test_app() -> (Router, Arc<dyn DatabaseOps>) {
    // Use in-memory SQLite for testing
    let database = DatabasePool::new("sqlite::memory:", 10).await.unwrap();
    database.migrate().await.unwrap();
    
    let config = Arc::new(tokio::sync::RwLock::new(DaemonConfig::default()));
    
    let app_state = AppState {
        database: Arc::new(database.clone()) as Arc<dyn DatabaseOps>,
        config,
    };

    let app = Router::new()
        .route("/api/v1/status", axum::routing::get(sv2_web::handlers::get_status))
        .route("/api/v1/health", axum::routing::get(sv2_web::handlers::health_check))
        .route("/api/v1/connections", axum::routing::get(sv2_web::handlers::get_connections))
        .route("/api/v1/connections/:id", axum::routing::get(sv2_web::handlers::get_connection))
        .route("/api/v1/shares", axum::routing::get(sv2_web::handlers::get_shares))
        .route("/api/v1/shares/stats", axum::routing::get(sv2_web::handlers::get_share_stats))
        .route("/api/v1/metrics", axum::routing::get(sv2_web::handlers::get_metrics))
        .route("/api/v1/mining/stats", axum::routing::get(sv2_web::handlers::get_mining_stats))
        .route("/api/v1/templates", axum::routing::get(sv2_web::handlers::get_templates))
        .route("/api/v1/templates/:id", axum::routing::get(sv2_web::handlers::get_template))
        .route("/api/v1/templates/custom", axum::routing::post(sv2_web::handlers::submit_custom_template))
        .route("/api/v1/alerts", axum::routing::get(sv2_web::handlers::get_alerts))
        .route("/api/v1/config", axum::routing::get(sv2_web::handlers::get_config))
        .route("/api/v1/config", axum::routing::put(sv2_web::handlers::update_config))
        .with_state(app_state);

    (app, Arc::new(database) as Arc<dyn DatabaseOps>)
}

#[tokio::test]
async fn test_health_check_endpoint() {
    let (app, _) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let health: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(health["status"], "healthy");
    assert!(health["timestamp"].is_string());
    assert!(health["version"].is_string());
}

#[tokio::test]
async fn test_status_endpoint() {
    let (app, _) = setup_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/status")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let status: sv2_core::DaemonStatus = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(status.connections, 0); // No connections in fresh database
    assert_eq!(status.total_shares, 0);
}

#[tokio::test]
async fn test_connections_endpoint() {
    let (app, database) = setup_test_app().await;

    // Create a test connection
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
        total_shares: 100,
        valid_shares: 95,
        invalid_shares: 5,
        blocks_found: 1,
    };

    database.create_connection(&connection).await.unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/connections")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let connections: Vec<ConnectionInfo> = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(connections.len(), 1);
    assert_eq!(connections[0].id, connection.id);
    assert_eq!(connections[0].protocol, Protocol::Sv2);
}

#[tokio::test]
async fn test_connection_by_id_endpoint() {
    let (app, database) = setup_test_app().await;

    // Create a test connection
    let connection = ConnectionInfo {
        id: Uuid::new_v4(),
        address: "127.0.0.1:3333".parse().unwrap(),
        protocol: Protocol::Sv1,
        state: ConnectionState::Authenticated,
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
    };

    database.create_connection(&connection).await.unwrap();

    // Test existing connection
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&format!("/api/v1/connections/{}", connection.id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let retrieved_connection: ConnectionInfo = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(retrieved_connection.id, connection.id);
    assert_eq!(retrieved_connection.protocol, Protocol::Sv1);

    // Test non-existent connection
    let non_existent_id = Uuid::new_v4();
    let response = app
        .oneshot(
            Request::builder()
                .uri(&format!("/api/v1/connections/{}", non_existent_id))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_shares_endpoint() {
    let (app, database) = setup_test_app().await;

    // Create a test share
    let connection_id = Uuid::new_v4();
    let share = Share {
        connection_id,
        nonce: 12345,
        timestamp: chrono::Utc::now().timestamp() as u32,
        difficulty: 1.0,
        is_valid: true,
        block_hash: None,
        submitted_at: chrono::Utc::now(),
    };

    database.create_share(&share).await.unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/shares")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let shares: Vec<Share> = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(shares.len(), 1);
    assert_eq!(shares[0].connection_id, connection_id);
    assert_eq!(shares[0].nonce, 12345);
    assert!(shares[0].is_valid);
}

#[tokio::test]
async fn test_share_stats_endpoint() {
    let (app, database) = setup_test_app().await;

    // Create test shares
    let connection_id = Uuid::new_v4();
    for i in 0..10 {
        let share = Share {
            connection_id,
            nonce: i,
            timestamp: chrono::Utc::now().timestamp() as u32,
            difficulty: 1.0,
            is_valid: i % 10 != 0, // 90% valid shares
            block_hash: None,
            submitted_at: chrono::Utc::now(),
        };
        database.create_share(&share).await.unwrap();
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/shares/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let stats: sv2_core::database::ShareStats = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(stats.total_shares, 10);
    assert_eq!(stats.valid_shares, 9);
    assert_eq!(stats.invalid_shares, 1);
    assert!((stats.acceptance_rate - 90.0).abs() < 0.1);
}

#[tokio::test]
async fn test_config_endpoints() {
    let (app, _) = setup_test_app().await;

    // Test get config
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let config: DaemonConfig = serde_json::from_slice(&body).unwrap();
    
    // Should be default config
    assert_eq!(config.network.bind_address.port(), 3333);

    // Test update config
    let mut new_config = config.clone();
    new_config.network.max_connections = 2000;

    let update_request = json!({
        "config": new_config,
        "validate_only": false
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/config")
                .header("content-type", "application/json")
                .body(Body::from(update_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let update_response: sv2_web::handlers::ConfigUpdateResponse = serde_json::from_slice(&body).unwrap();
    
    assert!(update_response.success);
    assert_eq!(update_response.message, "Configuration updated successfully");
}

#[tokio::test]
async fn test_config_validation() {
    let (app, _) = setup_test_app().await;

    // Test invalid config
    let invalid_config = json!({
        "config": {
            "mode": {
                "type": "Solo",
                "config": {
                    "coinbase_address": "", // Invalid empty address
                    "block_template_refresh_interval": 30,
                    "enable_custom_templates": false,
                    "max_template_age": 300
                }
            },
            "network": {
                "bind_address": "127.0.0.1:3333",
                "max_connections": 0, // Invalid zero connections
                "connection_timeout": 30,
                "keepalive_interval": 60
            },
            "bitcoin": {
                "rpc_url": "http://127.0.0.1:8332",
                "rpc_user": "bitcoin",
                "rpc_password": "password",
                "network": "Regtest",
                "coinbase_address": null,
                "block_template_timeout": 30
            },
            "database": {
                "url": "sqlite://sv2d.db",
                "max_connections": 10,
                "connection_timeout": 30,
                "enable_migrations": true
            },
            "monitoring": {
                "enable_metrics": true,
                "metrics_bind_address": "127.0.0.1:9090",
                "enable_health_checks": true,
                "health_check_interval": 30,
                "metrics": {
                    "enabled": true,
                    "collection_interval": 10,
                    "prometheus_port": 9090,
                    "system_monitoring": true,
                    "labels": {}
                },
                "health": {
                    "enabled": true,
                    "check_interval": 30,
                    "check_timeout": 10,
                    "alert_thresholds": {
                        "cpu_usage": 80.0,
                        "memory_usage": 85.0,
                        "connection_count": 900,
                        "rejection_rate": 10.0,
                        "response_time": 5000,
                        "database_connections": 8
                    }
                }
            },
            "logging": {
                "level": "info",
                "format": "Pretty",
                "output": "Stdout",
                "enable_correlation_ids": true
            },
            "security": {
                "enable_authentication": false,
                "api_key": null,
                "rate_limit_per_minute": 60,
                "enable_tls": false,
                "tls_cert_path": null,
                "tls_key_path": null
            }
        },
        "validate_only": true
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("PUT")
                .uri("/api/v1/config")
                .header("content-type", "application/json")
                .body(Body::from(invalid_config.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let update_response: sv2_web::handlers::ConfigUpdateResponse = serde_json::from_slice(&body).unwrap();
    
    assert!(!update_response.success);
    assert!(update_response.validation_errors.is_some());
    assert!(!update_response.validation_errors.unwrap().is_empty());
}

#[tokio::test]
async fn test_custom_template_submission() {
    let (app, _) = setup_test_app().await;

    let template_request = json!({
        "transactions": [
            "0100000001000000000000000000000000000000000000000000000000000000000000000000000000ffffffff0100f2052a01000000434104678afdb0fe5548271967f1a67130b7105cd6a828e03909a67962e0ea1f61deb649f6bc3f4cef38c4f35504e51ec112de5c384df7ba0b8d578a4c702b6bf11d5fac00000000"
        ],
        "coinbase_data": "test coinbase",
        "difficulty": 1.0
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/templates/custom")
                .header("content-type", "application/json")
                .body(Body::from(template_request.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let template: WorkTemplate = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(template.difficulty, 1.0);
    assert_eq!(template.transactions.len(), 1);
}

#[tokio::test]
async fn test_alerts_endpoint() {
    let (app, database) = setup_test_app().await;

    // Create a test alert
    let alert = Alert::new(
        AlertLevel::Warning,
        "Test Alert".to_string(),
        "This is a test alert".to_string(),
        "test_component".to_string(),
    );

    database.create_alert(&alert).await.unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/alerts")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let alerts: Vec<Alert> = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(alerts.len(), 1);
    assert_eq!(alerts[0].title, "Test Alert");
    assert_eq!(alerts[0].level, AlertLevel::Warning);
}

#[tokio::test]
async fn test_mining_stats_endpoint() {
    let (app, database) = setup_test_app().await;

    // Create some test shares for statistics
    let connection_id = Uuid::new_v4();
    for i in 0..100 {
        let share = Share {
            connection_id,
            nonce: i,
            timestamp: chrono::Utc::now().timestamp() as u32,
            difficulty: 1.0,
            is_valid: i % 10 != 0, // 90% valid
            block_hash: None,
            submitted_at: chrono::Utc::now() - chrono::Duration::minutes(i as i64),
        };
        database.create_share(&share).await.unwrap();
    }

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/mining/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let stats: sv2_core::types::MiningStats = serde_json::from_slice(&body).unwrap();
    
    assert!(stats.hashrate > 0.0);
    assert!((stats.acceptance_rate - 90.0).abs() < 0.1);
    assert!(stats.shares_per_minute > 0.0);
}

#[tokio::test]
async fn test_error_handling() {
    let (app, _) = setup_test_app().await;

    // Test 404 for non-existent connection
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(&format!("/api/v1/connections/{}", Uuid::new_v4()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let error: sv2_web::handlers::ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(error.code, 404);
    assert_eq!(error.error, "Connection not found");

    // Test 400 for invalid custom template
    let invalid_template = json!({
        "transactions": ["invalid_hex"],
        "difficulty": 1.0
    });

    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/v1/templates/custom")
                .header("content-type", "application/json")
                .body(Body::from(invalid_template.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::BAD_REQUEST);

    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let error: sv2_web::handlers::ApiError = serde_json::from_slice(&body).unwrap();
    assert_eq!(error.code, 400);
    assert!(error.error.contains("Invalid hex encoding"));
}