use crate::config::{LoggingConfig, LogFormat, LogOutput};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use tracing::{Event, Subscriber};
use tracing_subscriber::{
    fmt::{format::Writer, FormatEvent, FormatFields},
    layer::SubscriberExt,
    registry::LookupSpan,
    util::SubscriberInitExt,
    EnvFilter, Layer,
};
use uuid::Uuid;

/// Correlation ID for request tracing
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CorrelationId(Uuid);

impl CorrelationId {
    /// Generate a new correlation ID
    pub fn new() -> Self {
        Self(Uuid::new_v4())
    }

    /// Get the correlation ID as a string
    pub fn as_str(&self) -> String {
        self.0.to_string()
    }
}

impl fmt::Display for CorrelationId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Default for CorrelationId {
    fn default() -> Self {
        Self::new()
    }
}

/// Structured log entry for JSON output
#[derive(Debug, Serialize)]
pub struct StructuredLogEntry {
    /// Timestamp in RFC3339 format
    pub timestamp: String,
    /// Log level
    pub level: String,
    /// Component/module name
    pub component: String,
    /// Log message
    pub message: String,
    /// Correlation ID for request tracing
    #[serde(skip_serializing_if = "Option::is_none")]
    pub correlation_id: Option<String>,
    /// Additional structured fields
    #[serde(flatten)]
    pub fields: HashMap<String, serde_json::Value>,
    /// File and line information
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<LogLocation>,
}

/// Source code location information
#[derive(Debug, Serialize)]
pub struct LogLocation {
    pub file: String,
    pub line: u32,
    pub module: String,
}

/// Custom JSON formatter for structured logging
pub struct JsonFormatter {
    redact_sensitive: bool,
}

impl JsonFormatter {
    pub fn new(redact_sensitive: bool) -> Self {
        Self { redact_sensitive }
    }

    /// Redact sensitive data from log fields
    fn redact_fields(&self, fields: &mut HashMap<String, serde_json::Value>) {
        if !self.redact_sensitive {
            return;
        }

        let sensitive_keys = [
            "password", "secret", "key", "token", "auth", "credential",
            "private_key", "wallet", "seed", "mnemonic", "passphrase",
        ];

        for (key, value) in fields.iter_mut() {
            let key_lower = key.to_lowercase();
            if sensitive_keys.iter().any(|&sensitive| key_lower.contains(sensitive)) {
                *value = serde_json::Value::String("[REDACTED]".to_string());
            } else if let serde_json::Value::String(s) = value {
                // Redact potential Bitcoin addresses and private keys
                if self.looks_like_bitcoin_address(s) || self.looks_like_private_key(s) {
                    *value = serde_json::Value::String("[REDACTED]".to_string());
                }
            }
        }
    }

    fn looks_like_bitcoin_address(&self, s: &str) -> bool {
        // Simple heuristic for Bitcoin addresses
        (s.len() >= 26 && s.len() <= 62) && 
        (s.starts_with('1') || s.starts_with('3') || s.starts_with("bc1"))
    }

    fn looks_like_private_key(&self, s: &str) -> bool {
        // Simple heuristic for private keys (WIF format or hex)
        ((s.len() == 51 || s.len() == 52) && 
         (s.starts_with('5') || s.starts_with('K') || s.starts_with('L'))) ||
        (s.len() == 64 && s.chars().all(|c| c.is_ascii_hexdigit()))
    }
}

impl<S, N> FormatEvent<S, N> for JsonFormatter
where
    S: Subscriber + for<'a> LookupSpan<'a>,
    N: for<'a> FormatFields<'a> + 'static,
{
    fn format_event(
        &self,
        ctx: &tracing_subscriber::fmt::FmtContext<'_, S, N>,
        mut writer: Writer<'_>,
        event: &Event<'_>,
    ) -> fmt::Result {
        let metadata = event.metadata();
        
        // Extract correlation ID from span context
        let correlation_id = ctx
            .lookup_current()
            .and_then(|span| {
                span.extensions().get::<CorrelationId>().map(|id| id.to_string())
            });

        // Collect fields from the event
        let mut fields = HashMap::new();
        let mut visitor = JsonFieldVisitor::new(&mut fields);
        event.record(&mut visitor);

        // Redact sensitive data if enabled
        let mut fields_copy = fields.clone();
        self.redact_fields(&mut fields_copy);

        let entry = StructuredLogEntry {
            timestamp: chrono::Utc::now().to_rfc3339(),
            level: metadata.level().to_string().to_uppercase(),
            component: metadata.target().to_string(),
            message: fields_copy.get("message")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string(),
            correlation_id,
            fields: fields_copy,
            location: Some(LogLocation {
                file: metadata.file().unwrap_or("unknown").to_string(),
                line: metadata.line().unwrap_or(0),
                module: metadata.module_path().unwrap_or("unknown").to_string(),
            }),
        };

        let json = serde_json::to_string(&entry).map_err(|_| fmt::Error)?;
        writeln!(writer, "{}", json)
    }
}

/// Visitor for collecting event fields into a HashMap
struct JsonFieldVisitor<'a> {
    fields: &'a mut HashMap<String, serde_json::Value>,
}

impl<'a> JsonFieldVisitor<'a> {
    fn new(fields: &'a mut HashMap<String, serde_json::Value>) -> Self {
        Self { fields }
    }
}

impl<'a> tracing::field::Visit for JsonFieldVisitor<'a> {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::String(format!("{:?}", value)),
        );
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::String(value.to_string()),
        );
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Number(serde_json::Number::from(value)),
        );
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Number(serde_json::Number::from(value)),
        );
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields.insert(
            field.name().to_string(),
            serde_json::Value::Bool(value),
        );
    }
}

/// Initialize the logging system with the given configuration
pub fn init_logging(config: &LoggingConfig) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Build the environment filter
    let mut filter = EnvFilter::new(&config.level);
    
    // Add component-specific levels
    for (component, level) in &config.component_levels {
        filter = filter.add_directive(format!("{}={}", component, level).parse()?);
    }

    let registry = tracing_subscriber::registry().with(filter);

    match config.format {
        LogFormat::Json => {
            let formatter = JsonFormatter::new(config.redact_sensitive_data);
            let layer = tracing_subscriber::fmt::layer()
                .event_format(formatter);

            match &config.output {
                LogOutput::Stdout => {
                    registry.with(layer).init();
                }
                LogOutput::File(path) => {
                    // TODO: Implement file rotation
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)?;
                    registry.with(layer.with_writer(file)).init();
                }
                LogOutput::Both(path) => {
                    // TODO: Implement dual output (stdout + file)
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)?;
                    registry.with(layer.with_writer(file)).init();
                }
            }
        }
        LogFormat::Pretty => {
            let layer = tracing_subscriber::fmt::layer()
                .pretty();

            match &config.output {
                LogOutput::Stdout => {
                    registry.with(layer).init();
                }
                LogOutput::File(path) => {
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)?;
                    registry.with(layer.with_writer(file)).init();
                }
                LogOutput::Both(path) => {
                    // TODO: Implement dual output (stdout + file)
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)?;
                    registry.with(layer.with_writer(file)).init();
                }
            }
        }
        LogFormat::Compact => {
            let layer = tracing_subscriber::fmt::layer()
                .compact();

            match &config.output {
                LogOutput::Stdout => {
                    registry.with(layer).init();
                }
                LogOutput::File(path) => {
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)?;
                    registry.with(layer.with_writer(file)).init();
                }
                LogOutput::Both(path) => {
                    // TODO: Implement dual output (stdout + file)
                    let file = std::fs::OpenOptions::new()
                        .create(true)
                        .append(true)
                        .open(path)?;
                    registry.with(layer.with_writer(file)).init();
                }
            }
        }
    }

    Ok(())
}

/// Macro for creating a span with correlation ID
#[macro_export]
macro_rules! span_with_correlation {
    ($level:expr, $name:expr) => {{
        let correlation_id = $crate::logging::CorrelationId::new();
        let span = tracing::span!($level, $name, correlation_id = %correlation_id);
        span.record("correlation_id", &tracing::field::display(&correlation_id));
        span
    }};
    ($level:expr, $name:expr, $($field:tt)*) => {{
        let correlation_id = $crate::logging::CorrelationId::new();
        let span = tracing::span!($level, $name, correlation_id = %correlation_id, $($field)*);
        span.record("correlation_id", &tracing::field::display(&correlation_id));
        span
    }};
}

/// Macro for logging with automatic sensitive data redaction
#[macro_export]
macro_rules! log_with_redaction {
    ($level:expr, $($arg:tt)*) => {
        tracing::event!($level, $($arg)*);
    };
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod unit_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_correlation_id_generation() {
        let id1 = CorrelationId::new();
        let id2 = CorrelationId::new();
        
        assert_ne!(id1.to_string(), id2.to_string());
        assert_eq!(id1.to_string().len(), 36); // UUID v4 length
    }

    #[test]
    fn test_sensitive_data_detection() {
        let formatter = JsonFormatter::new(true);
        
        // Test Bitcoin address detection
        assert!(formatter.looks_like_bitcoin_address("1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa"));
        assert!(formatter.looks_like_bitcoin_address("3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy"));
        assert!(formatter.looks_like_bitcoin_address("bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4"));
        assert!(!formatter.looks_like_bitcoin_address("not_an_address"));
        
        // Test private key detection
        assert!(formatter.looks_like_private_key("5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ"));
        assert!(formatter.looks_like_private_key("a0b1c2d3e4f5061728394a5b6c7d8e9f0a1b2c3d4e5f607182938495061728ab"));
        assert!(!formatter.looks_like_private_key("not_a_private_key"));
    }

    #[test]
    fn test_field_redaction() {
        let formatter = JsonFormatter::new(true);
        let mut fields = HashMap::new();
        
        fields.insert("password".to_string(), serde_json::Value::String("secret123".to_string()));
        fields.insert("username".to_string(), serde_json::Value::String("user123".to_string()));
        fields.insert("private_key".to_string(), serde_json::Value::String("5HueCGU8rMjxEXxiPuD5BDku4MkFqeZyd4dZ1jvhTVqvbTLvyTJ".to_string()));
        
        let mut fields_copy = fields.clone();
        formatter.redact_fields(&mut fields_copy);
        
        assert_eq!(fields_copy.get("password").unwrap(), &serde_json::Value::String("[REDACTED]".to_string()));
        assert_eq!(fields_copy.get("username").unwrap(), &serde_json::Value::String("user123".to_string()));
        assert_eq!(fields_copy.get("private_key").unwrap(), &serde_json::Value::String("[REDACTED]".to_string()));
    }
}