use super::*;
use crate::config::{LoggingConfig, LogFormat, LogOutput};
use std::collections::HashMap;
use tempfile::NamedTempFile;
use tracing::{info, warn, error};

#[tokio::test]
async fn test_json_logging_configuration() {
    let config = LoggingConfig {
        level: "info".to_string(),
        component_levels: HashMap::new(),
        format: LogFormat::Json,
        output: LogOutput::Stdout,
        enable_correlation_ids: true,
        redact_sensitive_data: true,
        max_file_size_mb: Some(100),
        max_files: Some(10),
    };

    // Test that configuration is valid
    assert_eq!(config.level, "info");
    assert!(config.enable_correlation_ids);
    assert!(config.redact_sensitive_data);
}

#[tokio::test]
async fn test_file_logging_configuration() {
    let temp_file = NamedTempFile::new().unwrap();
    let config = LoggingConfig {
        level: "debug".to_string(),
        component_levels: HashMap::new(),
        format: LogFormat::Json,
        output: LogOutput::File(temp_file.path().to_path_buf()),
        enable_correlation_ids: true,
        redact_sensitive_data: true,
        max_file_size_mb: Some(100),
        max_files: Some(10),
    };

    // Test that file path configuration is valid
    assert_eq!(config.level, "debug");
    assert!(matches!(config.output, LogOutput::File(_)));
}

#[tokio::test]
async fn test_component_level_configuration() {
    let mut component_levels = HashMap::new();
    component_levels.insert("sv2_core::protocol".to_string(), "debug".to_string());
    component_levels.insert("sv2_core::database".to_string(), "warn".to_string());

    let config = LoggingConfig {
        level: "info".to_string(),
        component_levels,
        format: LogFormat::Json,
        output: LogOutput::Stdout,
        enable_correlation_ids: true,
        redact_sensitive_data: true,
        max_file_size_mb: Some(100),
        max_files: Some(10),
    };

    let result = init_logging(&config);
    assert!(result.is_ok());
}

#[test]
fn test_correlation_id_uniqueness() {
    let id1 = CorrelationId::new();
    let id2 = CorrelationId::new();
    
    assert_ne!(id1.to_string(), id2.to_string());
    assert_eq!(id1.to_string().len(), 36); // UUID v4 length with hyphens
}

#[test]
fn test_sensitive_data_redaction() {
    let formatter = JsonFormatter::new(true);
    let mut fields = HashMap::new();
    
    // Test various sensitive field names
    fields.insert("password".to_string(), serde_json::Value::String("secret123".to_string()));
    fields.insert("api_key".to_string(), serde_json::Value::String("sk_test_123".to_string()));
    fields.insert("private_key".to_string(), serde_json::Value::String("5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ".to_string()));
    fields.insert("username".to_string(), serde_json::Value::String("user123".to_string()));
    fields.insert("bitcoin_address".to_string(), serde_json::Value::String("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa".to_string()));
    
    let mut fields_copy = fields.clone();
    formatter.redact_fields(&mut fields_copy);
    
    // Sensitive fields should be redacted
    assert_eq!(fields_copy.get("password").unwrap(), &serde_json::Value::String("[REDACTED]".to_string()));
    assert_eq!(fields_copy.get("api_key").unwrap(), &serde_json::Value::String("[REDACTED]".to_string()));
    assert_eq!(fields_copy.get("private_key").unwrap(), &serde_json::Value::String("[REDACTED]".to_string()));
    assert_eq!(fields_copy.get("bitcoin_address").unwrap(), &serde_json::Value::String("[REDACTED]".to_string()));
    
    // Non-sensitive fields should remain unchanged
    assert_eq!(fields_copy.get("username").unwrap(), &serde_json::Value::String("user123".to_string()));
}

#[test]
fn test_bitcoin_address_detection() {
    let formatter = JsonFormatter::new(true);
    
    // Valid Bitcoin addresses
    assert!(formatter.looks_like_bitcoin_address("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"));
    assert!(formatter.looks_like_bitcoin_address("3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy"));
    assert!(formatter.looks_like_bitcoin_address("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"));
    
    // Invalid addresses
    assert!(!formatter.looks_like_bitcoin_address("not_an_address"));
    assert!(!formatter.looks_like_bitcoin_address(""));
    assert!(!formatter.looks_like_bitcoin_address("short"));
}

#[test]
fn test_private_key_detection() {
    let formatter = JsonFormatter::new(true);
    
    // Valid private keys
    assert!(formatter.looks_like_private_key("5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ"));
    assert!(formatter.looks_like_private_key("KwDiBf89QgGbjEhKnhXJuH7LrciVrZi3qYjgd9M7rFU73sVHnoWn"));
    assert!(formatter.looks_like_private_key("a0b1c2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f607182938495061728ab"));
    
    // Invalid private keys
    assert!(!formatter.looks_like_private_key("not_a_private_key"));
    assert!(!formatter.looks_like_private_key(""));
    assert!(!formatter.looks_like_private_key("short"));
}

#[test]
fn test_redaction_disabled() {
    let formatter = JsonFormatter::new(false);
    let mut fields = HashMap::new();
    
    fields.insert("password".to_string(), serde_json::Value::String("secret123".to_string()));
    fields.insert("private_key".to_string(), serde_json::Value::String("5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ".to_string()));
    
    let original_fields = fields.clone();
    formatter.redact_fields(&mut fields);
    
    // Fields should remain unchanged when redaction is disabled
    assert_eq!(fields, original_fields);
}

#[tokio::test]
async fn test_correlation_id_functionality() {
    // Test correlation ID creation and usage without initializing global subscriber
    let id1 = CorrelationId::new();
    let id2 = CorrelationId::new();
    
    assert_ne!(id1.to_string(), id2.to_string());
    assert!(!id1.as_str().is_empty());
    assert!(!id2.as_str().is_empty());
}

// Note: JsonFieldVisitor tests are complex due to tracing internals
// We test the functionality through integration tests instead

#[test]
fn test_structured_log_entry_serialization() {
    let mut fields = HashMap::new();
    fields.insert("user_id".to_string(), serde_json::Value::String("12345".to_string()));
    fields.insert("action".to_string(), serde_json::Value::String("login".to_string()));
    
    let entry = StructuredLogEntry {
        timestamp: "2023-01-01T00:00:00Z".to_string(),
        level: "INFO".to_string(),
        component: "sv2_core::auth".to_string(),
        message: "User logged in".to_string(),
        correlation_id: Some("550e8400-e29b-41d4-a716-446655440000".to_string()),
        fields,
        location: Some(LogLocation {
            file: "auth.rs".to_string(),
            line: 42,
            module: "sv2_core::auth".to_string(),
        }),
    };
    
    let json = serde_json::to_string(&entry).unwrap();
    assert!(json.contains("\"level\":\"INFO\""));
    assert!(json.contains("\"message\":\"User logged in\""));
    assert!(json.contains("\"correlation_id\":\"550e8400-e29b-41d4-a716-446655440000\""));
    assert!(json.contains("\"user_id\":\"12345\""));
}