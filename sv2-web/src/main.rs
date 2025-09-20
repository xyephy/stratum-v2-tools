use axum::{
    routing::{get, post, put, delete},
    Router,
    http::StatusCode,
    response::Redirect,
    middleware,
};
use tower_http::{
    services::ServeDir,
    cors::{CorsLayer, Any},
    trace::TraceLayer,
};
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::info;
use sv2_core::{
    database::{DatabasePool, DatabaseOps},
    config::DaemonConfig,
    auth::{AuthSystem, AuthConfig},
    connection_auth::ConnectionAuthManager,
};

pub mod auth_middleware;
pub mod validation_middleware;
pub mod handlers;
pub mod websocket;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging with structured output
    tracing_subscriber::fmt::init();

    // Initialize database connection
    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://sv2d.db".to_string());
    
    info!("Connecting to database: {}", database_url);
    let database = DatabasePool::new(&database_url, 10).await?;
    database.migrate().await?;
    info!("Database initialized successfully");
    
    // Initialize configuration
    let config = Arc::new(tokio::sync::RwLock::new(DaemonConfig::default()));
    
    // Initialize authentication system
    let auth_config = {
        let config_guard = config.read().await;
        config_guard.security.auth.clone()
    };
    let auth_system = Arc::new(tokio::sync::RwLock::new(AuthSystem::new(auth_config)));
    let connection_auth = Arc::new(ConnectionAuthManager::new(auth_system.clone()));
    
    // Create application state
    let app_state = handlers::AppState {
        database: Arc::new(database) as Arc<dyn DatabaseOps>,
        config,
    };
    
    // Create authentication middleware state
    let auth_middleware_state = auth_middleware::AuthMiddlewareState {
        auth_system,
        connection_auth,
    };
    
    // Create validation middleware state
    let validation_middleware_state = validation_middleware::ValidationMiddlewareState::new()
        .map_err(|e| anyhow::anyhow!("Failed to create validation middleware: {}", e))?;

    // Determine static files directory
    let static_dir = determine_static_dir();
    info!("Serving static files from: {}", static_dir);

    // Build the router with all API endpoints
    let app = Router::new()
        // Root redirect to static index.html
        .route("/", get(|| async { Redirect::permanent("/static/index.html") }))
        
        // Dashboard route (alternative access)
        .route("/dashboard", get(handlers::index))
        
        // API v1 routes
        .route("/api/v1/status", get(handlers::get_status))
        .route("/api/v1/health", get(handlers::health_check))
        
        // Connection management
        .route("/api/v1/connections", get(handlers::get_connections))
        .route("/api/v1/connections/:id", get(handlers::get_connection))
        .route("/api/v1/connections/:id", delete(handlers::disconnect_connection))
        
        // Share management
        .route("/api/v1/shares", get(handlers::get_shares))
        .route("/api/v1/shares/stats", get(handlers::get_share_stats))
        
        // Metrics and monitoring
        .route("/api/v1/metrics", get(handlers::get_metrics))
        .route("/api/v1/mining/stats", get(handlers::get_mining_stats))
        
        // Work template management
        .route("/api/v1/templates", get(handlers::get_templates))
        .route("/api/v1/templates/:id", get(handlers::get_template))
        .route("/api/v1/templates/custom", post(handlers::submit_custom_template))
        
        // Alert management
        .route("/api/v1/alerts", get(handlers::get_alerts))
        
        // Configuration management
        .route("/api/v1/config", get(handlers::get_config))
        .route("/api/v1/config", put(handlers::update_config))
        
        // WebSocket for real-time updates
        .route("/ws", get(websocket::websocket_handler))
        
        // Static file serving with proper fallback
        .nest_service("/static", ServeDir::new(&static_dir))
        
        // Fallback handler for SPA routing
        .fallback(static_file_fallback)
        
        // Add application state
        .with_state(app_state)
        
        // Add validation middleware (first)
        .layer(middleware::from_fn_with_state(
            validation_middleware_state.clone(),
            validation_middleware::rate_limit_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            validation_middleware_state,
            validation_middleware::input_validation_middleware,
        ))
        .layer(middleware::from_fn(validation_middleware::csp_middleware))
        
        // Add authentication middleware
        .layer(middleware::from_fn_with_state(
            auth_middleware_state.clone(),
            auth_middleware::auth_middleware,
        ))
        .layer(middleware::from_fn_with_state(
            auth_middleware_state,
            auth_middleware::rate_limit_middleware,
        ))
        .layer(middleware::from_fn(auth_middleware::security_headers_middleware))
        .layer(middleware::from_fn(auth_middleware::cors_middleware))
        
        // Add other middleware
        .layer(TraceLayer::new_for_http());

    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));
    info!("🚀 sv2-web dashboard starting on http://{}", addr);
    info!("📊 Dashboard available at http://{}/", addr);
    info!("🔌 WebSocket endpoint at ws://{}/ws", addr);
    info!("📡 API documentation at http://{}/api/v1/", addr);

    // Start the server
    info!("Server listening on {}", addr);
    
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}

/// Determine the static files directory based on the current working directory
fn determine_static_dir() -> String {
    let current_dir = std::env::current_dir().unwrap_or_default();
    
    // Check if we're in the sv2-web directory
    let sv2_web_static = current_dir.join("sv2-web").join("static");
    if sv2_web_static.exists() {
        return sv2_web_static.to_string_lossy().to_string();
    }
    
    // Check if we're already in sv2-web and static exists
    let local_static = current_dir.join("static");
    if local_static.exists() {
        return local_static.to_string_lossy().to_string();
    }
    
    // Default to relative static directory
    "static".to_string()
}

/// Fallback handler for serving static files (SPA support)
async fn static_file_fallback() -> Result<Redirect, StatusCode> {
    // For any unmatched routes, redirect to the main dashboard
    Ok(Redirect::permanent("/static/index.html"))
}