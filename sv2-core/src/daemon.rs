use crate::{Result, DaemonStatus};
use crate::config::DaemonConfig;
use async_trait::async_trait;

/// Core daemon interface for sv2d
#[async_trait]
pub trait Daemon: Send + Sync {
    /// Start the daemon with the given configuration
    async fn start(&mut self, config: DaemonConfig) -> Result<()>;

    /// Stop the daemon gracefully
    async fn stop(&mut self) -> Result<()>;

    /// Reload configuration without restarting
    async fn reload_config(&mut self, config: DaemonConfig) -> Result<()>;

    /// Get current daemon status
    fn get_status(&self) -> DaemonStatus;

    /// Check if daemon is running
    fn is_running(&self) -> bool;

    /// Get daemon uptime
    fn uptime(&self) -> std::time::Duration;
}