pub mod auth;
pub mod auth_integration_tests;
pub mod connection_auth;
pub mod validation;
pub mod bitcoin_rpc;
pub mod config;
pub mod daemon;
pub mod database;
pub mod error;
pub mod health;
pub mod logging;
pub mod metrics;
pub mod mode;
pub mod mode_factory;
pub mod modes;
pub mod protocol;
pub mod recovery;
pub mod server;
pub mod api_server;
pub mod share_validator;
pub mod types;

#[cfg(test)]
pub mod server_integration_tests;

#[cfg(test)]
pub mod api_server_integration_tests;

#[cfg(test)]
pub mod protocol_integration_tests;

#[cfg(test)]
pub mod bitcoin_rpc_integration_tests;

pub use auth::{AuthSystem, AuthConfig, AuthResult, AuthzResult, Permission, ApiKeyInfo, SessionInfo, HasPermission};
pub use connection_auth::{ConnectionAuthManager, ConnectionMetadata, ConnectionAuthResult, ConnectionAuth, ConnectionStats};
pub use validation::{InputValidator, ValidationConfig, ValidationResult, ValidationError, ValidationErrorType, SanitizationOptions, RateLimiter, RateLimitConfig};
pub use bitcoin_rpc::BitcoinRpcClient;
pub use error::{Error, Result};
pub use daemon::Daemon;
pub use database::{DatabasePool, DatabaseOps, ShareStats, ConfigHistoryEntry, DatabaseStats, RecoveryDatabasePool};
pub use health::{HealthMonitor, ExtendedHealthConfig, HealthStatus, HealthCheck, AlertSeverity, HealthService};
pub use logging::{CorrelationId, init_logging};
pub use metrics::{MetricsCollector, MetricsConfig, MetricsService, MetricsSummary};
pub use mode::{ModeHandler, OperationMode};
pub use modes::{SoloModeHandler, PoolModeHandler};
pub use mode_factory::{ModeHandlerFactory, ModeRouter, ModeState};
pub use protocol::{ProtocolHandler, ProtocolMessage};
pub use recovery::{RecoveryConfig, RetryExecutor, RetryStrategy, CircuitBreaker, CircuitBreakerState, GracefulDegradation, DatabaseRecovery};
pub use config::DaemonConfig;
pub use share_validator::{ShareValidator, ShareValidatorConfig, ShareValidatorStats, ShareValidationError};
pub use types::*;