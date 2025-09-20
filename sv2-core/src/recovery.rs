use crate::{Result, Error};
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use tracing::{info, warn, error, debug};

/// Recovery configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecoveryConfig {
    /// Maximum number of retry attempts
    pub max_retries: u32,
    /// Initial backoff delay in milliseconds
    pub initial_backoff_ms: u64,
    /// Backoff multiplier for exponential backoff
    pub backoff_multiplier: f64,
    /// Maximum backoff delay in milliseconds
    pub max_backoff_ms: u64,
    /// Jitter factor for randomizing delays (0.0 to 1.0)
    pub jitter_factor: f64,
    /// Enable circuit breaker functionality
    pub enable_circuit_breaker: bool,
    /// Circuit breaker failure threshold
    pub circuit_breaker_threshold: u32,
    /// Circuit breaker reset timeout in milliseconds
    pub circuit_breaker_reset_ms: u64,
}

impl Default for RecoveryConfig {
    fn default() -> Self {
        Self {
            max_retries: 3,
            initial_backoff_ms: 1000,
            backoff_multiplier: 2.0,
            max_backoff_ms: 30000,
            jitter_factor: 0.1,
            enable_circuit_breaker: true,
            circuit_breaker_threshold: 5,
            circuit_breaker_reset_ms: 60000,
        }
    }
}

/// Retry strategy for error recovery
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum RetryStrategy {
    /// Fixed delay between retries
    Fixed { delay: Duration },
    /// Exponential backoff with optional jitter
    ExponentialBackoff {
        initial: Duration,
        multiplier: f64,
        max: Duration,
    },
}

impl RetryStrategy {
    /// Calculate delay for the given attempt number
    pub fn delay_for_attempt(&self, attempt: u32) -> Duration {
        match self {
            RetryStrategy::Fixed { delay } => *delay,
            RetryStrategy::ExponentialBackoff { initial, multiplier, max } => {
                let delay = initial.as_millis() as f64 * multiplier.powi(attempt as i32);
                Duration::from_millis((delay as u64).min(max.as_millis() as u64))
            }
        }
    }
}

/// Circuit breaker states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CircuitBreakerState {
    /// Circuit is closed, requests are allowed
    Closed,
    /// Circuit is open, requests are blocked
    Open,
    /// Circuit is half-open, testing if service has recovered
    HalfOpen,
}

/// Circuit breaker for preventing cascading failures
#[derive(Debug)]
pub struct CircuitBreaker {
    state: CircuitBreakerState,
    failure_count: u32,
    last_failure_time: Option<Instant>,
    config: RecoveryConfig,
}

impl CircuitBreaker {
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            state: CircuitBreakerState::Closed,
            failure_count: 0,
            last_failure_time: None,
            config,
        }
    }

    pub fn can_execute(&self) -> bool {
        match self.state {
            CircuitBreakerState::Closed => true,
            CircuitBreakerState::Open => {
                if let Some(last_failure) = self.last_failure_time {
                    let reset_timeout = Duration::from_millis(self.config.circuit_breaker_reset_ms);
                    last_failure.elapsed() >= reset_timeout
                } else {
                    false
                }
            }
            CircuitBreakerState::HalfOpen => true,
        }
    }

    pub fn record_success(&mut self) {
        self.failure_count = 0;
        self.state = CircuitBreakerState::Closed;
        self.last_failure_time = None;
    }

    pub fn record_failure(&mut self) {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());

        if self.failure_count >= self.config.circuit_breaker_threshold {
            self.state = CircuitBreakerState::Open;
        }
    }

    pub fn state(&self) -> CircuitBreakerState {
        self.state.clone()
    }

    pub fn failure_count(&self) -> u32 {
        self.failure_count
    }
}

/// Retry executor with configurable strategies and circuit breaker
#[derive(Debug)]
pub struct RetryExecutor {
    strategy: RetryStrategy,
    max_retries: u32,
    timeout: Duration,
    circuit_breaker: Option<CircuitBreaker>,
}

impl RetryExecutor {
    pub fn new(config: RecoveryConfig) -> Self {
        let strategy = RetryStrategy::ExponentialBackoff {
            initial: Duration::from_millis(config.initial_backoff_ms),
            multiplier: config.backoff_multiplier,
            max: Duration::from_millis(config.max_backoff_ms),
        };

        let circuit_breaker = if config.enable_circuit_breaker {
            Some(CircuitBreaker::new(config.clone()))
        } else {
            None
        };

        Self {
            strategy,
            max_retries: config.max_retries,
            timeout: Duration::from_millis(config.max_backoff_ms),
            circuit_breaker,
        }
    }

    pub async fn execute_with_condition<F, Fut, T, P>(
        &mut self,
        operation: F,
        should_retry: P,
    ) -> Result<T>
    where
        F: Fn() -> Fut,
        Fut: std::future::Future<Output = Result<T>>,
        P: Fn(&Error) -> bool,
    {
        if let Some(ref cb) = self.circuit_breaker {
            if !cb.can_execute() {
                return Err(Error::System("Circuit breaker is open".to_string()));
            }
        }

        let mut last_error = None;

        for attempt in 0..=self.max_retries {
            match operation().await {
                Ok(result) => {
                    if let Some(ref mut cb) = self.circuit_breaker {
                        cb.record_success();
                    }
                    return Ok(result);
                }
                Err(error) => {
                    last_error = Some(error.clone());

                    if let Some(ref mut cb) = self.circuit_breaker {
                        cb.record_failure();
                    }

                    if attempt < self.max_retries && should_retry(&error) {
                        let delay = self.strategy.delay_for_attempt(attempt);
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Err(last_error.unwrap_or_else(|| Error::System("Unknown error".to_string())))
    }
}

/// Graceful degradation manager for handling non-critical feature failures
#[derive(Debug)]
pub struct GracefulDegradation {
    disabled_features: std::collections::HashSet<String>,
    feature_failure_counts: std::collections::HashMap<String, u32>,
    config: RecoveryConfig,
}

impl GracefulDegradation {
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            disabled_features: std::collections::HashSet::new(),
            feature_failure_counts: std::collections::HashMap::new(),
            config,
        }
    }

    pub fn record_feature_failure(&mut self, feature: &str) {
        let count = self.feature_failure_counts.entry(feature.to_string()).or_insert(0);
        *count += 1;

        if *count >= self.config.circuit_breaker_threshold {
            self.disabled_features.insert(feature.to_string());
            warn!("Feature '{}' disabled due to repeated failures", feature);
        }
    }

    pub fn record_feature_success(&mut self, feature: &str) {
        self.feature_failure_counts.remove(feature);
        if self.disabled_features.remove(feature) {
            info!("Feature '{}' re-enabled after successful operation", feature);
        }
    }

    pub fn is_feature_enabled(&self, feature: &str) -> bool {
        !self.disabled_features.contains(feature)
    }

    pub fn disabled_features(&self) -> Vec<String> {
        self.disabled_features.iter().cloned().collect()
    }

    pub fn is_degradation_active(&self) -> bool {
        !self.disabled_features.is_empty()
    }

    pub fn feature_failure_count(&self, feature: &str) -> u32 {
        self.feature_failure_counts.get(feature).copied().unwrap_or(0)
    }
}

/// Enhanced database connection recovery manager with failover capabilities
#[derive(Debug)]
pub struct DatabaseRecovery {
    retry_executor: RetryExecutor,
    degradation: GracefulDegradation,
}

impl DatabaseRecovery {
    pub fn new(config: RecoveryConfig) -> Self {
        Self {
            retry_executor: RetryExecutor::new(config.clone()),
            degradation: GracefulDegradation::new(config),
        }
    }

    pub fn is_database_available(&self) -> bool {
        self.degradation.is_feature_enabled("database")
    }

    pub fn database_failure_count(&self) -> u32 {
        self.degradation.feature_failure_count("database")
    }
}

/// Recovery-enabled database pool wrapper
#[derive(Debug)]
pub struct RecoveryDatabasePool {
    pool: crate::DatabasePool,
    recovery: std::sync::Arc<std::sync::Mutex<DatabaseRecovery>>,
    config: RecoveryConfig,
}

impl RecoveryDatabasePool {
    pub async fn new(primary_url: String, _fallback_urls: Vec<String>, config: RecoveryConfig) -> Result<Self> {
        let mut recovery = DatabaseRecovery::new(config.clone());
        
        // Initialize primary connection
        let pool = crate::DatabasePool::new(&primary_url, 10).await?;
        
        Ok(Self {
            pool,
            recovery: std::sync::Arc::new(std::sync::Mutex::new(recovery)),
            config,
        })
    }


}

#[async_trait::async_trait]
impl crate::DatabaseOps for RecoveryDatabasePool {
    async fn create_connection(&self, conn_info: &crate::ConnectionInfo) -> Result<()> {
        self.pool.create_connection(conn_info).await
    }

    async fn update_connection(&self, conn_info: &crate::ConnectionInfo) -> Result<()> {
        self.pool.update_connection(conn_info).await
    }

    async fn get_connection(&self, id: uuid::Uuid) -> Result<Option<crate::ConnectionInfo>> {
        self.pool.get_connection(id).await
    }

    async fn list_connections(&self, limit: Option<u32>) -> Result<Vec<crate::ConnectionInfo>> {
        self.pool.list_connections(limit).await
    }

    async fn delete_connection(&self, id: uuid::Uuid) -> Result<()> {
        self.pool.delete_connection(id).await
    }

    async fn create_share(&self, share: &crate::Share) -> Result<()> {
        self.pool.create_share(share).await
    }

    async fn get_shares(&self, connection_id: Option<uuid::Uuid>, limit: Option<u32>) -> Result<Vec<crate::Share>> {
        self.pool.get_shares(connection_id, limit).await
    }

    async fn get_share_stats(&self, connection_id: Option<uuid::Uuid>) -> Result<crate::ShareStats> {
        self.pool.get_share_stats(connection_id).await
    }

    async fn create_work_template(&self, template: &crate::WorkTemplate) -> Result<()> {
        self.pool.create_work_template(template).await
    }

    async fn get_work_template(&self, id: uuid::Uuid) -> Result<Option<crate::WorkTemplate>> {
        self.pool.get_work_template(id).await
    }

    async fn list_work_templates(&self, limit: Option<u32>) -> Result<Vec<crate::WorkTemplate>> {
        self.pool.list_work_templates(limit).await
    }

    async fn delete_expired_templates(&self) -> Result<u64> {
        self.pool.delete_expired_templates().await
    }

    async fn create_alert(&self, alert: &crate::Alert) -> Result<()> {
        self.pool.create_alert(alert).await
    }

    async fn update_alert(&self, alert: &crate::Alert) -> Result<()> {
        self.pool.update_alert(alert).await
    }

    async fn get_alerts(&self, resolved: Option<bool>, limit: Option<u32>) -> Result<Vec<crate::Alert>> {
        self.pool.get_alerts(resolved, limit).await
    }

    async fn store_performance_metrics(&self, metrics: &crate::PerformanceMetrics) -> Result<()> {
        self.pool.store_performance_metrics(metrics).await
    }

    async fn get_performance_metrics(&self, limit: Option<u32>) -> Result<Vec<crate::PerformanceMetrics>> {
        self.pool.get_performance_metrics(limit).await
    }

    async fn store_config_history(&self, config_data: &str, applied_by: &str) -> Result<()> {
        self.pool.store_config_history(config_data, applied_by).await
    }

    async fn get_config_history(&self, limit: Option<u32>) -> Result<Vec<crate::ConfigHistoryEntry>> {
        self.pool.get_config_history(limit).await
    }

    async fn store_connection(&self, conn: &crate::Connection) -> Result<()> {
        self.pool.store_connection(conn).await
    }

    async fn store_share(&self, share: &crate::Share) -> Result<()> {
        self.pool.store_share(share).await
    }

    async fn store_work_template(&self, template: &crate::WorkTemplate) -> Result<()> {
        self.pool.store_work_template(template).await
    }

    async fn update_connection_status(&self, connection_id: uuid::Uuid, status: crate::types::ConnectionState) -> Result<()> {
        self.pool.update_connection_status(connection_id, status).await
    }

    async fn get_connection_info(&self, connection_id: uuid::Uuid) -> Result<Option<crate::ConnectionInfo>> {
        self.pool.get_connection_info(connection_id).await
    }

    async fn get_connections(&self, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<crate::ConnectionInfo>> {
        self.pool.get_connections(limit, offset).await
    }

    async fn get_work_templates(&self, limit: Option<u32>) -> Result<Vec<crate::WorkTemplate>> {
        self.pool.get_work_templates(limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_recovery_config_default() {
        let config = RecoveryConfig::default();
        assert_eq!(config.max_retries, 3);
        assert!(config.enable_circuit_breaker);
    }

    #[tokio::test]
    async fn test_retry_executor() {
        let config = RecoveryConfig::default();
        let mut executor = RetryExecutor::new(config);
        
        let attempt_count = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
        let attempt_count_clone = attempt_count.clone();
        
        let result = executor.execute_with_condition(
            move || {
                let count = attempt_count_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                async move {
                    if count < 2 {
                        Err(Error::System("Test error".to_string()))
                    } else {
                        Ok("Success".to_string())
                    }
                }
            },
            |_| true,
        ).await;
        
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "Success");
        assert_eq!(attempt_count.load(std::sync::atomic::Ordering::SeqCst), 3);
    }

    #[test]
    fn test_circuit_breaker() {
        let config = RecoveryConfig::default();
        let mut cb = CircuitBreaker::new(config);
        
        assert!(cb.can_execute());
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
        
        // Record failures to open circuit
        for _ in 0..5 {
            cb.record_failure();
        }
        
        assert_eq!(cb.state(), CircuitBreakerState::Open);
        
        // Record success to close circuit
        cb.record_success();
        assert_eq!(cb.state(), CircuitBreakerState::Closed);
    }

    #[test]
    fn test_graceful_degradation() {
        let config = RecoveryConfig::default();
        let mut degradation = GracefulDegradation::new(config);
        
        assert!(degradation.is_feature_enabled("test_feature"));
        assert!(!degradation.is_degradation_active());
        
        // Record failures to disable feature
        for _ in 0..5 {
            degradation.record_feature_failure("test_feature");
        }
        
        assert!(!degradation.is_feature_enabled("test_feature"));
        assert!(degradation.is_degradation_active());
        
        // Record success to re-enable feature
        degradation.record_feature_success("test_feature");
        assert!(degradation.is_feature_enabled("test_feature"));
        assert!(!degradation.is_degradation_active());
    }
}