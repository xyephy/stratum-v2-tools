use crate::{Result, Connection, Share, ShareResult, WorkTemplate};
use async_trait::async_trait;

/// Mode-specific handler interface
#[async_trait]
pub trait ModeHandler: Send + Sync {
    /// Start the mode handler
    async fn start(&self) -> Result<()>;

    /// Stop the mode handler
    async fn stop(&self) -> Result<()>;

    /// Handle a new connection
    async fn handle_connection(&self, conn: Connection) -> Result<()>;

    /// Process a submitted share
    async fn process_share(&self, share: Share) -> Result<ShareResult>;

    /// Get work template for miners
    async fn get_work_template(&self) -> Result<WorkTemplate>;

    /// Handle connection disconnection
    async fn handle_disconnection(&self, connection_id: crate::ConnectionId) -> Result<()>;

    /// Get mode-specific statistics
    async fn get_statistics(&self) -> Result<crate::MiningStats>;

    /// Validate mode-specific configuration
    fn validate_config(&self, config: &crate::config::DaemonConfig) -> Result<()>;
}

/// Available operational modes
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum OperationMode {
    Solo,
    Pool,
    Proxy,
    Client,
}

impl std::fmt::Display for OperationMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationMode::Solo => write!(f, "solo"),
            OperationMode::Pool => write!(f, "pool"),
            OperationMode::Proxy => write!(f, "proxy"),
            OperationMode::Client => write!(f, "client"),
        }
    }
}

impl std::str::FromStr for OperationMode {
    type Err = crate::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "solo" => Ok(OperationMode::Solo),
            "pool" => Ok(OperationMode::Pool),
            "proxy" => Ok(OperationMode::Proxy),
            "client" => Ok(OperationMode::Client),
            _ => Err(crate::Error::Config(format!("Invalid operation mode: {}", s))),
        }
    }
}