use axum::{
    extract::State,
    http::{HeaderMap, StatusCode, Request},
    middleware::Next,
    response::{Json, Response},
    body::Body,
};
use serde_json::json;
use std::sync::Arc;
use sv2_core::{
    auth::{AuthSystem, Permission, AuthResult, AuthzResult},
    connection_auth::{ConnectionAuthManager, ConnectionAuthResult},
};
use tokio::sync::RwLock;

/// Authentication middleware state
#[derive(Clone)]
pub struct AuthMiddlewareState {
    pub auth_system: Arc<RwLock<AuthSystem>>,
    pub connection_auth: Arc<ConnectionAuthManager>,
}

/// Extract API key from request headers
fn extract_api_key(headers: &HeaderMap) -> Option<String> {
    // Try Authorization header first (Bearer token)
    if let Some(auth_header) = headers.get("authorization") {
        if let Ok(auth_str) = auth_header.to_str() {
            if auth_str.starts_with("Bearer ") {
                return Some(auth_str[7..].to_string());
            }
        }
    }
    
    // Try X-API-Key header
    if let Some(api_key_header) = headers.get("x-api-key") {
        if let Ok(api_key) = api_key_header.to_str() {
            return Some(api_key.to_string());
        }
    }
    
    None
}

/// Extract client identifier from request
fn extract_client_id(headers: &HeaderMap) -> String {
    // Try to get real IP from X-Forwarded-For or X-Real-IP
    if let Some(forwarded) = headers.get("x-forwarded-for") {
        if let Ok(forwarded_str) = forwarded.to_str() {
            if let Some(first_ip) = forwarded_str.split(',').next() {
                return first_ip.trim().to_string();
            }
        }
    }
    
    if let Some(real_ip) = headers.get("x-real-ip") {
        if let Ok(ip_str) = real_ip.to_str() {
            return ip_str.to_string();
        }
    }
    
    // Fallback to a default identifier
    "unknown".to_string()
}

/// Get required permission for a request path and method
fn get_required_permission(path: &str, method: &str) -> Option<Permission> {
    match (method, path) {
        // Health check and status - no auth required
        ("GET", "/health") => None,
        ("GET", "/") => None,
        
        // Read-only operations
        ("GET", path) if path.starts_with("/api/v1/status") => Some(Permission::ViewMetrics),
        ("GET", path) if path.starts_with("/api/v1/connections") => Some(Permission::ViewConnections),
        ("GET", path) if path.starts_with("/api/v1/shares") => Some(Permission::ViewShares),
        ("GET", path) if path.starts_with("/api/v1/templates") => Some(Permission::ViewTemplates),
        ("GET", path) if path.starts_with("/api/v1/metrics") => Some(Permission::ViewMetrics),
        ("GET", path) if path.starts_with("/api/v1/alerts") => Some(Permission::ViewHealth),
        ("GET", path) if path.starts_with("/api/v1/config") => Some(Permission::ViewConfig),
        ("GET", path) if path.starts_with("/api/v1/mining-stats") => Some(Permission::ViewMiningStats),
        
        // Write operations
        ("POST", path) if path.starts_with("/api/v1/templates") => Some(Permission::CreateTemplates),
        ("PUT", path) if path.starts_with("/api/v1/config") => Some(Permission::UpdateConfig),
        ("POST", path) if path.starts_with("/api/v1/config") => Some(Permission::UpdateConfig),
        ("DELETE", path) if path.starts_with("/api/v1/connections") => Some(Permission::ManageConnections),
        
        // Admin operations
        ("POST", path) if path.starts_with("/api/v1/daemon") => Some(Permission::AdminAccess),
        ("PUT", path) if path.starts_with("/api/v1/daemon") => Some(Permission::AdminAccess),
        ("DELETE", path) if path.starts_with("/api/v1/daemon") => Some(Permission::AdminAccess),
        
        // Default to API access for any other API endpoints
        (_, path) if path.starts_with("/api/") => Some(Permission::ApiAccess),
        
        // No authentication required for static files and WebSocket
        _ => None,
    }
}

/// Authentication middleware
pub async fn auth_middleware(
    State(auth_state): State<AuthMiddlewareState>,
    mut request: Request<Body>,
    next: Next<Body>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let path = request.uri().path();
    let method = request.method().as_str();
    
    // Check if authentication is required for this endpoint
    let required_permission = match get_required_permission(path, method) {
        Some(permission) => permission,
        None => {
            // No authentication required, proceed
            return Ok(next.run(request).await);
        }
    };
    
    // Check if authentication is enabled
    let auth_enabled = {
        let auth_system = auth_state.auth_system.read().await;
        auth_system.requires_auth(&required_permission)
    };
    
    if !auth_enabled {
        // Authentication disabled, proceed
        return Ok(next.run(request).await);
    }
    
    // Extract API key from headers
    let api_key = match extract_api_key(request.headers()) {
        Some(key) => key,
        None => {
            let error = json!({
                "error": "Authentication required",
                "message": "API key must be provided in Authorization header (Bearer token) or X-API-Key header",
                "code": 401
            });
            return Err((StatusCode::UNAUTHORIZED, Json(error)));
        }
    };
    
    let client_id = extract_client_id(request.headers());
    
    // Authenticate the request
    let session_id = {
        let mut auth_system = auth_state.auth_system.write().await;
        match auth_system.authenticate(&api_key, &client_id) {
            Ok(AuthResult::Success { session_id, .. }) => session_id,
            Ok(AuthResult::Failed { reason }) => {
                let error = json!({
                    "error": "Authentication failed",
                    "message": reason,
                    "code": 401
                });
                return Err((StatusCode::UNAUTHORIZED, Json(error)));
            }
            Ok(AuthResult::RateLimited { retry_after }) => {
                let error = json!({
                    "error": "Rate limited",
                    "message": "Too many authentication attempts",
                    "retry_after": retry_after,
                    "code": 429
                });
                return Err((StatusCode::TOO_MANY_REQUESTS, Json(error)));
            }
            Err(e) => {
                let error = json!({
                    "error": "Authentication error",
                    "message": e.to_string(),
                    "code": 500
                });
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)));
            }
        }
    };
    
    // Check authorization for the specific permission
    let authorized = {
        let mut auth_system = auth_state.auth_system.write().await;
        match auth_system.authorize(&session_id, &required_permission) {
            Ok(AuthzResult::Granted) => true,
            Ok(AuthzResult::Denied { required_permission }) => {
                let error = json!({
                    "error": "Authorization denied",
                    "message": format!("Insufficient permissions. Required: {:?}", required_permission),
                    "required_permission": format!("{:?}", required_permission),
                    "code": 403
                });
                return Err((StatusCode::FORBIDDEN, Json(error)));
            }
            Ok(AuthzResult::SessionInvalid) => {
                let error = json!({
                    "error": "Session invalid",
                    "message": "Session has expired or is invalid",
                    "code": 401
                });
                return Err((StatusCode::UNAUTHORIZED, Json(error)));
            }
            Err(e) => {
                let error = json!({
                    "error": "Authorization error",
                    "message": e.to_string(),
                    "code": 500
                });
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)));
            }
        }
    };
    
    if !authorized {
        let error = json!({
            "error": "Access denied",
            "message": "Insufficient permissions for this operation",
            "code": 403
        });
        return Err((StatusCode::FORBIDDEN, Json(error)));
    }
    
    // Add session information to request extensions for use in handlers
    request.extensions_mut().insert(SessionInfo {
        session_id,
        client_id,
        permission: required_permission,
    });
    
    Ok(next.run(request).await)
}

/// Session information added to request extensions
#[derive(Debug, Clone)]
pub struct SessionInfo {
    pub session_id: String,
    pub client_id: String,
    pub permission: Permission,
}

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(auth_state): State<AuthMiddlewareState>,
    request: Request<Body>,
    next: Next<Body>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let client_id = extract_client_id(request.headers());
    
    // Check rate limiting through auth system
    let rate_limited = {
        let auth_system = auth_state.auth_system.read().await;
        // This is a simplified check - in a real implementation, you'd want
        // separate rate limiting logic for different types of requests
        false // For now, we'll rely on the authentication rate limiting
    };
    
    if rate_limited {
        let error = json!({
            "error": "Rate limited",
            "message": "Too many requests from this client",
            "code": 429
        });
        return Err((StatusCode::TOO_MANY_REQUESTS, Json(error)));
    }
    
    Ok(next.run(request).await)
}

/// CORS middleware for API access
pub async fn cors_middleware(
    request: Request<Body>,
    next: Next<Body>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let mut response = next.run(request).await;
    
    let headers = response.headers_mut();
    headers.insert("Access-Control-Allow-Origin", "*".parse().unwrap());
    headers.insert("Access-Control-Allow-Methods", "GET, POST, PUT, DELETE, OPTIONS".parse().unwrap());
    headers.insert("Access-Control-Allow-Headers", "Content-Type, Authorization, X-API-Key".parse().unwrap());
    headers.insert("Access-Control-Max-Age", "86400".parse().unwrap());
    
    Ok(response)
}

/// Security headers middleware
pub async fn security_headers_middleware(
    request: Request<Body>,
    next: Next<Body>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let mut response = next.run(request).await;
    
    let headers = response.headers_mut();
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());
    headers.insert("Referrer-Policy", "strict-origin-when-cross-origin".parse().unwrap());
    headers.insert("Content-Security-Policy", "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'".parse().unwrap());
    
    Ok(response)
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Method, Request},
    };
    
    #[test]
    fn test_extract_api_key() {
        let mut headers = HeaderMap::new();
        
        // Test Bearer token
        headers.insert("authorization", "Bearer test-api-key".parse().unwrap());
        assert_eq!(extract_api_key(&headers), Some("test-api-key".to_string()));
        
        // Test X-API-Key header
        headers.clear();
        headers.insert("x-api-key", "test-api-key".parse().unwrap());
        assert_eq!(extract_api_key(&headers), Some("test-api-key".to_string()));
        
        // Test no API key
        headers.clear();
        assert_eq!(extract_api_key(&headers), None);
    }
    
    #[test]
    fn test_extract_client_id() {
        let mut headers = HeaderMap::new();
        
        // Test X-Forwarded-For
        headers.insert("x-forwarded-for", "192.168.1.1, 10.0.0.1".parse().unwrap());
        assert_eq!(extract_client_id(&headers), "192.168.1.1");
        
        // Test X-Real-IP
        headers.clear();
        headers.insert("x-real-ip", "192.168.1.2".parse().unwrap());
        assert_eq!(extract_client_id(&headers), "192.168.1.2");
        
        // Test fallback
        headers.clear();
        assert_eq!(extract_client_id(&headers), "unknown");
    }
    
    #[test]
    fn test_get_required_permission() {
        // Test read operations
        assert_eq!(get_required_permission("/api/v1/status", "GET"), Some(Permission::ViewMetrics));
        assert_eq!(get_required_permission("/api/v1/connections", "GET"), Some(Permission::ViewConnections));
        
        // Test write operations
        assert_eq!(get_required_permission("/api/v1/templates", "POST"), Some(Permission::CreateTemplates));
        assert_eq!(get_required_permission("/api/v1/config", "PUT"), Some(Permission::UpdateConfig));
        
        // Test no auth required
        assert_eq!(get_required_permission("/health", "GET"), None);
        assert_eq!(get_required_permission("/", "GET"), None);
        
        // Test admin operations
        assert_eq!(get_required_permission("/api/v1/daemon/start", "POST"), Some(Permission::AdminAccess));
    }
}