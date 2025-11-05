pub mod config;
pub mod error;
pub mod types;
pub mod protocol;
pub mod modes;
pub mod mode;
pub mod auth;
pub mod bitcoin_rpc;
pub mod database;
pub mod server;
pub mod share_validator;
pub mod validation;
pub mod health;
pub mod metrics;
pub mod logging;
pub mod recovery;
pub mod mode_factory;
pub mod connection_auth;
pub mod daemon;
pub mod api_server;

pub use error::{Error, Result};
pub use config::DaemonConfig;
pub use types::{
    Connection, ConnectionId, ConnectionInfo, ConnectionState,
    Share, ShareResult, WorkTemplate,
    MiningStats, PerformanceMetrics, PoolStats,
    Worker, Job, ShareSubmission, Protocol,
    Alert, AlertSeverity, AlertLevel,
    DaemonStatus, UpstreamStatus, BlockTemplate,
};
pub use database::{DatabasePool, DatabaseOps, ShareStats, ConfigHistoryEntry};