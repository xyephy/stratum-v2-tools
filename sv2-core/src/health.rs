use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::RwLock;
use serde::{Deserialize, Serialize};
use crate::error::{Error, Result};
use crate::database::DatabaseOps;
use crate::config::{HealthConfig, AlertThresholds};
use crate::types::{Alert as DbAlert, AlertLevel};

/// Health check status
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

/// Health check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthCheck {
    pub name: String,
    pub status: HealthStatus,
    pub message: String,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub duration: Duration,
    pub metadata: HashMap<String, String>,
}

/// Extended health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExtendedHealthConfig {
    /// Base health configuration
    pub base: HealthConfig,
    /// Notification channels
    pub notification_channels: Vec<NotificationChannel>,
}

/// Notification channel configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NotificationChannel {
    pub name: String,
    pub channel_type: NotificationChannelType,
    pub config: HashMap<String, String>,
    pub enabled: bool,
}

/// Types of notification channels
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum NotificationChannelType {
    Log,
    Email,
    Webhook,
    Slack,
    Discord,
}

/// Alert severity levels
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Critical,
}

/// Alert message
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: String,
    pub title: String,
    pub message: String,
    pub severity: AlertSeverity,
    pub timestamp: chrono::DateTime<chrono::Utc>,
    pub source: String,
    pub metadata: HashMap<String, String>,
}

/// Health monitoring service
pub struct HealthMonitor {
    config: ExtendedHealthConfig,
    checks: Vec<Box<dyn HealthChecker + Send + Sync>>,
    last_results: Arc<RwLock<HashMap<String, HealthCheck>>>,
    alert_history: Arc<RwLock<Vec<Alert>>>,
    notification_service: NotificationService,
}

/// Trait for health checkers
#[async_trait::async_trait]
pub trait HealthChecker {
    /// Get the name of this health checker
    fn name(&self) -> &str;
    
    /// Perform the health check
    async fn check(&self) -> Result<HealthCheck>;
    
    /// Get the check interval (if different from global)
    fn interval(&self) -> Option<Duration> {
        None
    }
}

/// Database health checker
pub struct DatabaseHealthChecker {
    database: Arc<dyn DatabaseOps>,
    name: String,
}

/// Connection health checker
pub struct ConnectionHealthChecker {
    name: String,
    connection_count_fn: Arc<dyn Fn() -> u32 + Send + Sync>,
    max_connections: u32,
}

/// System resource health checker
pub struct SystemHealthChecker {
    name: String,
    thresholds: AlertThresholds,
}

/// Bitcoin RPC health checker
pub struct BitcoinRpcHealthChecker {
    name: String,
    rpc_client: Arc<crate::bitcoin_rpc::BitcoinRpcClient>,
}

/// Notification service for sending alerts
pub struct NotificationService {
    channels: Vec<NotificationChannel>,
}

impl Default for ExtendedHealthConfig {
    fn default() -> Self {
        Self {
            base: HealthConfig::default(),
            notification_channels: vec![
                NotificationChannel {
                    name: "log".to_string(),
                    channel_type: NotificationChannelType::Log,
                    config: HashMap::new(),
                    enabled: true,
                }
            ],
        }
    }
}

impl Alert {
    /// Convert health Alert to database Alert
    pub fn to_db_alert(&self) -> DbAlert {
        DbAlert {
            id: uuid::Uuid::parse_str(&self.id).unwrap_or_else(|_| uuid::Uuid::new_v4()),
            level: match self.severity {
                AlertSeverity::Info => AlertLevel::Info,
                AlertSeverity::Warning => AlertLevel::Warning,
                AlertSeverity::Critical => AlertLevel::Critical,
            },
            title: self.title.clone(),
            message: self.message.clone(),
            component: self.source.clone(),
            created_at: self.timestamp,
            resolved_at: None,
            metadata: self.metadata.clone(),
        }
    }
}

impl HealthMonitor {
    /// Create a new health monitor
    pub fn new(config: ExtendedHealthConfig) -> Self {
        let notification_service = NotificationService::new(config.notification_channels.clone());
        
        Self {
            config,
            checks: Vec::new(),
            last_results: Arc::new(RwLock::new(HashMap::new())),
            alert_history: Arc::new(RwLock::new(Vec::new())),
            notification_service,
        }
    }

    /// Add a health checker
    pub fn add_checker(&mut self, checker: Box<dyn HealthChecker + Send + Sync>) {
        self.checks.push(checker);
    }

    /// Start the health monitoring service
    pub async fn start(&self) -> Result<()> {
        let mut interval = tokio::time::interval(Duration::from_secs(self.config.base.check_interval));
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.run_health_checks().await {
                tracing::error!("Failed to run health checks: {}", e);
            }
        }
    }

    /// Run all health checks
    pub async fn run_health_checks(&self) -> Result<()> {
        let mut results = HashMap::new();
        
        for checker in &self.checks {
            let start_time = Instant::now();
            
            let result = match tokio::time::timeout(
                Duration::from_secs(self.config.base.check_timeout),
                checker.check()
            ).await {
                Ok(Ok(check)) => check,
                Ok(Err(e)) => HealthCheck {
                    name: checker.name().to_string(),
                    status: HealthStatus::Critical,
                    message: format!("Health check failed: {}", e),
                    timestamp: chrono::Utc::now(),
                    duration: start_time.elapsed(),
                    metadata: HashMap::new(),
                },
                Err(_) => HealthCheck {
                    name: checker.name().to_string(),
                    status: HealthStatus::Critical,
                    message: "Health check timed out".to_string(),
                    timestamp: chrono::Utc::now(),
                    duration: start_time.elapsed(),
                    metadata: HashMap::new(),
                },
            };
            
            // Check if we need to generate alerts
            self.check_for_alerts(&result).await?;
            
            results.insert(result.name.clone(), result);
        }
        
        // Update last results
        let mut last_results = self.last_results.write().await;
        *last_results = results;
        
        Ok(())
    }

    /// Check if an alert should be generated based on health check result
    async fn check_for_alerts(&self, health_check: &HealthCheck) -> Result<()> {
        let should_alert = match health_check.status {
            HealthStatus::Critical => true,
            HealthStatus::Warning => true,
            HealthStatus::Healthy => false,
            HealthStatus::Unknown => false,
        };

        if should_alert {
            let alert = Alert {
                id: uuid::Uuid::new_v4().to_string(),
                title: format!("Health Check Alert: {}", health_check.name),
                message: health_check.message.clone(),
                severity: match health_check.status {
                    HealthStatus::Critical => AlertSeverity::Critical,
                    HealthStatus::Warning => AlertSeverity::Warning,
                    _ => AlertSeverity::Info,
                },
                timestamp: chrono::Utc::now(),
                source: health_check.name.clone(),
                metadata: health_check.metadata.clone(),
            };

            self.send_alert(alert).await?;
        }

        Ok(())
    }

    /// Send an alert through configured notification channels
    async fn send_alert(&self, alert: Alert) -> Result<()> {
        // Add to alert history
        let mut history = self.alert_history.write().await;
        history.push(alert.clone());
        
        // Keep only last 1000 alerts
        if history.len() > 1000 {
            let excess = history.len() - 1000;
            history.drain(0..excess);
        }
        drop(history);

        // Send through notification channels
        self.notification_service.send_alert(&alert).await?;
        
        // Store in database if we have database access
        // Note: This would require access to the database, which we don't have in this context
        // In a real implementation, you would inject the database dependency

        Ok(())
    }

    /// Get current health status
    pub async fn get_health_status(&self) -> HashMap<String, HealthCheck> {
        self.last_results.read().await.clone()
    }

    /// Get overall health status
    pub async fn get_overall_status(&self) -> HealthStatus {
        let results = self.last_results.read().await;
        
        if results.is_empty() {
            return HealthStatus::Unknown;
        }

        let mut has_critical = false;
        let mut has_warning = false;

        for check in results.values() {
            match check.status {
                HealthStatus::Critical => has_critical = true,
                HealthStatus::Warning => has_warning = true,
                _ => {}
            }
        }

        if has_critical {
            HealthStatus::Critical
        } else if has_warning {
            HealthStatus::Warning
        } else {
            HealthStatus::Healthy
        }
    }

    /// Get alert history
    pub async fn get_alert_history(&self, limit: Option<usize>) -> Vec<Alert> {
        let history = self.alert_history.read().await;
        let limit = limit.unwrap_or(100);
        
        if history.len() <= limit {
            history.clone()
        } else {
            history[history.len() - limit..].to_vec()
        }
    }
}

impl DatabaseHealthChecker {
    pub fn new(database: Arc<dyn DatabaseOps>, name: String) -> Self {
        Self { database, name }
    }
}

#[async_trait::async_trait]
impl HealthChecker for DatabaseHealthChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> Result<HealthCheck> {
        let start_time = Instant::now();
        
        match self.database.get_share_stats(None).await {
            Ok(stats) => {
                let mut metadata = HashMap::new();
                metadata.insert("total_shares".to_string(), stats.total_shares.to_string());
                metadata.insert("valid_shares".to_string(), stats.valid_shares.to_string());
                metadata.insert("invalid_shares".to_string(), stats.invalid_shares.to_string());
                
                let status = HealthStatus::Healthy;

                Ok(HealthCheck {
                    name: self.name.clone(),
                    status,
                    message: format!("Database operational with {} total shares", stats.total_shares),
                    timestamp: chrono::Utc::now(),
                    duration: start_time.elapsed(),
                    metadata,
                })
            }
            Err(e) => Ok(HealthCheck {
                name: self.name.clone(),
                status: HealthStatus::Critical,
                message: format!("Database connection failed: {}", e),
                timestamp: chrono::Utc::now(),
                duration: start_time.elapsed(),
                metadata: HashMap::new(),
            })
        }
    }
}

impl ConnectionHealthChecker {
    pub fn new(
        name: String,
        connection_count_fn: Arc<dyn Fn() -> u32 + Send + Sync>,
        max_connections: u32,
    ) -> Self {
        Self {
            name,
            connection_count_fn,
            max_connections,
        }
    }
}

#[async_trait::async_trait]
impl HealthChecker for ConnectionHealthChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> Result<HealthCheck> {
        let start_time = Instant::now();
        let connection_count = (self.connection_count_fn)();
        
        let mut metadata = HashMap::new();
        metadata.insert("active_connections".to_string(), connection_count.to_string());
        metadata.insert("max_connections".to_string(), self.max_connections.to_string());
        
        let usage_percentage = (connection_count as f64 / self.max_connections as f64) * 100.0;
        metadata.insert("usage_percentage".to_string(), format!("{:.1}", usage_percentage));

        let (status, message) = if usage_percentage >= 90.0 {
            (HealthStatus::Critical, format!("Connection usage critical: {:.1}%", usage_percentage))
        } else if usage_percentage >= 75.0 {
            (HealthStatus::Warning, format!("Connection usage high: {:.1}%", usage_percentage))
        } else {
            (HealthStatus::Healthy, format!("Connection usage normal: {:.1}%", usage_percentage))
        };

        Ok(HealthCheck {
            name: self.name.clone(),
            status,
            message,
            timestamp: chrono::Utc::now(),
            duration: start_time.elapsed(),
            metadata,
        })
    }
}

impl SystemHealthChecker {
    pub fn new(name: String, thresholds: AlertThresholds) -> Self {
        Self { name, thresholds }
    }
}

#[async_trait::async_trait]
impl HealthChecker for SystemHealthChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> Result<HealthCheck> {
        let start_time = Instant::now();
        
        // TODO: Implement actual system monitoring
        // For now, we'll use placeholder values
        // In a real implementation, you would use system monitoring libraries
        // like sysinfo to get actual system metrics
        
        let cpu_usage = 25.0; // Placeholder
        let memory_usage = 45.0; // Placeholder
        
        let mut metadata = HashMap::new();
        metadata.insert("cpu_usage".to_string(), format!("{:.1}", cpu_usage));
        metadata.insert("memory_usage".to_string(), format!("{:.1}", memory_usage));
        
        let (status, message) = if cpu_usage >= self.thresholds.cpu_usage || memory_usage >= self.thresholds.memory_usage {
            if cpu_usage >= 95.0 || memory_usage >= 95.0 {
                (HealthStatus::Critical, "System resources critically high".to_string())
            } else {
                (HealthStatus::Warning, "System resources elevated".to_string())
            }
        } else {
            (HealthStatus::Healthy, "System resources normal".to_string())
        };

        Ok(HealthCheck {
            name: self.name.clone(),
            status,
            message,
            timestamp: chrono::Utc::now(),
            duration: start_time.elapsed(),
            metadata,
        })
    }
}

impl BitcoinRpcHealthChecker {
    pub fn new(name: String, rpc_client: Arc<crate::bitcoin_rpc::BitcoinRpcClient>) -> Self {
        Self { name, rpc_client }
    }
}

#[async_trait::async_trait]
impl HealthChecker for BitcoinRpcHealthChecker {
    fn name(&self) -> &str {
        &self.name
    }

    async fn check(&self) -> Result<HealthCheck> {
        let start_time = Instant::now();
        
        match self.rpc_client.get_blockchain_info().await {
            Ok(info) => {
                let mut metadata = HashMap::new();
                metadata.insert("blocks".to_string(), info.blocks.to_string());
                metadata.insert("chain".to_string(), info.chain.clone());
                
                Ok(HealthCheck {
                    name: self.name.clone(),
                    status: HealthStatus::Healthy,
                    message: format!("Bitcoin RPC operational, {} blocks", info.blocks),
                    timestamp: chrono::Utc::now(),
                    duration: start_time.elapsed(),
                    metadata,
                })
            }
            Err(e) => Ok(HealthCheck {
                name: self.name.clone(),
                status: HealthStatus::Critical,
                message: format!("Bitcoin RPC connection failed: {}", e),
                timestamp: chrono::Utc::now(),
                duration: start_time.elapsed(),
                metadata: HashMap::new(),
            })
        }
    }
}

impl NotificationService {
    pub fn new(channels: Vec<NotificationChannel>) -> Self {
        Self { channels }
    }

    pub async fn send_alert(&self, alert: &Alert) -> Result<()> {
        for channel in &self.channels {
            if !channel.enabled {
                continue;
            }

            if let Err(e) = self.send_to_channel(channel, alert).await {
                tracing::error!("Failed to send alert to channel {}: {}", channel.name, e);
            }
        }
        Ok(())
    }

    async fn send_to_channel(&self, channel: &NotificationChannel, alert: &Alert) -> Result<()> {
        match channel.channel_type {
            NotificationChannelType::Log => {
                match alert.severity {
                    AlertSeverity::Critical => tracing::error!("[ALERT] {}: {}", alert.title, alert.message),
                    AlertSeverity::Warning => tracing::warn!("[ALERT] {}: {}", alert.title, alert.message),
                    AlertSeverity::Info => tracing::info!("[ALERT] {}: {}", alert.title, alert.message),
                }
            }
            NotificationChannelType::Email => {
                // TODO: Implement email notifications
                tracing::info!("Email notification not implemented: {}", alert.title);
            }
            NotificationChannelType::Webhook => {
                // TODO: Implement webhook notifications
                tracing::info!("Webhook notification not implemented: {}", alert.title);
            }
            NotificationChannelType::Slack => {
                // TODO: Implement Slack notifications
                tracing::info!("Slack notification not implemented: {}", alert.title);
            }
            NotificationChannelType::Discord => {
                // TODO: Implement Discord notifications
                tracing::info!("Discord notification not implemented: {}", alert.title);
            }
        }
        Ok(())
    }
}

/// Health monitoring service for background execution
pub struct HealthService {
    monitor: Arc<HealthMonitor>,
}

impl HealthService {
    pub fn new(monitor: Arc<HealthMonitor>) -> Self {
        Self { monitor }
    }

    /// Start the health monitoring service
    pub async fn start(&self) -> Result<()> {
        self.monitor.start().await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    struct MockHealthChecker {
        name: String,
        status: HealthStatus,
        message: String,
    }

    impl MockHealthChecker {
        fn new(name: String, status: HealthStatus, message: String) -> Self {
            Self { name, status, message }
        }
    }

    #[async_trait::async_trait]
    impl HealthChecker for MockHealthChecker {
        fn name(&self) -> &str {
            &self.name
        }

        async fn check(&self) -> Result<HealthCheck> {
            Ok(HealthCheck {
                name: self.name.clone(),
                status: self.status.clone(),
                message: self.message.clone(),
                timestamp: chrono::Utc::now(),
                duration: Duration::from_millis(10),
                metadata: HashMap::new(),
            })
        }
    }

    #[tokio::test]
    async fn test_health_monitor_creation() {
        let config = ExtendedHealthConfig::default();
        let monitor = HealthMonitor::new(config);
        
        let status = monitor.get_overall_status().await;
        assert_eq!(status, HealthStatus::Unknown);
    }

    #[tokio::test]
    async fn test_health_check_execution() {
        let config = ExtendedHealthConfig::default();
        let mut monitor = HealthMonitor::new(config);
        
        let checker = Box::new(MockHealthChecker::new(
            "test".to_string(),
            HealthStatus::Healthy,
            "All good".to_string(),
        ));
        
        monitor.add_checker(checker);
        
        monitor.run_health_checks().await.unwrap();
        
        let results = monitor.get_health_status().await;
        assert_eq!(results.len(), 1);
        assert_eq!(results["test"].status, HealthStatus::Healthy);
    }

    #[tokio::test]
    async fn test_overall_status_calculation() {
        let config = ExtendedHealthConfig::default();
        let mut monitor = HealthMonitor::new(config);
        
        monitor.add_checker(Box::new(MockHealthChecker::new(
            "healthy".to_string(),
            HealthStatus::Healthy,
            "OK".to_string(),
        )));
        
        monitor.add_checker(Box::new(MockHealthChecker::new(
            "warning".to_string(),
            HealthStatus::Warning,
            "Warning".to_string(),
        )));
        
        monitor.run_health_checks().await.unwrap();
        
        let overall_status = monitor.get_overall_status().await;
        assert_eq!(overall_status, HealthStatus::Warning);
    }

    #[tokio::test]
    async fn test_critical_status_priority() {
        let config = ExtendedHealthConfig::default();
        let mut monitor = HealthMonitor::new(config);
        
        monitor.add_checker(Box::new(MockHealthChecker::new(
            "healthy".to_string(),
            HealthStatus::Healthy,
            "OK".to_string(),
        )));
        
        monitor.add_checker(Box::new(MockHealthChecker::new(
            "critical".to_string(),
            HealthStatus::Critical,
            "Critical error".to_string(),
        )));
        
        monitor.run_health_checks().await.unwrap();
        
        let overall_status = monitor.get_overall_status().await;
        assert_eq!(overall_status, HealthStatus::Critical);
    }

    #[tokio::test]
    async fn test_connection_health_checker() {
        let connection_count = Arc::new(AtomicU32::new(50));
        let connection_count_clone = connection_count.clone();
        
        let checker = ConnectionHealthChecker::new(
            "connections".to_string(),
            Arc::new(move || connection_count_clone.load(Ordering::Relaxed)),
            100,
        );
        
        let result = checker.check().await.unwrap();
        assert_eq!(result.status, HealthStatus::Healthy);
        
        // Test warning threshold
        connection_count.store(80, Ordering::Relaxed);
        let result = checker.check().await.unwrap();
        assert_eq!(result.status, HealthStatus::Warning);
        
        // Test critical threshold
        connection_count.store(95, Ordering::Relaxed);
        let result = checker.check().await.unwrap();
        assert_eq!(result.status, HealthStatus::Critical);
    }

    #[tokio::test]
    async fn test_alert_generation() {
        let config = ExtendedHealthConfig::default();
        let mut monitor = HealthMonitor::new(config);
        
        monitor.add_checker(Box::new(MockHealthChecker::new(
            "critical_test".to_string(),
            HealthStatus::Critical,
            "Critical failure".to_string(),
        )));
        
        monitor.run_health_checks().await.unwrap();
        
        let alerts = monitor.get_alert_history(None).await;
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].severity, AlertSeverity::Critical);
        assert_eq!(alerts[0].source, "critical_test");
    }

    #[tokio::test]
    async fn test_alert_history_limit() {
        let config = ExtendedHealthConfig::default();
        let monitor = HealthMonitor::new(config);
        
        // Add many alerts
        for i in 0..150 {
            let alert = Alert {
                id: format!("alert_{}", i),
                title: format!("Test Alert {}", i),
                message: "Test message".to_string(),
                severity: AlertSeverity::Info,
                timestamp: chrono::Utc::now(),
                source: "test".to_string(),
                metadata: HashMap::new(),
            };
            
            monitor.send_alert(alert).await.unwrap();
        }
        
        let alerts = monitor.get_alert_history(Some(50)).await;
        assert_eq!(alerts.len(), 50);
        
        // Check that we get the most recent alerts
        assert!(alerts[0].title.contains("100") || alerts[0].title.contains("149"));
    }
}