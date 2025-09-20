use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, Json},

};
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use std::sync::Arc;
use sv2_core::{
    DaemonStatus, ConnectionInfo, Share, WorkTemplate, PerformanceMetrics, Alert,
    database::{DatabaseOps, ShareStats},
    config::DaemonConfig,
    types::MiningStats,
};
use uuid::Uuid;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub database: Arc<dyn DatabaseOps>,
    pub config: Arc<tokio::sync::RwLock<DaemonConfig>>,
}

/// Query parameters for pagination
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// Query parameters for connection filtering
#[derive(Debug, Deserialize)]
pub struct ConnectionQuery {
    pub protocol: Option<String>,
    pub state: Option<String>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

/// Query parameters for share filtering
#[derive(Debug, Deserialize)]
pub struct ShareQuery {
    pub connection_id: Option<Uuid>,
    pub valid_only: Option<bool>,
    #[serde(flatten)]
    pub pagination: PaginationQuery,
}

/// Configuration update request
#[derive(Debug, Deserialize)]
pub struct ConfigUpdateRequest {
    pub config: serde_json::Value,
    pub validate_only: Option<bool>,
}

/// Configuration update response
#[derive(Debug, Serialize, Deserialize)]
pub struct ConfigUpdateResponse {
    pub success: bool,
    pub message: String,
    pub validation_errors: Option<Vec<String>>,
}

/// Custom work template request
#[derive(Debug, Deserialize)]
pub struct CustomTemplateRequest {
    pub transactions: Vec<String>, // Hex-encoded transactions
    pub coinbase_data: Option<String>,
    pub difficulty: Option<f64>,
}

/// API error response
#[derive(Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
    pub code: u16,
    pub details: Option<serde_json::Value>,
}

impl ApiError {
    pub fn new(code: u16, message: &str) -> Self {
        Self {
            error: message.to_string(),
            code,
            details: None,
        }
    }

    pub fn with_details(code: u16, message: &str, details: serde_json::Value) -> Self {
        Self {
            error: message.to_string(),
            code,
            details: Some(details),
        }
    }
}

/// Serve the main dashboard page
pub async fn index() -> Html<&'static str> {
    Html(r#"
<!DOCTYPE html>
<html>
<head>
    <title>sv2d Dashboard</title>
    <style>
        body { font-family: Arial, sans-serif; margin: 40px; }
        .status { background: #f0f0f0; padding: 20px; border-radius: 5px; margin: 20px 0; }
        .metrics { display: grid; grid-template-columns: repeat(auto-fit, minmax(200px, 1fr)); gap: 20px; }
        .metric { background: #fff; padding: 15px; border: 1px solid #ddd; border-radius: 5px; }
        .metric h3 { margin: 0 0 10px 0; color: #333; }
        .metric .value { font-size: 24px; font-weight: bold; color: #007acc; }
        .api-links { background: #f9f9f9; padding: 20px; border-radius: 5px; margin: 20px 0; }
        .api-links h3 { margin-top: 0; }
        .api-links a { display: block; margin: 5px 0; color: #007acc; text-decoration: none; }
        .api-links a:hover { text-decoration: underline; }
    </style>
</head>
<body>
    <h1>sv2d Dashboard</h1>
    
    <div class="status">
        <h2>System Status</h2>
        <p>Dashboard is running. Real-time data will be available via WebSocket connection.</p>
    </div>

    <div class="metrics">
        <div class="metric">
            <h3>Connections</h3>
            <div class="value" id="connections">-</div>
        </div>
        <div class="metric">
            <h3>Hashrate</h3>
            <div class="value" id="hashrate">- TH/s</div>
        </div>
        <div class="metric">
            <h3>Shares</h3>
            <div class="value" id="shares">-</div>
        </div>
        <div class="metric">
            <h3>Blocks Found</h3>
            <div class="value" id="blocks">-</div>
        </div>
    </div>

    <div class="api-links">
        <h3>API Endpoints</h3>
        <a href="/api/v1/status">System Status</a>
        <a href="/api/v1/connections">Active Connections</a>
        <a href="/api/v1/shares">Recent Shares</a>
        <a href="/api/v1/metrics">Performance Metrics</a>
        <a href="/api/v1/templates">Work Templates</a>
        <a href="/api/v1/alerts">System Alerts</a>
        <a href="/api/v1/config">Configuration</a>
    </div>

    <script>
        // Connect to WebSocket for real-time updates
        const ws = new WebSocket('ws://localhost:8080/ws');
        
        ws.onmessage = function(event) {
            const data = JSON.parse(event.data);
            if (data.type === 'status') {
                document.getElementById('connections').textContent = data.connections;
                document.getElementById('hashrate').textContent = (data.hashrate / 1e12).toFixed(2) + ' TH/s';
                document.getElementById('shares').textContent = data.total_shares;
                document.getElementById('blocks').textContent = data.blocks_found;
            }
        };
        
        ws.onopen = function() {
            console.log('WebSocket connected');
        };
        
        ws.onerror = function(error) {
            console.error('WebSocket error:', error);
        };
    </script>
</body>
</html>
    "#)
}

/// Get daemon status via API
pub async fn get_status(State(state): State<AppState>) -> Result<Json<DaemonStatus>, (StatusCode, Json<ApiError>)> {
    // In a real implementation, this would query the actual daemon
    // For now, we'll return mock data with some database stats
    match state.database.get_share_stats(None).await {
        Ok(share_stats) => {
            let connections = state.database.list_connections(None).await.unwrap_or_default();
            let status = DaemonStatus {
                running: true,
                uptime: std::time::Duration::from_secs(3600), // Mock 1 hour uptime
                active_connections: connections.len() as u64,
                total_connections: connections.len() as u64,
                mode: "Solo".to_string(), // TODO: Get from config
                version: env!("CARGO_PKG_VERSION").to_string(),
                total_shares: share_stats.total_shares,
                valid_shares: share_stats.valid_shares,
                blocks_found: share_stats.blocks_found,
                current_difficulty: 1.0, // TODO: Get from config
                hashrate: share_stats.total_shares as f64 * 1e9, // Mock calculation
            };
            Ok(Json(status))
        }
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get status: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get active connections
pub async fn get_connections(
    State(state): State<AppState>,
    Query(query): Query<ConnectionQuery>,
) -> Result<Json<Vec<ConnectionInfo>>, (StatusCode, Json<ApiError>)> {
    match state.database.list_connections(query.pagination.limit).await {
        Ok(mut connections) => {
            // Apply filters
            if let Some(protocol) = &query.protocol {
                connections.retain(|conn| format!("{:?}", conn.protocol).to_lowercase() == protocol.to_lowercase());
            }
            
            if let Some(state_filter) = &query.state {
                connections.retain(|conn| format!("{:?}", conn.state).to_lowercase().contains(&state_filter.to_lowercase()));
            }
            
            Ok(Json(connections))
        }
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get connections: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get connection by ID
pub async fn get_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<ConnectionInfo>, (StatusCode, Json<ApiError>)> {
    match state.database.get_connection(id).await {
        Ok(Some(connection)) => Ok(Json(connection)),
        Ok(None) => {
            let error = ApiError::new(404, "Connection not found");
            Err((StatusCode::NOT_FOUND, Json(error)))
        }
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get connection: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get shares with optional filtering
pub async fn get_shares(
    State(state): State<AppState>,
    Query(query): Query<ShareQuery>,
) -> Result<Json<Vec<Share>>, (StatusCode, Json<ApiError>)> {
    match state.database.get_shares(query.connection_id, query.pagination.limit).await {
        Ok(mut shares) => {
            // Apply valid_only filter
            if let Some(true) = query.valid_only {
                shares.retain(|share| share.is_valid);
            }
            
            Ok(Json(shares))
        }
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get shares: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get share statistics
pub async fn get_share_stats(
    State(state): State<AppState>,
    Query(query): Query<ShareQuery>,
) -> Result<Json<ShareStats>, (StatusCode, Json<ApiError>)> {
    match state.database.get_share_stats(query.connection_id).await {
        Ok(stats) => Ok(Json(stats)),
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get share stats: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get performance metrics
pub async fn get_metrics(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<Vec<PerformanceMetrics>>, (StatusCode, Json<ApiError>)> {
    match state.database.get_performance_metrics(query.limit).await {
        Ok(metrics) => Ok(Json(metrics)),
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get metrics: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get work templates
pub async fn get_templates(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<Vec<WorkTemplate>>, (StatusCode, Json<ApiError>)> {
    match state.database.list_work_templates(query.limit).await {
        Ok(templates) => Ok(Json(templates)),
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get templates: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get work template by ID
pub async fn get_template(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<WorkTemplate>, (StatusCode, Json<ApiError>)> {
    match state.database.get_work_template(id).await {
        Ok(Some(template)) => Ok(Json(template)),
        Ok(None) => {
            let error = ApiError::new(404, "Template not found");
            Err((StatusCode::NOT_FOUND, Json(error)))
        }
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get template: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Submit custom work template
pub async fn submit_custom_template(
    State(state): State<AppState>,
    Json(request): Json<CustomTemplateRequest>,
) -> Result<Json<WorkTemplate>, (StatusCode, Json<ApiError>)> {
    // This is a simplified implementation
    // In a real system, this would validate the template and integrate with the mining system
    
    // Parse transactions from hex
    let mut transactions = Vec::new();
    for tx_hex in &request.transactions {
        match hex::decode(tx_hex) {
            Ok(tx_bytes) => {
                match bitcoin::consensus::encode::deserialize::<bitcoin::Transaction>(&tx_bytes) {
                    Ok(tx) => transactions.push(tx),
                    Err(e) => {
                        let error = ApiError::new(400, &format!("Invalid transaction: {}", e));
                        return Err((StatusCode::BAD_REQUEST, Json(error)));
                    }
                }
            }
            Err(e) => {
                let error = ApiError::new(400, &format!("Invalid hex encoding: {}", e));
                return Err((StatusCode::BAD_REQUEST, Json(error)));
            }
        }
    }
    
    // Create a mock template (in real implementation, this would be more sophisticated)
    let prev_hash = bitcoin::BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000")
        .unwrap();
    let coinbase_tx = bitcoin::Transaction {
        version: 1,
        lock_time: bitcoin::absolute::LockTime::ZERO,
        input: vec![bitcoin::TxIn::default()],
        output: vec![bitcoin::TxOut::default()],
    };
    
    let template = WorkTemplate::new(
        prev_hash,
        coinbase_tx,
        transactions,
        request.difficulty.unwrap_or(1.0),
    );
    
    // Store the template
    match state.database.create_work_template(&template).await {
        Ok(_) => Ok(Json(template)),
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to store template: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get system alerts
pub async fn get_alerts(
    State(state): State<AppState>,
    Query(query): Query<PaginationQuery>,
) -> Result<Json<Vec<Alert>>, (StatusCode, Json<ApiError>)> {
    match state.database.get_alerts(None, query.limit).await {
        Ok(alerts) => Ok(Json(alerts)),
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get alerts: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Get current configuration
pub async fn get_config(
    State(state): State<AppState>,
) -> Result<Json<DaemonConfig>, (StatusCode, Json<ApiError>)> {
    let config = state.config.read().await;
    Ok(Json(config.clone()))
}

/// Update configuration
pub async fn update_config(
    State(state): State<AppState>,
    Json(request): Json<ConfigUpdateRequest>,
) -> Result<Json<ConfigUpdateResponse>, (StatusCode, Json<ApiError>)> {
    // Parse the configuration
    let new_config: DaemonConfig = match serde_json::from_value(request.config) {
        Ok(config) => config,
        Err(e) => {
            let error = ApiError::new(400, &format!("Invalid configuration format: {}", e));
            return Err((StatusCode::BAD_REQUEST, Json(error)));
        }
    };
    
    // Validate the configuration
    if let Err(e) = new_config.validate() {
        let response = ConfigUpdateResponse {
            success: false,
            message: "Configuration validation failed".to_string(),
            validation_errors: Some(vec![e.to_string()]),
        };
        return Ok(Json(response));
    }
    
    // If validate_only is true, just return validation result
    if request.validate_only.unwrap_or(false) {
        let response = ConfigUpdateResponse {
            success: true,
            message: "Configuration is valid".to_string(),
            validation_errors: None,
        };
        return Ok(Json(response));
    }
    
    // Store configuration history
    let config_json = serde_json::to_string_pretty(&new_config).unwrap_or_default();
    
    // Update the configuration
    {
        let mut config = state.config.write().await;
        *config = new_config;
    }
    if let Err(e) = state.database.store_config_history(&config_json, "api").await {
        tracing::warn!("Failed to store config history: {}", e);
    }
    
    let response = ConfigUpdateResponse {
        success: true,
        message: "Configuration updated successfully".to_string(),
        validation_errors: None,
    };
    
    Ok(Json(response))
}

/// Get mining statistics (aggregated data)
pub async fn get_mining_stats(
    State(state): State<AppState>,
) -> Result<Json<MiningStats>, (StatusCode, Json<ApiError>)> {
    match state.database.get_share_stats(None).await {
        Ok(share_stats) => {
            let stats = MiningStats {
                hashrate: share_stats.total_shares as f64 * 1e12, // Mock calculation
                shares_per_minute: if let Some(first) = share_stats.first_share {
                    let duration = chrono::Utc::now() - first;
                    if duration.num_minutes() > 0 {
                        share_stats.total_shares as f64 / duration.num_minutes() as f64
                    } else {
                        0.0
                    }
                } else {
                    0.0
                },
                acceptance_rate: share_stats.acceptance_rate,
                efficiency: share_stats.acceptance_rate, // Simplified
                uptime: std::time::Duration::from_secs(3600), // Mock uptime
                shares_accepted: share_stats.valid_shares,
                shares_rejected: share_stats.invalid_shares,
                blocks_found: 0, // TODO: Get from database
            };
            Ok(Json(stats))
        }
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to get mining stats: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Disconnect a connection by ID
pub async fn disconnect_connection(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    // In a real implementation, this would signal the daemon to disconnect the connection
    // For now, we'll just return a success response
    match state.database.get_connection(id).await {
        Ok(Some(_connection)) => {
            // TODO: Implement actual connection disconnection logic
            // This would typically send a signal to the daemon to close the connection
            
            let response = serde_json::json!({
                "success": true,
                "message": "Connection disconnect requested",
                "connection_id": id
            });
            Ok(Json(response))
        }
        Ok(None) => {
            let error = ApiError::new(404, "Connection not found");
            Err((StatusCode::NOT_FOUND, Json(error)))
        }
        Err(e) => {
            let error = ApiError::new(500, &format!("Failed to disconnect connection: {}", e));
            Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)))
        }
    }
}

/// Health check endpoint
pub async fn health_check() -> Result<Json<serde_json::Value>, (StatusCode, Json<ApiError>)> {
    let health = serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now(),
        "version": env!("CARGO_PKG_VERSION"),
        "uptime": 3600 // Mock uptime in seconds
    });
    Ok(Json(health))
}