// HTTP API server for sv2-cli communication
use crate::{
    error::{Error, Result},
    types::{DaemonStatus, ConnectionInfo, MiningStats, WorkTemplate},
    database::DatabaseOps,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::Json,
    routing::{get, post},
    Router,
};
use serde::{Deserialize, Serialize};
use std::{net::SocketAddr, sync::Arc};
use tokio::sync::RwLock;
use tower::ServiceBuilder;
use tower_http::{
    cors::CorsLayer,
    trace::TraceLayer,
};
use tracing::{info, error};
use uuid::Uuid;

/// API response wrapper
#[derive(Debug, Serialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: Option<T>,
    pub error: Option<String>,
}

impl<T> ApiResponse<T> {
    pub fn success(data: T) -> Self {
        Self {
            success: true,
            data: Some(data),
            error: None,
        }
    }

    pub fn error(message: String) -> Self {
        Self {
            success: false,
            data: None,
            error: Some(message),
        }
    }
}

/// API server state
#[derive(Clone)]
pub struct ApiState {
    pub database: Arc<dyn DatabaseOps>,
    pub daemon_status: Arc<RwLock<DaemonStatus>>,
    pub mining_stats: Arc<RwLock<MiningStats>>,
}

/// Query parameters for pagination
#[derive(Debug, Deserialize)]
pub struct PaginationQuery {
    pub limit: Option<u32>,
    pub offset: Option<u32>,
}

/// HTTP API server
pub struct ApiServer {
    bind_address: SocketAddr,
    state: ApiState,
}

impl ApiServer {
    pub fn new(
        bind_address: SocketAddr,
        database: Arc<dyn DatabaseOps>,
        daemon_status: Arc<RwLock<DaemonStatus>>,
        mining_stats: Arc<RwLock<MiningStats>>,
    ) -> Self {
        let state = ApiState {
            database,
            daemon_status,
            mining_stats,
        };

        Self {
            bind_address,
            state,
        }
    }

    /// Start the API server
    pub async fn start(self) -> Result<()> {
        let app = self.create_router();
        
        info!("Starting API server on {}", self.bind_address);
        
        let listener = tokio::net::TcpListener::bind(self.bind_address).await
            .map_err(|e| Error::Network(format!("Failed to bind API server: {}", e)))?;

        axum::serve(listener, app).await
            .map_err(|e| Error::Network(format!("API server error: {}", e)))?;

        Ok(())
    }

    /// Create the router with all endpoints
    fn create_router(&self) -> Router {
        Router::new()
            // Status endpoints
            .route("/api/v1/status", get(get_status))
            .route("/api/v1/health", get(get_health))
            // Connection endpoints
            .route("/api/v1/connections", get(get_connections))
            .route("/api/v1/connections/:id", get(get_connection))
            // Mining endpoints
            .route("/api/v1/mining/stats", get(get_mining_stats))
            .route("/api/v1/mining/templates", get(get_templates))
            // Control endpoints
            .route("/api/v1/control/shutdown", post(shutdown_daemon))
            .layer(
                ServiceBuilder::new()
                    .layer(TraceLayer::new_for_http())
                    .layer(CorsLayer::permissive())
            )
            .with_state(self.state.clone())
    }
}

/// Get daemon status
async fn get_status(State(state): State<ApiState>) -> Json<ApiResponse<DaemonStatus>> {
    let status = state.daemon_status.read().await.clone();
    Json(ApiResponse::success(status))
}

/// Health check endpoint
async fn get_health() -> Json<ApiResponse<&'static str>> {
    Json(ApiResponse::success("OK"))
}

/// Get all connections
async fn get_connections(
    State(state): State<ApiState>,
    Query(params): Query<PaginationQuery>,
) -> std::result::Result<Json<ApiResponse<Vec<ConnectionInfo>>>, StatusCode> {
    let limit = params.limit.unwrap_or(100);
    let offset = params.offset.unwrap_or(0);

    match state.database.get_connections(Some(limit), Some(offset)).await {
        Ok(connections) => Ok(Json(ApiResponse::success(connections))),
        Err(e) => {
            error!("Failed to get connections: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get specific connection
async fn get_connection(
    State(state): State<ApiState>,
    Path(id): Path<String>,
) -> std::result::Result<Json<ApiResponse<ConnectionInfo>>, StatusCode> {
    let connection_id = Uuid::parse_str(&id)
        .map_err(|_| StatusCode::BAD_REQUEST)?;

    match state.database.get_connection_info(connection_id).await {
        Ok(Some(connection)) => Ok(Json(ApiResponse::success(connection))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            error!("Failed to get connection {}: {}", id, e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Get mining statistics
async fn get_mining_stats(State(state): State<ApiState>) -> Json<ApiResponse<MiningStats>> {
    let stats = state.mining_stats.read().await.clone();
    Json(ApiResponse::success(stats))
}

/// Get work templates
async fn get_templates(
    State(state): State<ApiState>,
    Query(params): Query<PaginationQuery>,
) -> std::result::Result<Json<ApiResponse<Vec<WorkTemplate>>>, StatusCode> {
    let limit = params.limit.unwrap_or(10);

    match state.database.get_work_templates(Some(limit)).await {
        Ok(templates) => Ok(Json(ApiResponse::success(templates))),
        Err(e) => {
            error!("Failed to get templates: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// Shutdown daemon
async fn shutdown_daemon(State(_state): State<ApiState>) -> Json<ApiResponse<&'static str>> {
    // In a real implementation, this would trigger a graceful shutdown
    info!("Shutdown requested via API");
    Json(ApiResponse::success("Shutdown initiated"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::MockDatabaseOps;
    use std::sync::Arc;
    use tokio::sync::RwLock;

    fn create_test_state() -> ApiState {
        let database = Arc::new(MockDatabaseOps::new());
        let daemon_status = Arc::new(RwLock::new(DaemonStatus::default()));
        let mining_stats = Arc::new(RwLock::new(MiningStats::default()));

        ApiState {
            database,
            daemon_status,
            mining_stats,
        }
    }

    #[tokio::test]
    async fn test_api_server_creation() {
        let state = create_test_state();
        let server = ApiServer::new(
            "127.0.0.1:0".parse().unwrap(),
            state.database,
            state.daemon_status,
            state.mining_stats,
        );

        // Just test that we can create the router without panicking
        let _router = server.create_router();
    }

    #[tokio::test]
    async fn test_health_endpoint() {
        let response = get_health().await;
        assert!(response.0.success);
        assert_eq!(response.0.data, Some("OK"));
    }
}