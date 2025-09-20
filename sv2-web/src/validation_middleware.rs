use axum::{
    extract::State,
    http::{StatusCode, Request},
    middleware::Next,
    response::{Json, Response},
    body::Body,
};
use serde_json::json;
use std::sync::Arc;
use sv2_core::{
    validation::{InputValidator, ValidationConfig, RateLimiter, RateLimitConfig},
};
use tokio::sync::RwLock;

/// Validation middleware state
#[derive(Clone)]
pub struct ValidationMiddlewareState {
    pub validator: Arc<InputValidator>,
    pub rate_limiter: Arc<RwLock<RateLimiter>>,
}

/// Extract client IP from request headers
fn extract_client_ip(headers: &axum::http::HeaderMap) -> String {
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

/// Rate limiting middleware
pub async fn rate_limit_middleware(
    State(validation_state): State<ValidationMiddlewareState>,
    request: Request<Body>,
    next: Next<Body>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let client_ip = extract_client_ip(request.headers());
    
    // Check rate limiting
    let allowed = {
        let mut rate_limiter = validation_state.rate_limiter.write().await;
        match rate_limiter.check_rate_limit(&client_ip) {
            Ok(allowed) => allowed,
            Err(e) => {
                let error = json!({
                    "error": "Rate limiting error",
                    "message": e.to_string(),
                    "code": 500
                });
                return Err((StatusCode::INTERNAL_SERVER_ERROR, Json(error)));
            }
        }
    };
    
    if !allowed {
        let error = json!({
            "error": "Rate limited",
            "message": "Too many requests from this IP address",
            "code": 429
        });
        return Err((StatusCode::TOO_MANY_REQUESTS, Json(error)));
    }
    
    Ok(next.run(request).await)
}

/// Input validation middleware for JSON payloads
pub async fn input_validation_middleware(
    State(validation_state): State<ValidationMiddlewareState>,
    mut request: Request<Body>,
    next: Next<Body>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let path = request.uri().path().to_string();
    let method = request.method().as_str().to_string();
    
    // Only validate POST/PUT requests with JSON payloads
    if !matches!(method.as_str(), "POST" | "PUT") {
        return Ok(next.run(request).await);
    }
    
    // Check Content-Type header
    let content_type = request.headers()
        .get("content-type")
        .and_then(|ct| ct.to_str().ok())
        .unwrap_or("");
    
    if !content_type.contains("application/json") {
        return Ok(next.run(request).await);
    }
    
    // Extract and validate the request body
    let body = match hyper::body::to_bytes(request.body_mut()).await {
        Ok(bytes) => bytes,
        Err(e) => {
            let error = json!({
                "error": "Invalid request body",
                "message": e.to_string(),
                "code": 400
            });
            return Err((StatusCode::BAD_REQUEST, Json(error)));
        }
    };
    
    // Parse JSON to validate structure
    let json_value: serde_json::Value = match serde_json::from_slice(&body) {
        Ok(value) => value,
        Err(e) => {
            let error = json!({
                "error": "Invalid JSON",
                "message": e.to_string(),
                "code": 400
            });
            return Err((StatusCode::BAD_REQUEST, Json(error)));
        }
    };
    
    // Validate JSON content based on endpoint
    if let Err(validation_error) = validate_json_for_endpoint(&path, &json_value, &validation_state.validator) {
        let error = json!({
            "error": "Validation failed",
            "message": validation_error,
            "code": 400
        });
        return Err((StatusCode::BAD_REQUEST, Json(error)));
    }
    
    // Reconstruct the request with the validated body
    *request.body_mut() = Body::from(body.to_vec());
    
    Ok(next.run(request).await)
}

/// Validate JSON content based on the endpoint
fn validate_json_for_endpoint(
    path: &str,
    json: &serde_json::Value,
    validator: &InputValidator,
) -> Result<(), String> {
    match path {
        "/api/v1/config" => validate_config_json(json, validator),
        "/api/v1/templates/custom" => validate_custom_template_json(json, validator),
        "/api/v1/auth/keys" => validate_api_key_generation_json(json, validator),
        _ => Ok(()), // No specific validation for other endpoints
    }
}

/// Validate configuration JSON
fn validate_config_json(json: &serde_json::Value, validator: &InputValidator) -> Result<(), String> {
    let config = json.get("config").ok_or("Missing 'config' field")?;
    
    // Validate Bitcoin configuration
    if let Some(bitcoin) = config.get("bitcoin") {
        if let Some(rpc_url) = bitcoin.get("rpc_url").and_then(|v| v.as_str()) {
            validator.validate_url(rpc_url)
                .map_err(|e| format!("Invalid Bitcoin RPC URL: {}", e))?;
        }
        
        if let Some(coinbase_address) = bitcoin.get("coinbase_address").and_then(|v| v.as_str()) {
            if !coinbase_address.is_empty() {
                validator.validate_bitcoin_address(coinbase_address)
                    .map_err(|e| format!("Invalid coinbase address: {}", e))?;
            }
        }
    }
    
    // Validate network configuration
    if let Some(network) = config.get("network") {
        if let Some(max_connections) = network.get("max_connections").and_then(|v| v.as_f64()) {
            if max_connections < 1.0 || max_connections > 100000.0 {
                return Err("max_connections must be between 1 and 100000".to_string());
            }
        }
        
        if let Some(connection_timeout) = network.get("connection_timeout").and_then(|v| v.as_f64()) {
            match validator.validate_numeric("connection_timeout", connection_timeout, "timeout") {
                sv2_core::validation::ValidationResult::Valid => {}
                sv2_core::validation::ValidationResult::Invalid { errors } => {
                    let error_messages: Vec<String> = errors.iter()
                        .map(|e| e.message.clone())
                        .collect();
                    return Err(format!("Invalid connection timeout: {}", error_messages.join(", ")));
                }
            }
        }
    }
    
    // Validate mode-specific configuration
    if let Some(mode) = config.get("mode") {
        match mode.get("type").and_then(|v| v.as_str()) {
            Some("Solo") => {
                if let Some(solo_config) = mode.get("config") {
                    if let Some(coinbase_address) = solo_config.get("coinbase_address").and_then(|v| v.as_str()) {
                        validator.validate_bitcoin_address(coinbase_address)
                            .map_err(|e| format!("Invalid solo coinbase address: {}", e))?;
                    }
                }
            }
            Some("Pool") => {
                if let Some(pool_config) = mode.get("config") {
                    if let Some(share_difficulty) = pool_config.get("share_difficulty").and_then(|v| v.as_f64()) {
                        match validator.validate_numeric("share_difficulty", share_difficulty, "difficulty") {
                            sv2_core::validation::ValidationResult::Valid => {}
                            sv2_core::validation::ValidationResult::Invalid { errors } => {
                                let error_messages: Vec<String> = errors.iter()
                                    .map(|e| e.message.clone())
                                    .collect();
                                return Err(format!("Invalid share difficulty: {}", error_messages.join(", ")));
                            }
                        }
                    }
                    
                    if let Some(fee_percentage) = pool_config.get("fee_percentage").and_then(|v| v.as_f64()) {
                        match validator.validate_numeric("fee_percentage", fee_percentage, "percentage") {
                            sv2_core::validation::ValidationResult::Valid => {}
                            sv2_core::validation::ValidationResult::Invalid { errors } => {
                                let error_messages: Vec<String> = errors.iter()
                                    .map(|e| e.message.clone())
                                    .collect();
                                return Err(format!("Invalid fee percentage: {}", error_messages.join(", ")));
                            }
                        }
                    }
                }
            }
            Some("Proxy") => {
                if let Some(proxy_config) = mode.get("config") {
                    if let Some(upstream_pools) = proxy_config.get("upstream_pools").and_then(|v| v.as_array()) {
                        match validator.validate_collection("upstream_pools", upstream_pools, "upstream_pools") {
                            sv2_core::validation::ValidationResult::Valid => {}
                            sv2_core::validation::ValidationResult::Invalid { errors } => {
                                let error_messages: Vec<String> = errors.iter()
                                    .map(|e| e.message.clone())
                                    .collect();
                                return Err(format!("Too many upstream pools: {}", error_messages.join(", ")));
                            }
                        }
                        
                        for (i, pool) in upstream_pools.iter().enumerate() {
                            if let Some(url) = pool.get("url").and_then(|v| v.as_str()) {
                                validator.validate_url(url)
                                    .map_err(|e| format!("Invalid upstream pool {} URL: {}", i, e))?;
                            }
                        }
                    }
                }
            }
            Some("Client") => {
                if let Some(client_config) = mode.get("config") {
                    if let Some(upstream_pool) = client_config.get("upstream_pool") {
                        if let Some(url) = upstream_pool.get("url").and_then(|v| v.as_str()) {
                            validator.validate_url(url)
                                .map_err(|e| format!("Invalid client upstream URL: {}", e))?;
                        }
                    }
                }
            }
            _ => {} // Unknown mode type, let the application handle it
        }
    }
    
    Ok(())
}

/// Validate custom template JSON
fn validate_custom_template_json(json: &serde_json::Value, validator: &InputValidator) -> Result<(), String> {
    // Validate transactions array
    if let Some(transactions) = json.get("transactions").and_then(|v| v.as_array()) {
        match validator.validate_collection("transactions", transactions, "transactions") {
            sv2_core::validation::ValidationResult::Valid => {}
            sv2_core::validation::ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                return Err(format!("Too many transactions: {}", error_messages.join(", ")));
            }
        }
        
        for (i, tx) in transactions.iter().enumerate() {
            if let Some(tx_hex) = tx.as_str() {
                validator.validate_hex_string(tx_hex)
                    .map_err(|e| format!("Invalid transaction {} hex: {}", i, e))?;
            }
        }
    }
    
    // Validate coinbase data if present
    if let Some(coinbase_data) = json.get("coinbase_data").and_then(|v| v.as_str()) {
        validator.validate_hex_string(coinbase_data)
            .map_err(|e| format!("Invalid coinbase data: {}", e))?;
    }
    
    // Validate difficulty if present
    if let Some(difficulty) = json.get("difficulty").and_then(|v| v.as_f64()) {
        match validator.validate_numeric("difficulty", difficulty, "difficulty") {
            sv2_core::validation::ValidationResult::Valid => {}
            sv2_core::validation::ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                return Err(format!("Invalid difficulty value: {}", error_messages.join(", ")));
            }
        }
    }
    
    Ok(())
}

/// Validate API key generation JSON
fn validate_api_key_generation_json(json: &serde_json::Value, validator: &InputValidator) -> Result<(), String> {
    // Validate name
    if let Some(name) = json.get("name").and_then(|v| v.as_str()) {
        let validation_result = validator.validate_string("name", name, "name");
        match validation_result {
            sv2_core::validation::ValidationResult::Valid => {}
            sv2_core::validation::ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                return Err(format!("Invalid name: {}", error_messages.join(", ")));
            }
        }
    }
    
    // Validate permissions array
    if let Some(permissions) = json.get("permissions").and_then(|v| v.as_array()) {
        match validator.validate_collection("permissions", permissions, "permissions") {
            sv2_core::validation::ValidationResult::Valid => {}
            sv2_core::validation::ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                return Err(format!("Too many permissions: {}", error_messages.join(", ")));
            }
        }
    }
    
    // Validate expires_at if present
    if let Some(expires_at) = json.get("expires_at").and_then(|v| v.as_f64()) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs() as f64;
        
        if expires_at <= now {
            return Err("Expiration time must be in the future".to_string());
        }
        
        // Don't allow expiration more than 10 years in the future
        if expires_at > now + (10 * 365 * 24 * 3600) as f64 {
            return Err("Expiration time too far in the future".to_string());
        }
    }
    
    Ok(())
}

/// Content Security Policy middleware
pub async fn csp_middleware(
    request: Request<Body>,
    next: Next<Body>,
) -> Result<Response, (StatusCode, Json<serde_json::Value>)> {
    let mut response = next.run(request).await;
    
    let headers = response.headers_mut();
    
    // Add Content Security Policy
    headers.insert(
        "Content-Security-Policy",
        "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; connect-src 'self'; font-src 'self'; object-src 'none'; media-src 'self'; frame-src 'none';"
            .parse()
            .unwrap(),
    );
    
    // Add other security headers
    headers.insert("X-Content-Type-Options", "nosniff".parse().unwrap());
    headers.insert("X-Frame-Options", "DENY".parse().unwrap());
    headers.insert("X-XSS-Protection", "1; mode=block".parse().unwrap());
    headers.insert("Referrer-Policy", "strict-origin-when-cross-origin".parse().unwrap());
    
    Ok(response)
}

impl ValidationMiddlewareState {
    /// Create new validation middleware state
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let validation_config = ValidationConfig::default();
        let validator = Arc::new(InputValidator::new(validation_config)?);
        
        let rate_limit_config = RateLimitConfig {
            max_requests: 100,
            window_duration: 60,
            block_duration: 300,
        };
        let rate_limiter = Arc::new(RwLock::new(RateLimiter::new(rate_limit_config)));
        
        Ok(Self {
            validator,
            rate_limiter,
        })
    }
    
    /// Cleanup rate limiter periodically
    pub async fn cleanup_rate_limiter(&self) {
        let mut rate_limiter = self.rate_limiter.write().await;
        rate_limiter.cleanup();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    
    #[test]
    fn test_config_validation() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();
        
        // Valid config
        let valid_config = json!({
            "config": {
                "bitcoin": {
                    "rpc_url": "http://localhost:8332",
                    "coinbase_address": "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
                },
                "network": {
                    "max_connections": 1000,
                    "connection_timeout": 30
                }
            }
        });
        
        assert!(validate_config_json(&valid_config, &validator).is_ok());
        
        // Invalid config (bad URL)
        let invalid_config = json!({
            "config": {
                "bitcoin": {
                    "rpc_url": "not-a-url",
                    "coinbase_address": "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"
                }
            }
        });
        
        assert!(validate_config_json(&invalid_config, &validator).is_err());
    }
    
    #[test]
    fn test_custom_template_validation() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();
        
        // Valid template
        let valid_template = json!({
            "transactions": ["deadbeef", "cafebabe"],
            "coinbase_data": "01234567",
            "difficulty": 1000.0
        });
        
        assert!(validate_custom_template_json(&valid_template, &validator).is_ok());
        
        // Invalid template (bad hex)
        let invalid_template = json!({
            "transactions": ["not-hex"],
            "difficulty": 1000.0
        });
        
        assert!(validate_custom_template_json(&invalid_template, &validator).is_err());
    }
    
    #[test]
    fn test_api_key_generation_validation() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();
        
        // Valid request
        let valid_request = json!({
            "name": "test-key",
            "permissions": ["view_connections", "view_shares"],
            "expires_at": (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() + 3600) as f64
        });
        
        assert!(validate_api_key_generation_json(&valid_request, &validator).is_ok());
        
        // Invalid request (expired time)
        let invalid_request = json!({
            "name": "test-key",
            "permissions": ["view_connections"],
            "expires_at": 1000.0  // Way in the past
        });
        
        assert!(validate_api_key_generation_json(&invalid_request, &validator).is_err());
    }
}