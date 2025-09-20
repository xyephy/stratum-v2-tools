use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::time::Instant;
use uuid::Uuid;
use bitcoin::{BlockHash, Transaction};

// Serde modules for Bitcoin types
mod bitcoin_hash_serde {
    use bitcoin::BlockHash;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(hash: &BlockHash, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        hash.to_string().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<BlockHash, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

mod bitcoin_hash_option_serde {
    use bitcoin::BlockHash;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(hash: &Option<BlockHash>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match hash {
            Some(h) => h.to_string().serialize(serializer),
            None => serializer.serialize_none(),
        }
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Option<BlockHash>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let opt = Option::<String>::deserialize(deserializer)?;
        match opt {
            Some(s) => Ok(Some(s.parse().map_err(serde::de::Error::custom)?)),
            None => Ok(None),
        }
    }
}

mod bitcoin_tx_serde {
    use bitcoin::Transaction;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(tx: &Transaction, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        bitcoin::consensus::encode::serialize_hex(tx).serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Transaction, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
        bitcoin::consensus::encode::deserialize(&bytes).map_err(serde::de::Error::custom)
    }
}

mod bitcoin_tx_vec_serde {
    use bitcoin::Transaction;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(txs: &Vec<Transaction>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let hex_txs: Vec<String> = txs
            .iter()
            .map(|tx| bitcoin::consensus::encode::serialize_hex(tx))
            .collect();
        hex_txs.serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Vec<Transaction>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let hex_txs = Vec::<String>::deserialize(deserializer)?;
        hex_txs
            .into_iter()
            .map(|s| {
                let bytes = hex::decode(&s).map_err(serde::de::Error::custom)?;
                bitcoin::consensus::encode::deserialize(&bytes).map_err(serde::de::Error::custom)
            })
            .collect()
    }
}

/// Unique identifier for connections
pub type ConnectionId = Uuid;

/// Unique identifier for work templates
pub type TemplateId = Uuid;

/// Protocol version enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Protocol {
    Sv1,
    Sv2,
    StratumV1,
    StratumV2,
}

/// Connection state tracking
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConnectionState {
    Connecting,
    Connected,
    Authenticated,
    Disconnecting,
    Disconnected,
    Error(String),
}

/// Connection information
#[derive(Debug, Clone)]
pub struct Connection {
    pub id: ConnectionId,
    pub address: SocketAddr,
    pub protocol: Protocol,
    pub state: ConnectionState,
    pub last_activity: Instant,
    pub user_agent: Option<String>,
}

/// Mining share representation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Share {
    pub connection_id: ConnectionId,
    pub nonce: u32,
    pub timestamp: u32,
    pub difficulty: f64,
    pub is_valid: bool,
    #[serde(with = "bitcoin_hash_option_serde", skip_serializing_if = "Option::is_none", default)]
    pub block_hash: Option<BlockHash>,
    pub submitted_at: chrono::DateTime<chrono::Utc>,
}

/// Work template for miners
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkTemplate {
    pub id: TemplateId,
    #[serde(with = "bitcoin_hash_serde")]
    pub previous_hash: BlockHash,
    #[serde(with = "bitcoin_tx_serde")]
    pub coinbase_tx: Transaction,
    #[serde(with = "bitcoin_tx_vec_serde")]
    pub transactions: Vec<Transaction>,
    pub difficulty: f64,
    pub timestamp: u32,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Share validation result
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ShareResult {
    Valid,
    Invalid(String),
    #[serde(with = "bitcoin_hash_serde")]
    Block(BlockHash),
}

/// Daemon status information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonStatus {
    pub running: bool,
    #[serde(serialize_with = "serialize_duration_as_secs")]
    pub uptime: std::time::Duration,
    pub active_connections: u64,
    pub total_connections: u64,
    pub mode: String,
    pub version: String,
    pub total_shares: u64,
    pub valid_shares: u64,
    pub blocks_found: u64,
    pub current_difficulty: f64,
    pub hashrate: f64,
}

impl Default for DaemonStatus {
    fn default() -> Self {
        Self {
            running: true,
            uptime: std::time::Duration::from_secs(0),
            active_connections: 0,
            total_connections: 0,
            mode: "Solo".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            total_shares: 0,
            valid_shares: 0,
            blocks_found: 0,
            current_difficulty: 1.0,
            hashrate: 0.0,
        }
    }
}

fn serialize_duration_as_secs<S>(duration: &std::time::Duration, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    serializer.serialize_u64(duration.as_secs())
}

/// Mining statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningStats {
    pub hashrate: f64,
    pub shares_per_minute: f64,
    pub acceptance_rate: f64,
    pub efficiency: f64,
    #[serde(serialize_with = "serialize_duration_as_secs")]
    pub uptime: std::time::Duration,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub blocks_found: u64,
}

impl Default for MiningStats {
    fn default() -> Self {
        Self {
            hashrate: 0.0,
            shares_per_minute: 0.0,
            acceptance_rate: 0.0,
            efficiency: 0.0,
            uptime: std::time::Duration::from_secs(0),
            shares_accepted: 0,
            shares_rejected: 0,
            blocks_found: 0,
        }
    }
}

impl Connection {
    pub fn new(address: SocketAddr, protocol: Protocol) -> Self {
        Self {
            id: Uuid::new_v4(),
            address,
            protocol,
            state: ConnectionState::Connecting,
            last_activity: Instant::now(),
            user_agent: None,
        }
    }

    pub fn update_activity(&mut self) {
        self.last_activity = Instant::now();
    }

    pub fn is_active(&self) -> bool {
        matches!(self.state, ConnectionState::Connected | ConnectionState::Authenticated)
    }
}

impl Share {
    pub fn new(connection_id: ConnectionId, nonce: u32, timestamp: u32, difficulty: f64) -> Self {
        Self {
            connection_id,
            nonce,
            timestamp,
            difficulty,
            is_valid: false,
            block_hash: None,
            submitted_at: chrono::Utc::now(),
        }
    }
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
            timestamp: chrono::Utc::now().timestamp() as u32,
            expires_at: chrono::Utc::now() + chrono::Duration::minutes(10),
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }
}

/// Extended connection information with additional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectionInfo {
    pub id: ConnectionId,
    pub address: SocketAddr,
    pub protocol: Protocol,
    pub state: ConnectionState,
    pub connected_at: chrono::DateTime<chrono::Utc>,
    pub last_activity: chrono::DateTime<chrono::Utc>,
    pub user_agent: Option<String>,
    pub version: Option<String>,
    pub subscribed_difficulty: Option<f64>,
    pub extranonce1: Option<String>,
    pub extranonce2_size: Option<u8>,
    pub authorized_workers: Vec<String>,
    pub total_shares: u64,
    pub valid_shares: u64,
    pub invalid_shares: u64,
    pub blocks_found: u64,
}

/// Worker information for pool mode
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Worker {
    pub name: String,
    pub connection_id: ConnectionId,
    pub difficulty: f64,
    pub last_share: Option<chrono::DateTime<chrono::Utc>>,
    pub total_shares: u64,
    pub valid_shares: u64,
    pub invalid_shares: u64,
    pub hashrate: f64,
    pub efficiency: f64,
}

/// Job information for SV2 protocol
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub template_id: TemplateId,
    pub version: u32,
    #[serde(with = "bitcoin_hash_serde")]
    pub previous_hash: BlockHash,
    pub merkle_root: String,
    pub timestamp: u32,
    pub bits: u32,
    pub target: String,
    pub clean_jobs: bool,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub expires_at: chrono::DateTime<chrono::Utc>,
}

/// Block template with additional metadata
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlockTemplate {
    pub template: WorkTemplate,
    pub height: u64,
    pub reward: u64,
    pub fees: u64,
    pub weight: u64,
    pub sigops: u64,
    pub min_time: u32,
    pub max_time: u32,
    pub mutable: Vec<String>,
    pub noncerange: String,
    pub capabilities: Vec<String>,
}

/// Share submission with extended information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShareSubmission {
    pub share: Share,
    pub job_id: String,
    pub extranonce2: String,
    pub ntime: u32,
    pub worker_name: String,
    pub user_agent: Option<String>,
    pub validation_result: Option<ShareResult>,
}

/// Pool statistics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    pub total_hashrate: f64,
    pub connected_miners: u64,
    pub active_workers: u64,
    pub shares_per_minute: f64,
    pub blocks_found_24h: u64,
    pub efficiency: f64,
    pub uptime: std::time::Duration,
    pub network_difficulty: f64,
    pub pool_difficulty: f64,
}

/// Network information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkInfo {
    pub chain: String,
    pub blocks: u64,
    pub headers: u64,
    pub difficulty: f64,
    pub median_time: u32,
    pub verification_progress: f64,
    pub chain_work: String,
    pub size_on_disk: u64,
    pub warnings: Vec<String>,
}

/// Upstream pool connection status
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamStatus {
    pub url: String,
    pub connected: bool,
    pub last_connected: Option<chrono::DateTime<chrono::Utc>>,
    pub connection_attempts: u32,
    pub last_error: Option<String>,
    pub latency: Option<std::time::Duration>,
    pub shares_submitted: u64,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
}

/// Alert levels for monitoring
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertLevel {
    Info,
    Warning,
    Error,
    Critical,
}

/// System alert
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Alert {
    pub id: Uuid,
    pub level: AlertLevel,
    pub title: String,
    pub message: String,
    pub component: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub resolved_at: Option<chrono::DateTime<chrono::Utc>>,
    pub metadata: std::collections::HashMap<String, String>,
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
    pub timestamp: chrono::DateTime<chrono::Utc>,
}

impl ConnectionInfo {
    pub fn from_connection(conn: &Connection) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: conn.id,
            address: conn.address,
            protocol: conn.protocol,
            state: conn.state.clone(),
            connected_at: now,
            last_activity: now,
            user_agent: conn.user_agent.clone(),
            version: None,
            subscribed_difficulty: None,
            extranonce1: None,
            extranonce2_size: None,
            authorized_workers: Vec::new(),
            total_shares: 0,
            valid_shares: 0,
            invalid_shares: 0,
            blocks_found: 0,
        }
    }

    pub fn update_activity(&mut self) {
        self.last_activity = chrono::Utc::now();
    }

    pub fn add_share(&mut self, is_valid: bool, is_block: bool) {
        self.total_shares += 1;
        if is_valid {
            self.valid_shares += 1;
        } else {
            self.invalid_shares += 1;
        }
        if is_block {
            self.blocks_found += 1;
        }
        self.update_activity();
    }

    pub fn acceptance_rate(&self) -> f64 {
        if self.total_shares == 0 {
            0.0
        } else {
            (self.valid_shares as f64 / self.total_shares as f64) * 100.0
        }
    }

    pub fn is_stale(&self, timeout_seconds: u64) -> bool {
        let timeout = chrono::Duration::seconds(timeout_seconds as i64);
        chrono::Utc::now() - self.last_activity > timeout
    }
}

impl Worker {
    pub fn new(name: String, connection_id: ConnectionId, difficulty: f64) -> Self {
        Self {
            name,
            connection_id,
            difficulty,
            last_share: None,
            total_shares: 0,
            valid_shares: 0,
            invalid_shares: 0,
            hashrate: 0.0,
            efficiency: 0.0,
        }
    }

    pub fn add_share(&mut self, is_valid: bool) {
        self.total_shares += 1;
        if is_valid {
            self.valid_shares += 1;
        } else {
            self.invalid_shares += 1;
        }
        self.last_share = Some(chrono::Utc::now());
        self.update_efficiency();
    }

    pub fn update_efficiency(&mut self) {
        if self.total_shares == 0 {
            self.efficiency = 0.0;
        } else {
            self.efficiency = (self.valid_shares as f64 / self.total_shares as f64) * 100.0;
        }
    }

    pub fn is_active(&self, timeout_minutes: i64) -> bool {
        match self.last_share {
            Some(last) => {
                let timeout = chrono::Duration::minutes(timeout_minutes);
                chrono::Utc::now() - last <= timeout
            }
            None => false,
        }
    }
}

impl Job {
    pub fn new(template: &WorkTemplate, clean_jobs: bool) -> Self {
        let now = chrono::Utc::now();
        Self {
            id: format!("{:x}", template.id.as_u128()),
            template_id: template.id,
            version: 0x20000000, // Default version
            previous_hash: template.previous_hash,
            merkle_root: "".to_string(), // Will be calculated
            timestamp: template.timestamp,
            bits: 0x207fffff, // Default difficulty bits
            target: "".to_string(), // Will be calculated from difficulty
            clean_jobs,
            created_at: now,
            expires_at: template.expires_at,
        }
    }

    pub fn is_expired(&self) -> bool {
        chrono::Utc::now() > self.expires_at
    }
}

impl ShareSubmission {
    pub fn new(
        connection_id: ConnectionId,
        job_id: String,
        extranonce2: String,
        ntime: u32,
        nonce: u32,
        worker_name: String,
        difficulty: f64,
    ) -> Self {
        let share = Share::new(connection_id, nonce, ntime, difficulty);
        Self {
            share,
            job_id,
            extranonce2,
            ntime,
            worker_name,
            user_agent: None,
            validation_result: None,
        }
    }

    pub fn validate(&mut self, template: &WorkTemplate) -> ShareResult {
        // Basic validation - in a real implementation this would be more comprehensive
        if template.is_expired() {
            let result = ShareResult::Invalid("Template expired".to_string());
            self.validation_result = Some(result.clone());
            return result;
        }

        // Simulate share validation
        if self.share.nonce % 1000 == 0 {
            // Simulate finding a block (very rare)
            let block_hash = template.previous_hash; // Simplified
            let result = ShareResult::Block(block_hash);
            self.validation_result = Some(result.clone());
            self.share.is_valid = true;
            self.share.block_hash = Some(block_hash);
            result
        } else if self.share.nonce % 10 != 0 {
            // 90% of shares are valid
            let result = ShareResult::Valid;
            self.validation_result = Some(result.clone());
            self.share.is_valid = true;
            result
        } else {
            // 10% of shares are invalid
            let result = ShareResult::Invalid("Low difficulty".to_string());
            self.validation_result = Some(result.clone());
            self.share.is_valid = false;
            result
        }
    }
}

impl Alert {
    pub fn new(level: AlertLevel, title: String, message: String, component: String) -> Self {
        Self {
            id: Uuid::new_v4(),
            level,
            title,
            message,
            component,
            created_at: chrono::Utc::now(),
            resolved_at: None,
            metadata: std::collections::HashMap::new(),
        }
    }

    pub fn resolve(&mut self) {
        self.resolved_at = Some(chrono::Utc::now());
    }

    pub fn is_resolved(&self) -> bool {
        self.resolved_at.is_some()
    }

    pub fn age(&self) -> chrono::Duration {
        chrono::Utc::now() - self.created_at
    }
}

impl PerformanceMetrics {
    pub fn new() -> Self {
        Self {
            cpu_usage: 0.0,
            memory_usage: 0,
            memory_total: 0,
            network_rx_bytes: 0,
            network_tx_bytes: 0,
            disk_usage: 0,
            disk_total: 0,
            open_connections: 0,
            database_connections: 0,
            timestamp: chrono::Utc::now(),
        }
    }

    pub fn memory_usage_percent(&self) -> f64 {
        if self.memory_total == 0 {
            0.0
        } else {
            (self.memory_usage as f64 / self.memory_total as f64) * 100.0
        }
    }

    pub fn disk_usage_percent(&self) -> f64 {
        if self.disk_total == 0 {
            0.0
        } else {
            (self.disk_usage as f64 / self.disk_total as f64) * 100.0
        }
    }
}

// Validation traits and implementations
pub trait Validate {
    type Error;
    fn validate(&self) -> Result<(), Self::Error>;
}

impl Validate for Share {
    type Error = String;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.difficulty <= 0.0 {
            return Err("Share difficulty must be positive".to_string());
        }

        if self.timestamp == 0 {
            return Err("Share timestamp cannot be zero".to_string());
        }

        // Check if timestamp is reasonable (not too far in the past or future)
        let now = chrono::Utc::now().timestamp() as u32;
        let max_age = 3600; // 1 hour
        let max_future = 300; // 5 minutes

        if self.timestamp < now.saturating_sub(max_age) {
            return Err("Share timestamp too old".to_string());
        }

        if self.timestamp > now + max_future {
            return Err("Share timestamp too far in the future".to_string());
        }

        Ok(())
    }
}

impl Validate for WorkTemplate {
    type Error = String;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.difficulty <= 0.0 {
            return Err("Template difficulty must be positive".to_string());
        }

        if self.timestamp == 0 {
            return Err("Template timestamp cannot be zero".to_string());
        }

        if self.is_expired() {
            return Err("Template has expired".to_string());
        }

        // Validate coinbase transaction
        if self.coinbase_tx.input.is_empty() {
            return Err("Coinbase transaction must have at least one input".to_string());
        }

        if self.coinbase_tx.output.is_empty() {
            return Err("Coinbase transaction must have at least one output".to_string());
        }

        Ok(())
    }
}

impl Validate for ConnectionInfo {
    type Error = String;

    fn validate(&self) -> Result<(), Self::Error> {
        if self.connected_at > chrono::Utc::now() {
            return Err("Connection time cannot be in the future".to_string());
        }

        if self.last_activity < self.connected_at {
            return Err("Last activity cannot be before connection time".to_string());
        }

        if let Some(difficulty) = self.subscribed_difficulty {
            if difficulty <= 0.0 {
                return Err("Subscribed difficulty must be positive".to_string());
            }
        }

        if let Some(size) = self.extranonce2_size {
            if size == 0 || size > 8 {
                return Err("Extranonce2 size must be between 1 and 8".to_string());
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bitcoin::{BlockHash, Transaction};
    use std::str::FromStr;

    #[test]
    fn test_connection_creation() {
        let addr = "127.0.0.1:3333".parse().unwrap();
        let conn = Connection::new(addr, Protocol::Sv2);
        
        assert_eq!(conn.address, addr);
        assert_eq!(conn.protocol, Protocol::Sv2);
        assert_eq!(conn.state, ConnectionState::Connecting);
        assert!(conn.user_agent.is_none());
    }

    #[test]
    fn test_connection_info_from_connection() {
        let addr = "127.0.0.1:3333".parse().unwrap();
        let conn = Connection::new(addr, Protocol::Sv1);
        let conn_info = ConnectionInfo::from_connection(&conn);
        
        assert_eq!(conn_info.id, conn.id);
        assert_eq!(conn_info.address, conn.address);
        assert_eq!(conn_info.protocol, conn.protocol);
        assert_eq!(conn_info.total_shares, 0);
        assert_eq!(conn_info.acceptance_rate(), 0.0);
    }

    #[test]
    fn test_worker_share_tracking() {
        let mut worker = Worker::new("test_worker".to_string(), Uuid::new_v4(), 1.0);
        
        assert_eq!(worker.total_shares, 0);
        assert_eq!(worker.efficiency, 0.0);
        
        worker.add_share(true);
        worker.add_share(true);
        worker.add_share(false);
        
        assert_eq!(worker.total_shares, 3);
        assert_eq!(worker.valid_shares, 2);
        assert_eq!(worker.invalid_shares, 1);
        assert!((worker.efficiency - 66.66666666666667).abs() < 0.0001);
    }

    #[test]
    fn test_share_validation() {
        let connection_id = Uuid::new_v4();
        let share = Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0);
        
        assert!(share.validate().is_ok());
        
        let invalid_share = Share {
            difficulty: -1.0,
            ..share
        };
        assert!(invalid_share.validate().is_err());
    }

    #[test]
    fn test_work_template_validation() {
        let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        
        let template = WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0);
        assert!(template.validate().is_ok());
        
        let invalid_template = WorkTemplate {
            difficulty: -1.0,
            ..template
        };
        assert!(invalid_template.validate().is_err());
    }

    #[test]
    fn test_share_submission_validation() {
        let connection_id = Uuid::new_v4();
        let mut submission = ShareSubmission::new(
            connection_id,
            "job123".to_string(),
            "abcd".to_string(),
            chrono::Utc::now().timestamp() as u32,
            12345,
            "worker1".to_string(),
            1.0,
        );
        
        let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        let template = WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0);
        
        let result = submission.validate(&template);
        assert!(matches!(result, ShareResult::Valid | ShareResult::Invalid(_) | ShareResult::Block(_)));
        assert!(submission.validation_result.is_some());
    }

    #[test]
    fn test_alert_lifecycle() {
        let mut alert = Alert::new(
            AlertLevel::Warning,
            "Test Alert".to_string(),
            "This is a test alert".to_string(),
            "test_component".to_string(),
        );
        
        assert!(!alert.is_resolved());
        assert!(alert.age().num_seconds() >= 0);
        
        alert.resolve();
        assert!(alert.is_resolved());
    }

    #[test]
    fn test_performance_metrics() {
        let metrics = PerformanceMetrics {
            memory_usage: 500,
            memory_total: 1000,
            disk_usage: 250,
            disk_total: 500,
            ..PerformanceMetrics::new()
        };
        
        assert_eq!(metrics.memory_usage_percent(), 50.0);
        assert_eq!(metrics.disk_usage_percent(), 50.0);
    }

    #[test]
    fn test_job_creation() {
        let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        let template = WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0);
        let job = Job::new(&template, true);
        
        assert_eq!(job.template_id, template.id);
        assert_eq!(job.previous_hash, template.previous_hash);
        assert!(job.clean_jobs);
        assert!(!job.is_expired());
    }

    #[test]
    fn test_serialization() {
        let connection_id = Uuid::new_v4();
        let share = Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0);
        
        let json = serde_json::to_string(&share).unwrap();
        let deserialized: Share = serde_json::from_str(&json).unwrap();
        
        assert_eq!(share.connection_id, deserialized.connection_id);
        assert_eq!(share.nonce, deserialized.nonce);
        assert_eq!(share.difficulty, deserialized.difficulty);
    }
}