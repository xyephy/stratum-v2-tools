use crate::{Result, Error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use regex::Regex;

/// Input validation and sanitization system
pub struct InputValidator {
    /// Configuration for validation rules
    config: ValidationConfig,
    /// Compiled regex patterns for performance
    patterns: HashMap<String, Regex>,
}

/// Configuration for input validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationConfig {
    /// Maximum string length for various input types
    pub max_lengths: HashMap<String, usize>,
    /// Allowed characters for different input types
    pub allowed_patterns: HashMap<String, String>,
    /// Whether to enable strict validation
    pub strict_mode: bool,
    /// Maximum array/collection sizes
    pub max_collection_sizes: HashMap<String, usize>,
    /// Numeric ranges for validation
    pub numeric_ranges: HashMap<String, NumericRange>,
}

/// Numeric range validation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NumericRange {
    pub min: f64,
    pub max: f64,
}

/// Validation result
#[derive(Debug, Clone)]
pub enum ValidationResult {
    Valid,
    Invalid { errors: Vec<ValidationError> },
}

/// Validation error details
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationError {
    pub field: String,
    pub error_type: ValidationErrorType,
    pub message: String,
    pub value: Option<String>,
}

/// Types of validation errors
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ValidationErrorType {
    TooLong,
    TooShort,
    InvalidFormat,
    InvalidCharacters,
    OutOfRange,
    TooManyItems,
    Required,
    InvalidType,
    Malicious,
}

/// Sanitization options
#[derive(Debug, Clone)]
pub struct SanitizationOptions {
    /// Remove HTML tags
    pub strip_html: bool,
    /// Remove SQL injection patterns
    pub strip_sql: bool,
    /// Remove script injection patterns
    pub strip_scripts: bool,
    /// Normalize whitespace
    pub normalize_whitespace: bool,
    /// Convert to lowercase
    pub to_lowercase: bool,
    /// Trim whitespace
    pub trim: bool,
}

impl Default for ValidationConfig {
    fn default() -> Self {
        let mut max_lengths = HashMap::new();
        max_lengths.insert("api_key".to_string(), 256);
        max_lengths.insert("username".to_string(), 64);
        max_lengths.insert("password".to_string(), 128);
        max_lengths.insert("email".to_string(), 254);
        max_lengths.insert("url".to_string(), 2048);
        max_lengths.insert("name".to_string(), 128);
        max_lengths.insert("description".to_string(), 1024);
        max_lengths.insert("address".to_string(), 128);
        max_lengths.insert("worker_name".to_string(), 64);
        max_lengths.insert("user_agent".to_string(), 256);
        max_lengths.insert("session_id".to_string(), 128);

        let mut allowed_patterns = HashMap::new();
        allowed_patterns.insert("api_key".to_string(), r"^[A-Za-z0-9_\-+=/.]{1,256}$".to_string());
        allowed_patterns.insert("username".to_string(), r"^[A-Za-z0-9_\-\.]{1,64}$".to_string());
        allowed_patterns.insert("email".to_string(), r"^[A-Za-z0-9._%+-]+@[A-Za-z0-9.-]+\.[A-Za-z]{2,}$".to_string());
        allowed_patterns.insert("url".to_string(), r"^https?://[A-Za-z0-9\-\._~:/?#\[\]@!$&'()*+,;=%]+$".to_string());
        allowed_patterns.insert("bitcoin_address".to_string(), r"^[13][a-km-zA-HJ-NP-Z1-9]{25,34}$|^bc1[a-z0-9]{39,59}$|^tb1[a-z0-9]{39,59}$".to_string());
        allowed_patterns.insert("worker_name".to_string(), r"^[A-Za-z0-9_\-\.]{1,64}$".to_string());
        allowed_patterns.insert("hex".to_string(), r"^[0-9a-fA-F]*$".to_string());
        allowed_patterns.insert("uuid".to_string(), r"^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}$".to_string());
        allowed_patterns.insert("ip_address".to_string(), r"^(?:(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)\.){3}(?:25[0-5]|2[0-4][0-9]|[01]?[0-9][0-9]?)$|^(?:[0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}$".to_string());

        let mut max_collection_sizes = HashMap::new();
        max_collection_sizes.insert("permissions".to_string(), 50);
        max_collection_sizes.insert("upstream_pools".to_string(), 10);
        max_collection_sizes.insert("transactions".to_string(), 10000);
        max_collection_sizes.insert("connections".to_string(), 10000);

        let mut numeric_ranges = HashMap::new();
        numeric_ranges.insert("difficulty".to_string(), NumericRange { min: 0.0001, max: 1e15 });
        numeric_ranges.insert("hashrate".to_string(), NumericRange { min: 0.0, max: 1e20 });
        numeric_ranges.insert("port".to_string(), NumericRange { min: 1.0, max: 65535.0 });
        numeric_ranges.insert("timeout".to_string(), NumericRange { min: 1.0, max: 86400.0 });
        numeric_ranges.insert("percentage".to_string(), NumericRange { min: 0.0, max: 100.0 });
        numeric_ranges.insert("rate_limit".to_string(), NumericRange { min: 1.0, max: 10000.0 });

        Self {
            max_lengths,
            allowed_patterns,
            strict_mode: true,
            max_collection_sizes,
            numeric_ranges,
        }
    }
}

impl Default for SanitizationOptions {
    fn default() -> Self {
        Self {
            strip_html: true,
            strip_sql: true,
            strip_scripts: true,
            normalize_whitespace: true,
            to_lowercase: false,
            trim: true,
        }
    }
}

impl InputValidator {
    /// Create a new input validator
    pub fn new(config: ValidationConfig) -> Result<Self> {
        let mut patterns = HashMap::new();
        
        // Compile all regex patterns
        for (key, pattern) in &config.allowed_patterns {
            let regex = Regex::new(pattern)
                .map_err(|e| Error::Config(format!("Invalid regex pattern for {}: {}", key, e)))?;
            patterns.insert(key.clone(), regex);
        }

        Ok(Self { config, patterns })
    }

    /// Validate a string input
    pub fn validate_string(&self, field: &str, value: &str, input_type: &str) -> ValidationResult {
        let mut errors = Vec::new();

        // Check if required field is empty
        if value.is_empty() && self.is_required_field(field) {
            errors.push(ValidationError {
                field: field.to_string(),
                error_type: ValidationErrorType::Required,
                message: format!("Field '{}' is required", field),
                value: None,
            });
            return ValidationResult::Invalid { errors };
        }

        // Skip validation for empty optional fields
        if value.is_empty() {
            return ValidationResult::Valid;
        }

        // Check length limits
        if let Some(&max_length) = self.config.max_lengths.get(input_type) {
            if value.len() > max_length {
                errors.push(ValidationError {
                    field: field.to_string(),
                    error_type: ValidationErrorType::TooLong,
                    message: format!("Field '{}' exceeds maximum length of {}", field, max_length),
                    value: Some(value.to_string()),
                });
            }
        }

        // Check pattern matching
        if let Some(pattern) = self.patterns.get(input_type) {
            if !pattern.is_match(value) {
                errors.push(ValidationError {
                    field: field.to_string(),
                    error_type: ValidationErrorType::InvalidFormat,
                    message: format!("Field '{}' has invalid format", field),
                    value: Some(value.to_string()),
                });
            }
        }

        // Check for malicious patterns
        if self.config.strict_mode && self.contains_malicious_patterns(value) {
            errors.push(ValidationError {
                field: field.to_string(),
                error_type: ValidationErrorType::Malicious,
                message: format!("Field '{}' contains potentially malicious content", field),
                value: Some(value.to_string()),
            });
        }

        if errors.is_empty() {
            ValidationResult::Valid
        } else {
            ValidationResult::Invalid { errors }
        }
    }

    /// Validate a numeric value
    pub fn validate_numeric(&self, field: &str, value: f64, numeric_type: &str) -> ValidationResult {
        let mut errors = Vec::new();

        if let Some(range) = self.config.numeric_ranges.get(numeric_type) {
            if value < range.min || value > range.max {
                errors.push(ValidationError {
                    field: field.to_string(),
                    error_type: ValidationErrorType::OutOfRange,
                    message: format!("Field '{}' must be between {} and {}", field, range.min, range.max),
                    value: Some(value.to_string()),
                });
            }
        }

        // Check for special numeric values
        if !value.is_finite() {
            errors.push(ValidationError {
                field: field.to_string(),
                error_type: ValidationErrorType::InvalidType,
                message: format!("Field '{}' must be a finite number", field),
                value: Some(value.to_string()),
            });
        }

        if errors.is_empty() {
            ValidationResult::Valid
        } else {
            ValidationResult::Invalid { errors }
        }
    }

    /// Validate a collection (array/vector)
    pub fn validate_collection<T>(&self, field: &str, collection: &[T], collection_type: &str) -> ValidationResult {
        let mut errors = Vec::new();

        if let Some(&max_size) = self.config.max_collection_sizes.get(collection_type) {
            if collection.len() > max_size {
                errors.push(ValidationError {
                    field: field.to_string(),
                    error_type: ValidationErrorType::TooManyItems,
                    message: format!("Field '{}' exceeds maximum size of {}", field, max_size),
                    value: Some(collection.len().to_string()),
                });
            }
        }

        if errors.is_empty() {
            ValidationResult::Valid
        } else {
            ValidationResult::Invalid { errors }
        }
    }

    /// Sanitize a string input
    pub fn sanitize_string(&self, input: &str, options: &SanitizationOptions) -> String {
        let mut result = input.to_string();

        // Trim whitespace
        if options.trim {
            result = result.trim().to_string();
        }

        // Strip HTML tags
        if options.strip_html {
            result = self.strip_html_tags(&result);
        }

        // Strip SQL injection patterns
        if options.strip_sql {
            result = self.strip_sql_patterns(&result);
        }

        // Strip script injection patterns
        if options.strip_scripts {
            result = self.strip_script_patterns(&result);
        }

        // Normalize whitespace
        if options.normalize_whitespace {
            result = self.normalize_whitespace(&result);
        }

        // Convert to lowercase
        if options.to_lowercase {
            result = result.to_lowercase();
        }

        result
    }

    /// Validate and sanitize API key
    pub fn validate_api_key(&self, api_key: &str) -> Result<String> {
        let validation_result = self.validate_string("api_key", api_key, "api_key");
        
        match validation_result {
            ValidationResult::Valid => {
                let sanitized = self.sanitize_string(api_key, &SanitizationOptions {
                    strip_html: true,
                    strip_sql: true,
                    strip_scripts: true,
                    normalize_whitespace: false,
                    to_lowercase: false,
                    trim: true,
                });
                Ok(sanitized)
            }
            ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                Err(Error::Authentication(format!("Invalid API key: {}", error_messages.join(", "))))
            }
        }
    }

    /// Validate Bitcoin address
    pub fn validate_bitcoin_address(&self, address: &str) -> Result<String> {
        let validation_result = self.validate_string("bitcoin_address", address, "bitcoin_address");
        
        match validation_result {
            ValidationResult::Valid => {
                let sanitized = self.sanitize_string(address, &SanitizationOptions {
                    trim: true,
                    to_lowercase: false,
                    ..SanitizationOptions::default()
                });
                Ok(sanitized)
            }
            ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                Err(Error::Config(format!("Invalid Bitcoin address: {}", error_messages.join(", "))))
            }
        }
    }

    /// Validate URL
    pub fn validate_url(&self, url: &str) -> Result<String> {
        let validation_result = self.validate_string("url", url, "url");
        
        match validation_result {
            ValidationResult::Valid => {
                let sanitized = self.sanitize_string(url, &SanitizationOptions {
                    trim: true,
                    to_lowercase: false,
                    strip_html: true,
                    strip_sql: true,
                    strip_scripts: true,
                    normalize_whitespace: false,
                });
                Ok(sanitized)
            }
            ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                Err(Error::Config(format!("Invalid URL: {}", error_messages.join(", "))))
            }
        }
    }

    /// Validate worker name
    pub fn validate_worker_name(&self, name: &str) -> Result<String> {
        let validation_result = self.validate_string("worker_name", name, "worker_name");
        
        match validation_result {
            ValidationResult::Valid => {
                let sanitized = self.sanitize_string(name, &SanitizationOptions {
                    trim: true,
                    to_lowercase: false,
                    ..SanitizationOptions::default()
                });
                Ok(sanitized)
            }
            ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                Err(Error::Config(format!("Invalid worker name: {}", error_messages.join(", "))))
            }
        }
    }

    /// Validate hex string
    pub fn validate_hex_string(&self, hex: &str) -> Result<String> {
        let validation_result = self.validate_string("hex", hex, "hex");
        
        match validation_result {
            ValidationResult::Valid => {
                let sanitized = self.sanitize_string(hex, &SanitizationOptions {
                    trim: true,
                    to_lowercase: true,
                    strip_html: true,
                    strip_sql: true,
                    strip_scripts: true,
                    normalize_whitespace: false,
                });
                Ok(sanitized)
            }
            ValidationResult::Invalid { errors } => {
                let error_messages: Vec<String> = errors.iter()
                    .map(|e| e.message.clone())
                    .collect();
                Err(Error::Protocol(format!("Invalid hex string: {}", error_messages.join(", "))))
            }
        }
    }

    // Private helper methods

    fn is_required_field(&self, field: &str) -> bool {
        // Define required fields
        matches!(field, "api_key" | "bitcoin_address" | "url" | "worker_name")
    }

    fn contains_malicious_patterns(&self, input: &str) -> bool {
        let malicious_patterns = [
            // SQL injection patterns
            r"(?i)(union|select|insert|update|delete|drop|create|alter|exec|execute)",
            r"(?i)(script|javascript|vbscript|onload|onerror|onclick)",
            r"(?i)(<script|</script|<iframe|</iframe)",
            // XSS patterns
            r"(?i)(alert\s*\(|confirm\s*\(|prompt\s*\()",
            // Path traversal
            r"(\.\./|\.\.\\)",
            // Command injection
            r"(?i)(;|\||&|`|\$\(|\${)",
        ];

        for pattern in &malicious_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                if regex.is_match(input) {
                    return true;
                }
            }
        }

        false
    }

    fn strip_html_tags(&self, input: &str) -> String {
        let html_regex = Regex::new(r"<[^>]*>").unwrap();
        html_regex.replace_all(input, "").to_string()
    }

    fn strip_sql_patterns(&self, input: &str) -> String {
        let sql_patterns = [
            r"(?i)\b(union|select|insert|update|delete|drop|create|alter|exec|execute)\b",
            r"(?i)\b(or|and)\s+\d+\s*=\s*\d+",
            r"(?i)'\s*(or|and)\s+'",
        ];

        let mut result = input.to_string();
        for pattern in &sql_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                result = regex.replace_all(&result, "").to_string();
            }
        }
        result
    }

    fn strip_script_patterns(&self, input: &str) -> String {
        let script_patterns = [
            r"(?i)<script[^>]*>.*?</script>",
            r"(?i)javascript:",
            r"(?i)vbscript:",
            r"(?i)on\w+\s*=",
        ];

        let mut result = input.to_string();
        for pattern in &script_patterns {
            if let Ok(regex) = Regex::new(pattern) {
                result = regex.replace_all(&result, "").to_string();
            }
        }
        result
    }

    fn normalize_whitespace(&self, input: &str) -> String {
        let whitespace_regex = Regex::new(r"\s+").unwrap();
        whitespace_regex.replace_all(input.trim(), " ").to_string()
    }
}

/// Rate limiting for input validation
pub struct RateLimiter {
    /// Request counts per client
    request_counts: HashMap<String, RequestCount>,
    /// Configuration
    config: RateLimitConfig,
}

/// Rate limiting configuration
#[derive(Debug, Clone)]
pub struct RateLimitConfig {
    /// Maximum requests per window
    pub max_requests: u32,
    /// Window duration in seconds
    pub window_duration: u64,
    /// Block duration in seconds
    pub block_duration: u64,
}

/// Request count tracking
#[derive(Debug, Clone)]
struct RequestCount {
    count: u32,
    window_start: u64,
    blocked_until: Option<u64>,
}

impl RateLimiter {
    /// Create a new rate limiter
    pub fn new(config: RateLimitConfig) -> Self {
        Self {
            request_counts: HashMap::new(),
            config,
        }
    }

    /// Check if a request should be allowed
    pub fn check_rate_limit(&mut self, client_id: &str) -> Result<bool> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let entry = self.request_counts.entry(client_id.to_string())
            .or_insert_with(|| RequestCount {
                count: 0,
                window_start: now,
                blocked_until: None,
            });

        // Check if client is blocked
        if let Some(blocked_until) = entry.blocked_until {
            if now < blocked_until {
                return Ok(false);
            } else {
                entry.blocked_until = None;
            }
        }

        // Reset window if needed
        if now - entry.window_start >= self.config.window_duration {
            entry.count = 0;
            entry.window_start = now;
        }

        // Check rate limit
        if entry.count >= self.config.max_requests {
            entry.blocked_until = Some(now + self.config.block_duration);
            return Ok(false);
        }

        entry.count += 1;
        Ok(true)
    }

    /// Clean up old entries
    pub fn cleanup(&mut self) {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.request_counts.retain(|_, entry| {
            // Keep entries that are still in current window or blocked
            (now - entry.window_start < self.config.window_duration * 2) ||
            entry.blocked_until.map_or(false, |blocked| blocked > now)
        });
    }
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            max_requests: 100,
            window_duration: 60,
            block_duration: 300,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_string_validation() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();

        // Valid API key
        let result = validator.validate_string("api_key", "abc123_-+=", "api_key");
        assert!(matches!(result, ValidationResult::Valid));

        // Invalid API key (too long)
        let long_key = "a".repeat(300);
        let result = validator.validate_string("api_key", &long_key, "api_key");
        assert!(matches!(result, ValidationResult::Invalid { .. }));

        // Valid Bitcoin address
        let result = validator.validate_string("address", "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", "bitcoin_address");
        assert!(matches!(result, ValidationResult::Valid));

        // Invalid Bitcoin address
        let result = validator.validate_string("address", "invalid_address", "bitcoin_address");
        assert!(matches!(result, ValidationResult::Invalid { .. }));
    }

    #[test]
    fn test_numeric_validation() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();

        // Valid difficulty
        let result = validator.validate_numeric("difficulty", 1000.0, "difficulty");
        assert!(matches!(result, ValidationResult::Valid));

        // Invalid difficulty (too low)
        let result = validator.validate_numeric("difficulty", 0.0, "difficulty");
        assert!(matches!(result, ValidationResult::Invalid { .. }));

        // Invalid difficulty (infinite)
        let result = validator.validate_numeric("difficulty", f64::INFINITY, "difficulty");
        assert!(matches!(result, ValidationResult::Invalid { .. }));
    }

    #[test]
    fn test_collection_validation() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();

        // Valid collection
        let items = vec![1, 2, 3];
        let result = validator.validate_collection("permissions", &items, "permissions");
        assert!(matches!(result, ValidationResult::Valid));

        // Invalid collection (too many items)
        let items: Vec<i32> = (0..100).collect();
        let result = validator.validate_collection("permissions", &items, "permissions");
        assert!(matches!(result, ValidationResult::Invalid { .. }));
    }

    #[test]
    fn test_sanitization() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();
        let options = SanitizationOptions::default();

        // Test HTML stripping
        let input = "<script>alert('xss')</script>Hello World";
        let result = validator.sanitize_string(input, &options);
        assert!(!result.contains("<script>"));
        assert!(result.contains("Hello World"));

        // Test whitespace normalization
        let input = "  Hello    World  ";
        let result = validator.sanitize_string(input, &options);
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_malicious_pattern_detection() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();

        // SQL injection
        let result = validator.validate_string("input", "'; DROP TABLE users; --", "username");
        assert!(matches!(result, ValidationResult::Invalid { .. }));

        // XSS
        let result = validator.validate_string("input", "<script>alert('xss')</script>", "username");
        assert!(matches!(result, ValidationResult::Invalid { .. }));

        // Path traversal
        let result = validator.validate_string("input", "../../../etc/passwd", "username");
        assert!(matches!(result, ValidationResult::Invalid { .. }));
    }

    #[test]
    fn test_api_key_validation() {
        let validator = InputValidator::new(ValidationConfig::default()).unwrap();

        // Valid API key
        let result = validator.validate_api_key("abc123_-+=");
        assert!(result.is_ok());

        // Invalid API key
        let result = validator.validate_api_key("<script>alert('xss')</script>");
        assert!(result.is_err());
    }

    #[test]
    fn test_rate_limiting() {
        let config = RateLimitConfig {
            max_requests: 2,
            window_duration: 60,
            block_duration: 300,
        };
        let mut limiter = RateLimiter::new(config);

        // First two requests should be allowed
        assert!(limiter.check_rate_limit("client1").unwrap());
        assert!(limiter.check_rate_limit("client1").unwrap());

        // Third request should be blocked
        assert!(!limiter.check_rate_limit("client1").unwrap());

        // Different client should still be allowed
        assert!(limiter.check_rate_limit("client2").unwrap());
    }
}