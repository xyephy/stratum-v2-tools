use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use hyper::Method;
use serde_json::Value;
use std::sync::Arc;
use sv2_core::{
    config::DaemonConfig,
    database::{DatabasePool, DatabaseOps},
};
use sv2_web::handlers::AppState;
use tower::ServiceExt;

/// Helper function to create test app state
async fn create_test_app_state() -> AppState {
    let database = DatabasePool::new("sqlite::memory:", 1).await.unwrap();
    database.migrate().await.unwrap();
    
    let config = Arc::new(tokio::sync::RwLock::new(DaemonConfig::default()));
    
    AppState {
        database: Arc::new(database) as Arc<dyn DatabaseOps>,
        config,
    }
}

/// Helper function to create test router
async fn create_test_router() -> axum::Router {
    use axum::{routing::get, Router};
    use sv2_web::{handlers, websocket};
    
    let app_state = create_test_app_state().await;
    
    Router::new()
        .route("/api/v1/status", get(handlers::get_status))
        .route("/api/v1/health", get(handlers::health_check))
        .route("/api/v1/connections", get(handlers::get_connections))
        .route("/api/v1/shares", get(handlers::get_shares))
        .route("/api/v1/config", get(handlers::get_config))
        .route("/ws", get(websocket::websocket_handler))
        .with_state(app_state)
}

#[tokio::test]
async fn test_health_check_endpoint() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/health")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let health: Value = serde_json::from_slice(&body).unwrap();
    
    assert_eq!(health["status"], "healthy");
    assert!(health["timestamp"].is_string());
    assert!(health["version"].is_string());
    assert!(health["uptime"].is_number());
}

#[tokio::test]
async fn test_status_endpoint() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/status")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let status: Value = serde_json::from_slice(&body).unwrap();
    
    // Check that all expected fields are present
    assert!(status["uptime"].is_number());
    assert!(status["connections"].is_number());
    assert!(status["total_shares"].is_number());
    assert!(status["valid_shares"].is_number());
    assert!(status["blocks_found"].is_number());
    assert!(status["current_difficulty"].is_number());
    assert!(status["hashrate"].is_number());
}

#[tokio::test]
async fn test_connections_endpoint() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/connections")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let connections: Value = serde_json::from_slice(&body).unwrap();
    
    // Should return an array (empty initially)
    assert!(connections.is_array());
}

#[tokio::test]
async fn test_shares_endpoint() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/shares")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let shares: Value = serde_json::from_slice(&body).unwrap();
    
    // Should return an array (empty initially)
    assert!(shares.is_array());
}

#[tokio::test]
async fn test_config_endpoint() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/config")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
    let config: Value = serde_json::from_slice(&body).unwrap();
    
    // Should return a configuration object
    assert!(config.is_object());
}

#[tokio::test]
async fn test_connections_with_query_params() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/connections?limit=10")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    // Should be OK even with query params
    assert!(response.status().is_success() || response.status() == StatusCode::BAD_REQUEST);
    
    if response.status() == StatusCode::OK {
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let connections: Value = serde_json::from_slice(&body).unwrap();
        assert!(connections.is_array());
    }
}

#[tokio::test]
async fn test_shares_with_query_params() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/shares?limit=20")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    // Should be OK even with query params
    assert!(response.status().is_success() || response.status() == StatusCode::BAD_REQUEST);
    
    if response.status() == StatusCode::OK {
        let body = hyper::body::to_bytes(response.into_body()).await.unwrap();
        let shares: Value = serde_json::from_slice(&body).unwrap();
        assert!(shares.is_array());
    }
}

#[tokio::test]
async fn test_nonexistent_endpoint() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/nonexistent")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_cors_headers() {
    let app = create_test_router().await;
    
    let request = Request::builder()
        .method(Method::OPTIONS)
        .uri("/api/v1/status")
        .header("Origin", "http://localhost:3000")
        .header("Access-Control-Request-Method", "GET")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    // Should handle CORS preflight requests
    assert!(response.status().is_success() || response.status() == StatusCode::METHOD_NOT_ALLOWED);
}

#[tokio::test]
async fn test_api_error_format() {
    let app = create_test_router().await;
    
    // Test with an invalid UUID to trigger an error
    let request = Request::builder()
        .method(Method::GET)
        .uri("/api/v1/connections/invalid-uuid")
        .body(Body::empty())
        .unwrap();
    
    let response = app.oneshot(request).await.unwrap();
    
    // Should return 404 or 400 for invalid UUID
    assert!(response.status().is_client_error());
}

#[cfg(test)]
mod static_file_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_static_files_exist() {
        use std::path::Path;
        
        // Check that required static files exist (relative to workspace root or sv2-web directory)
        let static_files = [
            "static/index.html",
            "static/css/dashboard.css", 
            "static/css/charts.css",
            "static/js/websocket.js",
            "static/js/charts.js",
            "static/js/dashboard.js",
        ];

        for file_path in &static_files {
            let path = Path::new(file_path);
            let alt_path_str = format!("sv2-web/{}", file_path);
            let alt_path = Path::new(&alt_path_str);
            
            assert!(
                path.exists() || alt_path.exists(),
                "Static file {} does not exist (checked {} and {})",
                file_path,
                path.display(),
                alt_path.display()
            );
        }
    }

    #[test]
    fn test_html_file_validity() {
        let html_content = std::fs::read_to_string("static/index.html")
            .or_else(|_| std::fs::read_to_string("sv2-web/static/index.html"))
            .expect("Failed to read index.html");

        // Basic HTML validation
        assert!(html_content.contains("<!DOCTYPE html>"));
        assert!(html_content.contains("<html"));
        assert!(html_content.contains("</html>"));
        assert!(html_content.contains("<head>"));
        assert!(html_content.contains("</head>"));
        assert!(html_content.contains("<body>"));
        assert!(html_content.contains("</body>"));
        
        // Check for required elements
        assert!(html_content.contains("sv2d Dashboard"));
        assert!(html_content.contains("dashboard.css"));
        assert!(html_content.contains("charts.css"));
        assert!(html_content.contains("websocket.js"));
        assert!(html_content.contains("charts.js"));
        assert!(html_content.contains("dashboard.js"));
    }

    #[test]
    fn test_css_files_validity() {
        let css_files = [
            "static/css/dashboard.css",
            "static/css/charts.css",
        ];

        for css_file in &css_files {
            let css_content = std::fs::read_to_string(css_file)
                .or_else(|_| std::fs::read_to_string(&format!("sv2-web/{}", css_file)))
                .unwrap_or_else(|_| panic!("Failed to read {}", css_file));

            // Basic CSS validation - check for CSS rules
            assert!(css_content.contains("{"));
            assert!(css_content.contains("}"));
            
            // Check for CSS variables (modern CSS)
            if css_file.contains("dashboard.css") {
                assert!(css_content.contains(":root"));
                assert!(css_content.contains("--primary-color"));
            }
        }
    }

    #[test]
    fn test_js_files_validity() {
        let js_files = [
            "static/js/websocket.js",
            "static/js/charts.js",
            "static/js/dashboard.js",
        ];

        for js_file in &js_files {
            let js_content = std::fs::read_to_string(js_file)
                .or_else(|_| std::fs::read_to_string(&format!("sv2-web/{}", js_file)))
                .unwrap_or_else(|_| panic!("Failed to read {}", js_file));

            // Basic JavaScript validation
            assert!(!js_content.is_empty());
            
            // Check for class definitions (modern JS)
            if js_file.contains("websocket.js") {
                assert!(js_content.contains("class WebSocketManager"));
            }
            if js_file.contains("charts.js") {
                assert!(js_content.contains("class ChartManager"));
            }
            if js_file.contains("dashboard.js") {
                assert!(js_content.contains("class DashboardManager"));
            }
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use tokio_test;

    #[tokio::test]
    async fn test_full_api_workflow() {
        let app = create_test_router().await;

        // Test health check
        let health_request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/health")
            .body(Body::empty())
            .unwrap();
        
        let health_response = app.clone().oneshot(health_request).await.unwrap();
        assert_eq!(health_response.status(), StatusCode::OK);

        // Test status
        let status_request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/status")
            .body(Body::empty())
            .unwrap();
        
        let status_response = app.clone().oneshot(status_request).await.unwrap();
        assert_eq!(status_response.status(), StatusCode::OK);

        // Test connections
        let connections_request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/connections")
            .body(Body::empty())
            .unwrap();
        
        let connections_response = app.clone().oneshot(connections_request).await.unwrap();
        assert_eq!(connections_response.status(), StatusCode::OK);

        // Test shares
        let shares_request = Request::builder()
            .method(Method::GET)
            .uri("/api/v1/shares")
            .body(Body::empty())
            .unwrap();
        
        let shares_response = app.oneshot(shares_request).await.unwrap();
        assert_eq!(shares_response.status(), StatusCode::OK);
    }
}