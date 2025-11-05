use crate::{Result, Error};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use uuid::Uuid;
use chrono::{DateTime, Utc};
use bitcoin::{BlockHash, Transaction};
use std::time::Duration;

/// Type alias for connection IDs
pub type ConnectionId = Uuid;

/// Mining protocol type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Sv1,
    Sv2,
    StratumV1,
    StratumV2,
}

/// Connection state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Connected,
    Disconnected,
    Connecting,
    Authenticated,
    Error,
}

/// Connection information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub id: Uuid,
    pub address: SocketAddr,
    pub protocol: Protocol,
    pub state: ConnectionState,
    pub connected_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
    pub subscribed_difficulty: Option<f64>,
    pub authorized_workers: Vec<String>,
    pub total_shares: u64,
    pub valid_shares: u64,
}

impl ConnectionInfo {
    pub fn from_connection(conn: &Connection) -> Self {
        Self {
            id: conn.id,
            address: conn.address,
            protocol: conn.protocol,
            state: conn.state,
            connected_at: conn.connected_at,
            last_activity: conn.last_activity,
            subscribed_difficulty: None,
            authorized_workers: Vec::new(),
            total_shares: 0,
            valid_shares: 0,
        }
    }

    pub fn add_share(&mut self, is_valid: bool) {
        self.total_shares += 1;
        if is_valid {
            self.valid_shares += 1;
        }
    }

    pub fn is_stale(&self) -> bool {
        let now = Utc::now();
        (now - self.last_activity).num_seconds() > 300 // 5 minutes
    }
}

/// Connection structure
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: Uuid,
    pub address: SocketAddr,
    pub protocol: Protocol,
    pub state: ConnectionState,
    pub connected_at: DateTime<Utc>,
    pub last_activity: DateTime<Utc>,
}

/// Worker information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub id: String,
    pub connection_id: ConnectionId,
    pub username: String,
    pub difficulty: f64,
    pub shares_submitted: u64,
    pub shares_accepted: u64,
    pub total_shares: u64,
    pub hashrate: f64,
    pub last_activity: DateTime<Utc>,
}

impl Worker {
    pub fn new(connection_id: ConnectionId, username: String, difficulty: f64) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            connection_id,
            username,
            difficulty,
            shares_submitted: 0,
            shares_accepted: 0,
            total_shares: 0,
            hashrate: 0.0,
            last_activity: Utc::now(),
        }
    }

    pub fn add_share(&mut self, accepted: bool) {
        self.shares_submitted += 1;
        self.total_shares += 1;
        if accepted {
            self.shares_accepted += 1;
        }
        self.last_activity = Utc::now();
    }

    pub fn is_active(&self) -> bool {
        let now = Utc::now();
        (now - self.last_activity).num_seconds() < 600 // Active if submitted in last 10 minutes
    }
}

/// Mining job
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub template_id: Uuid,
    pub difficulty: f64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

impl Job {
    pub fn new(template_id: Uuid, difficulty: f64) -> Self {
        let now = Utc::now();
        Self {
            id: Uuid::new_v4().to_string(),
            template_id,
            difficulty,
            created_at: now,
            expires_at: now + chrono::Duration::seconds(300),
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Share submission from miner
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSubmission {
    pub connection_id: ConnectionId,
    pub job_id: String,
    pub worker_name: String,
    pub nonce: u32,
    pub timestamp: u32,
    pub extranonce2: Vec<u8>,
    pub share: Share,
}

impl ShareSubmission {
    pub fn new(connection_id: ConnectionId, job_id: String, worker_name: String, nonce: u32) -> Self {
        let timestamp = Utc::now().timestamp() as u32;
        let share = Share::new(connection_id, nonce, timestamp, 1.0);
        Self {
            connection_id,
            job_id,
            worker_name,
            nonce,
            timestamp,
            extranonce2: Vec::new(),
            share,
        }
    }

    pub fn validate(&self) -> Result<()> {
        self.share.validate()
    }
}

/// Share data
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    pub connection_id: Uuid,
    pub nonce: u32,
    pub timestamp: u32,
    pub difficulty: f64,
    pub is_valid: bool,
    pub block_hash: Option<BlockHash>,
    pub submitted_at: DateTime<Utc>,
}

impl Share {
    pub fn new(connection_id: Uuid, nonce: u32, timestamp: u32, difficulty: f64) -> Self {
        Self {
            connection_id,
            nonce,
            timestamp,
            difficulty,
            is_valid: false,
            block_hash: None,
            submitted_at: Utc::now(),
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.difficulty <= 0.0 {
            return Err(Error::Validation("Invalid difficulty".to_string()));
        }

        let now = Utc::now().timestamp() as u32;
        // Reject shares older than 1 hour
        if self.timestamp < now.saturating_sub(3600) {
            return Err(Error::Validation("Share too old".to_string()));
        }
        // Reject shares more than 15 minutes in the future
        if self.timestamp > now + 900 {
            return Err(Error::Validation("Share timestamp in future".to_string()));
        }

        Ok(())
    }
}

/// Share validation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ShareResult {
    Accepted,
    Rejected(String),
    Stale,
    Valid,
    Invalid(String),
    Block(BlockHash),
}

/// Work template for mining
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkTemplate {
    pub id: Uuid,
    pub previous_hash: BlockHash,
    pub coinbase_tx: Transaction,
    pub transactions: Vec<Transaction>,
    pub difficulty: f64,
    pub timestamp: u32,
    pub expires_at: DateTime<Utc>,
}

impl WorkTemplate {
    pub fn new(
        previous_hash: BlockHash,
        coinbase_tx: Transaction,
        transactions: Vec<Transaction>,
        difficulty: f64,
    ) -> Self {
        Self {
            id: Uuid::new_v4(),
            previous_hash,
            coinbase_tx,
            transactions,
            difficulty,
            timestamp: Utc::now().timestamp() as u32,
            expires_at: Utc::now() + chrono::Duration::seconds(300), // 5 minutes
        }
    }

    pub fn is_expired(&self) -> bool {
        Utc::now() > self.expires_at
    }
}

/// Mining statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningStats {
    pub hashrate: f64,
    pub shares_per_minute: f64,
    pub acceptance_rate: f64,
    pub efficiency: f64,
    pub uptime: Duration,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub blocks_found: u64,
}

/// Pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_hashrate: f64,
    pub connected_workers: u64,
    pub connected_miners: u64,
    pub active_workers: u64,
    pub active_connections: u64,
    pub shares_per_minute: f64,
    pub blocks_found: u64,
    pub blocks_found_24h: u64,
    pub efficiency: f64,
    pub uptime: Duration,
}

/// Performance metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PerformanceMetrics {
    pub cpu_usage: f64,
    pub memory_usage: u64,
    pub memory_total: u64,
    pub network_rx_bytes: u64,
    pub network_tx_bytes: u64,
    pub disk_usage: u64,
    pub disk_total: u64,
    pub open_connections: u64,
    pub database_connections: u32,
    pub timestamp: DateTime<Utc>,
}

/// Alert information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub severity: AlertSeverity,
    pub message: String,
    pub timestamp: DateTime<Utc>,
    pub acknowledged: bool,
}

/// Alert severity levels
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertSeverity {
    Info,
    Warning,
    Error,
    Critical,
}

/// Alert level (alias for AlertSeverity)
pub type AlertLevel = AlertSeverity;

/// Daemon status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    pub mode: String,
    pub uptime: Duration,
    pub connections: u64,
    pub hashrate: f64,
}

/// Upstream pool status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamStatus {
    pub connected: bool,
    pub url: String,
    pub last_update: DateTime<Utc>,
    pub last_connected: Option<DateTime<Utc>>,
    pub hashrate: f64,
}

/// Block template for mining
pub type BlockTemplate = WorkTemplate;
