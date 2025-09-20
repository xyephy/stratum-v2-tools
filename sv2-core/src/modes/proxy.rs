//! Proxy mode implementation for sv2d
//! 
//! This module implements the proxy mode functionality, which acts as a bridge
//! between downstream SV1 miners and upstream SV2 pools. It handles:
//! - Upstream pool connection management with failover
//! - Protocol translation between SV1 and SV2
//! - Load balancing across multiple upstream pools
//! - Share forwarding with proper attribution

use crate::{
    Result, Error, Connection, Share, ShareResult, WorkTemplate, MiningStats, ConnectionId,
    config::{ProxyConfig, UpstreamPool, LoadBalancingStrategy},
    database::DatabaseOps,
    protocol::ProtocolTranslator,
    types::{ConnectionState, UpstreamStatus, Alert, AlertLevel},
};
use super::proxy_protocol::{ProxyProtocolService, TranslationStats};
use crate::mode::ModeHandler;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::RwLock;
use std::collections::HashMap;
use tokio::sync::mpsc;
use tokio::time::Duration;
use tracing::{info, warn, error, debug};

/// Upstream pool connection manager
#[derive(Debug)]
struct UpstreamConnection {
    pool: UpstreamPool,
    status: UpstreamStatus,
    connection_handle: Option<tokio::task::JoinHandle<()>>,
    work_sender: Option<mpsc::UnboundedSender<WorkTemplate>>,
    share_receiver: Option<mpsc::UnboundedReceiver<Share>>,
    last_work_template: Option<WorkTemplate>,
    connection_attempts: u32,
    last_error: Option<String>,
}

impl UpstreamConnection {
    fn new(pool: UpstreamPool) -> Self {
        Self {
            status: UpstreamStatus {
                url: pool.url.clone(),
                connected: false,
                last_connected: None,
                connection_attempts: 0,
                last_error: None,
                latency: None,
                shares_submitted: 0,
                shares_accepted: 0,
                shares_rejected: 0,
            },
            pool,
            connection_handle: None,
            work_sender: None,
            share_receiver: None,
            last_work_template: None,
            connection_attempts: 0,
            last_error: None,
        }
    }

    async fn connect(&mut self) -> Result<()> {
        info!("Connecting to upstream pool: {}", self.pool.url);
        
        // Simulate connection establishment
        // In a real implementation, this would establish an actual SV2 connection
        self.connection_attempts += 1;
        
        // Simulate connection success/failure based on URL validity
        if self.pool.url.contains("invalid") {
            let error = format!("Failed to connect to {}", self.pool.url);
            self.last_error = Some(error.clone());
            self.status.last_error = Some(error.clone());
            return Err(Error::Connection(error));
        }

        self.status.connected = true;
        self.status.last_connected = Some(chrono::Utc::now());
        self.status.connection_attempts = self.connection_attempts;
        self.status.last_error = None;
        self.last_error = None;

        // Create communication channels
        let (work_tx, work_rx) = mpsc::unbounded_channel();
        let (share_tx, share_rx) = mpsc::unbounded_channel();
        
        self.work_sender = Some(work_tx);
        self.share_receiver = Some(share_rx);

        // Start connection handler task
        let pool_url = self.pool.url.clone();
        let pool_username = self.pool.username.clone();
        let pool_password = self.pool.password.clone();
        
        let handle = tokio::spawn(async move {
            Self::connection_handler(pool_url, pool_username, pool_password, work_rx, share_tx).await;
        });
        
        self.connection_handle = Some(handle);
        
        info!("Successfully connected to upstream pool: {}", self.pool.url);
        Ok(())
    }

    async fn disconnect(&mut self) {
        info!("Disconnecting from upstream pool: {}", self.pool.url);
        
        if let Some(handle) = self.connection_handle.take() {
            handle.abort();
        }
        
        self.work_sender = None;
        self.share_receiver = None;
        self.status.connected = false;
        self.last_work_template = None;
    }

    async fn connection_handler(
        url: String,
        username: String,
        password: String,
        mut work_rx: mpsc::UnboundedReceiver<WorkTemplate>,
        share_tx: mpsc::UnboundedSender<Share>,
    ) {
        info!("Starting connection handler for upstream pool: {}", url);
        
        // Simulate periodic work template updates
        let mut work_interval = tokio::time::interval(Duration::from_secs(30));
        
        loop {
            tokio::select! {
                _ = work_interval.tick() => {
                    // Simulate receiving new work from upstream pool
                    debug!("Simulating work template update from {}", url);
                    // In a real implementation, this would receive actual work from the upstream pool
                }
                
                work_template = work_rx.recv() => {
                    if work_template.is_none() {
                        break;
                    }
                    // Handle work template forwarding to upstream
                    debug!("Forwarding work template to upstream pool: {}", url);
                }
            }
        }
        
        info!("Connection handler for {} terminated", url);
    }

    fn is_connected(&self) -> bool {
        self.status.connected
    }

    fn get_status(&self) -> &UpstreamStatus {
        &self.status
    }

    async fn submit_share(&mut self, share: Share) -> Result<ShareResult> {
        if !self.is_connected() {
            return Err(Error::Connection("Upstream pool not connected".to_string()));
        }

        // Simulate share submission
        self.status.shares_submitted += 1;
        
        // Simulate acceptance/rejection based on share properties
        if share.is_valid {
            self.status.shares_accepted += 1;
            if share.block_hash.is_some() {
                Ok(ShareResult::Block(share.block_hash.unwrap()))
            } else {
                Ok(ShareResult::Valid)
            }
        } else {
            self.status.shares_rejected += 1;
            Ok(ShareResult::Invalid("Share rejected by upstream pool".to_string()))
        }
    }

    fn update_latency(&mut self, latency: Duration) {
        self.status.latency = Some(latency);
    }
}

/// Load balancer for upstream pool selection
#[derive(Debug)]
pub struct LoadBalancer {
    pub strategy: LoadBalancingStrategy,
    current_index: usize,
    pub connection_counts: HashMap<String, usize>,
}

impl LoadBalancer {
    pub fn new(strategy: LoadBalancingStrategy) -> Self {
        Self {
            strategy,
            current_index: 0,
            connection_counts: HashMap::new(),
        }
    }

    fn select_upstream<'a>(&mut self, upstreams: &'a [UpstreamConnection]) -> Option<&'a UpstreamConnection> {
        let connected_upstreams: Vec<&UpstreamConnection> = upstreams
            .iter()
            .filter(|u| u.is_connected())
            .collect();

        if connected_upstreams.is_empty() {
            return None;
        }

        match self.strategy {
            LoadBalancingStrategy::RoundRobin => {
                let selected = connected_upstreams.get(self.current_index % connected_upstreams.len())?;
                self.current_index = (self.current_index + 1) % connected_upstreams.len();
                Some(selected)
            }
            
            LoadBalancingStrategy::WeightedRoundRobin => {
                // Find upstream with highest weight among connected ones
                connected_upstreams
                    .iter()
                    .max_by_key(|u| u.pool.weight)
                    .copied()
            }
            
            LoadBalancingStrategy::LeastConnections => {
                // Select upstream with least connections
                connected_upstreams
                    .iter()
                    .min_by_key(|u| {
                        self.connection_counts.get(&u.pool.url).unwrap_or(&0)
                    })
                    .copied()
            }
            
            LoadBalancingStrategy::Random => {
                use rand::Rng;
                let mut rng = rand::thread_rng();
                let index = rng.gen_range(0..connected_upstreams.len());
                connected_upstreams.get(index).copied()
            }
        }
    }

    pub fn update_connection_count(&mut self, url: &str, delta: i32) {
        let count = self.connection_counts.entry(url.to_string()).or_insert(0);
        if delta < 0 && *count > 0 {
            *count -= (-delta) as usize;
        } else if delta > 0 {
            *count += delta as usize;
        }
    }
}

/// Proxy mode handler implementation
pub struct ProxyModeHandler {
    config: ProxyConfig,
    database: Arc<dyn DatabaseOps>,
    protocol_translator: Arc<ProtocolTranslator>,
    protocol_service: Arc<ProxyProtocolService>,
    upstream_connections: Arc<RwLock<Vec<UpstreamConnection>>>,
    load_balancer: Arc<RwLock<LoadBalancer>>,
    downstream_connections: Arc<RwLock<HashMap<ConnectionId, Connection>>>,
    connection_mapping: Arc<RwLock<HashMap<ConnectionId, String>>>, // downstream -> upstream URL
    stats: Arc<RwLock<MiningStats>>,
    alerts: Arc<RwLock<Vec<Alert>>>,
}

impl ProxyModeHandler {
    pub fn new(
        config: ProxyConfig,
        database: Arc<dyn DatabaseOps>,
    ) -> Self {
        let upstream_connections: Vec<UpstreamConnection> = config
            .upstream_pools
            .iter()
            .map(|pool| UpstreamConnection::new(pool.clone()))
            .collect();

        Self {
            load_balancer: Arc::new(RwLock::new(LoadBalancer::new(config.load_balancing.clone()))),
            config,
            database,
            protocol_translator: Arc::new(ProtocolTranslator::new()),
            protocol_service: Arc::new(ProxyProtocolService::new()),
            upstream_connections: Arc::new(RwLock::new(upstream_connections)),
            downstream_connections: Arc::new(RwLock::new(HashMap::new())),
            connection_mapping: Arc::new(RwLock::new(HashMap::new())),
            stats: Arc::new(RwLock::new(MiningStats {
                hashrate: 0.0,
                shares_per_minute: 0.0,
                acceptance_rate: 0.0,
                efficiency: 0.0,
                uptime: Duration::from_secs(0),
                shares_accepted: 0,
                shares_rejected: 0,
                blocks_found: 0,
            })),
            alerts: Arc::new(RwLock::new(Vec::new())),
        }
    }

    /// Initialize upstream connections
    pub async fn initialize(&self) -> Result<()> {
        info!("Initializing proxy mode with {} upstream pools", self.config.upstream_pools.len());
        
        let mut upstreams = self.upstream_connections.write().await;
        let mut connected_count = 0;
        
        for upstream in upstreams.iter_mut() {
            match upstream.connect().await {
                Ok(()) => {
                    connected_count += 1;
                    info!("Connected to upstream pool: {}", upstream.pool.url);
                }
                Err(e) => {
                    warn!("Failed to connect to upstream pool {}: {}", upstream.pool.url, e);
                    self.create_alert(
                        AlertLevel::Warning,
                        "Upstream Connection Failed".to_string(),
                        format!("Failed to connect to upstream pool {}: {}", upstream.pool.url, e),
                    ).await;
                }
            }
        }

        if connected_count == 0 {
            let error = "No upstream pools connected";
            error!("{}", error);
            self.create_alert(
                AlertLevel::Critical,
                "No Upstream Pools".to_string(),
                error.to_string(),
            ).await;
            return Err(Error::Connection(error.to_string()));
        }

        info!("Proxy mode initialized with {}/{} upstream pools connected", 
              connected_count, self.config.upstream_pools.len());

        // Start connection monitoring task
        self.start_connection_monitor().await;
        
        Ok(())
    }

    /// Start background task to monitor upstream connections
    async fn start_connection_monitor(&self) {
        let upstreams = Arc::clone(&self.upstream_connections);
        let config = self.config.clone();
        let alerts = Arc::clone(&self.alerts);
        
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(config.connection_retry_interval));
            
            loop {
                interval.tick().await;
                
                let mut upstreams_guard = upstreams.write().await;
                for upstream in upstreams_guard.iter_mut() {
                    if !upstream.is_connected() && upstream.connection_attempts < config.max_retry_attempts {
                        info!("Attempting to reconnect to upstream pool: {}", upstream.pool.url);
                        
                        match upstream.connect().await {
                            Ok(()) => {
                                info!("Reconnected to upstream pool: {}", upstream.pool.url);
                                
                                // Create recovery alert
                                let alert = Alert::new(
                                    AlertLevel::Info,
                                    "Upstream Reconnected".to_string(),
                                    format!("Successfully reconnected to upstream pool: {}", upstream.pool.url),
                                    "proxy".to_string(),
                                );
                                alerts.write().await.push(alert);
                            }
                            Err(e) => {
                                warn!("Failed to reconnect to upstream pool {}: {}", upstream.pool.url, e);
                                
                                if upstream.connection_attempts >= config.max_retry_attempts {
                                    let alert = Alert::new(
                                        AlertLevel::Error,
                                        "Upstream Connection Failed".to_string(),
                                        format!("Exhausted retry attempts for upstream pool: {}", upstream.pool.url),
                                        "proxy".to_string(),
                                    );
                                    alerts.write().await.push(alert);
                                }
                            }
                        }
                    }
                }
            }
        });
    }

    /// Create and store an alert
    async fn create_alert(&self, level: AlertLevel, title: String, message: String) {
        let alert = Alert::new(level, title, message, "proxy".to_string());
        self.alerts.write().await.push(alert);
    }

    /// Select best upstream pool for a new connection
    async fn select_upstream_for_connection(&self) -> Option<String> {
        let upstreams = self.upstream_connections.read().await;
        let mut load_balancer = self.load_balancer.write().await;
        
        load_balancer
            .select_upstream(&upstreams)
            .map(|u| u.pool.url.clone())
    }

    /// Forward share to appropriate upstream pool
    async fn forward_share_to_upstream(&self, share: Share, upstream_url: &str) -> Result<ShareResult> {
        let mut upstreams = self.upstream_connections.write().await;
        
        if let Some(upstream) = upstreams.iter_mut().find(|u| u.pool.url == upstream_url) {
            upstream.submit_share(share).await
        } else {
            Err(Error::Connection(format!("Upstream pool not found: {}", upstream_url)))
        }
    }

    /// Get current upstream pool statuses
    pub async fn get_upstream_statuses(&self) -> Vec<UpstreamStatus> {
        let upstreams = self.upstream_connections.read().await;
        upstreams.iter().map(|u| u.get_status().clone()).collect()
    }

    /// Get current alerts
    pub async fn get_alerts(&self) -> Vec<Alert> {
        self.alerts.read().await.clone()
    }

    /// Clear resolved alerts
    pub async fn clear_resolved_alerts(&self) {
        let mut alerts = self.alerts.write().await;
        alerts.retain(|alert| !alert.is_resolved());
    }

    /// Handle protocol message from downstream miner
    pub async fn handle_downstream_message(
        &self,
        connection_id: ConnectionId,
        message: crate::protocol::ProtocolMessage,
    ) -> Result<Vec<crate::protocol::ProtocolMessage>> {
        self.protocol_service.handle_downstream_message(connection_id, message).await
    }

    /// Forward work template to all connected downstream miners
    pub async fn broadcast_work_template(&self, template: &WorkTemplate) -> Result<()> {
        let active_connections = self.protocol_service.get_active_connections().await;
        
        if active_connections.is_empty() {
            debug!("No active downstream connections to forward work to");
            return Ok(());
        }

        let responses = self.protocol_service
            .forward_work_template(template, &active_connections)
            .await?;

        info!("Forwarded work template to {} downstream connections", responses.len());
        
        // In a real implementation, you would send these messages to the actual connections
        // For now, we just log that they would be sent
        for (connection_id, message) in responses {
            debug!("Would send {:?} to connection {}", message.message_type(), connection_id);
        }

        Ok(())
    }

    /// Get protocol translation statistics
    pub async fn get_translation_stats(&self) -> TranslationStats {
        self.protocol_service.get_translation_stats().await
    }

    /// Update difficulty for a specific connection
    pub async fn update_connection_difficulty(
        &self,
        connection_id: ConnectionId,
        difficulty: f64,
    ) -> Result<()> {
        self.protocol_service.update_connection_difficulty(connection_id, difficulty).await
    }
}

#[async_trait]
impl ModeHandler for ProxyModeHandler {
    /// Start the proxy mode handler
    async fn start(&self) -> Result<()> {
        tracing::info!("Starting proxy mode handler");
        // Proxy mode doesn't need special startup procedures
        Ok(())
    }

    /// Stop the proxy mode handler
    async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping proxy mode handler");
        // Proxy mode doesn't need special shutdown procedures
        Ok(())
    }

    async fn handle_connection(&self, mut conn: Connection) -> Result<()> {
        info!("Handling new downstream connection: {} ({:?})", conn.address, conn.protocol);
        
        // Select upstream pool for this connection
        let upstream_url = match self.select_upstream_for_connection().await {
            Some(url) => url,
            None => {
                error!("No available upstream pools for new connection");
                return Err(Error::Connection("No available upstream pools".to_string()));
            }
        };

        // Initialize protocol state for the connection
        if let Err(e) = self.protocol_service.initialize_connection(&conn).await {
            warn!("Failed to initialize protocol state: {}", e);
        }

        // Update connection state
        conn.state = ConnectionState::Connected;
        
        // Store connection mapping
        {
            let mut downstream_conns = self.downstream_connections.write().await;
            let mut mapping = self.connection_mapping.write().await;
            
            downstream_conns.insert(conn.id, conn.clone());
            mapping.insert(conn.id, upstream_url.clone());
        }

        // Update load balancer connection count
        {
            let mut load_balancer = self.load_balancer.write().await;
            load_balancer.update_connection_count(&upstream_url, 1);
        }

        // Store connection in database
        if let Err(e) = self.database.store_connection(&conn).await {
            warn!("Failed to store connection in database: {}", e);
        }

        info!("Connection {} assigned to upstream pool: {}", conn.id, upstream_url);
        Ok(())
    }

    async fn process_share(&self, share: Share) -> Result<ShareResult> {
        debug!("Processing share from connection: {}", share.connection_id);
        
        // Get upstream pool for this connection
        let upstream_url = {
            let mapping = self.connection_mapping.read().await;
            match mapping.get(&share.connection_id) {
                Some(url) => url.clone(),
                None => {
                    warn!("No upstream mapping found for connection: {}", share.connection_id);
                    return Err(Error::Connection("Connection not mapped to upstream pool".to_string()));
                }
            }
        };

        // Forward share to upstream pool
        let result = self.forward_share_to_upstream(share.clone(), &upstream_url).await?;
        
        // Store share in database
        if let Err(e) = self.database.store_share(&share).await {
            warn!("Failed to store share in database: {}", e);
        }

        // Update statistics
        {
            let mut stats = self.stats.write().await;
            stats.shares_per_minute += 1.0; // Simplified calculation
            
            match &result {
                ShareResult::Valid | ShareResult::Block(_) => {
                    stats.acceptance_rate = (stats.acceptance_rate * 0.9) + (100.0 * 0.1); // Moving average
                }
                ShareResult::Invalid(_) => {
                    stats.acceptance_rate = (stats.acceptance_rate * 0.9) + (0.0 * 0.1); // Moving average
                }
            }
        }

        debug!("Share processed with result: {:?}", result);
        Ok(result)
    }

    async fn get_work_template(&self) -> Result<WorkTemplate> {
        debug!("Getting work template for downstream miner");
        
        // Get work template from any connected upstream pool
        let upstreams = self.upstream_connections.read().await;
        
        for upstream in upstreams.iter() {
            if upstream.is_connected() {
                if let Some(template) = &upstream.last_work_template {
                    debug!("Returning cached work template from upstream: {}", upstream.pool.url);
                    return Ok(template.clone());
                }
            }
        }

        // If no cached template available, create a default one
        // In a real implementation, this would request fresh work from upstream
        warn!("No work template available from upstream pools, creating default");
        
        use bitcoin::{BlockHash, Transaction};
        use std::str::FromStr;
        
        let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000")
            .map_err(|e| Error::Protocol(format!("Invalid block hash: {}", e)))?;
        
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        
        Ok(WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0))
    }

    async fn handle_disconnection(&self, connection_id: ConnectionId) -> Result<()> {
        info!("Handling disconnection for connection: {}", connection_id);
        
        // Clean up protocol state
        if let Err(e) = self.protocol_service.cleanup_connection(connection_id).await {
            warn!("Failed to cleanup protocol state: {}", e);
        }
        
        // Get upstream URL for this connection
        let upstream_url = {
            let mut downstream_conns = self.downstream_connections.write().await;
            let mut mapping = self.connection_mapping.write().await;
            
            downstream_conns.remove(&connection_id);
            mapping.remove(&connection_id)
        };

        // Update load balancer connection count
        if let Some(url) = upstream_url {
            let mut load_balancer = self.load_balancer.write().await;
            load_balancer.update_connection_count(&url, -1);
        }

        // Update database
        if let Err(e) = self.database.update_connection_status(connection_id, ConnectionState::Disconnected).await {
            warn!("Failed to update connection status in database: {}", e);
        }

        Ok(())
    }

    async fn get_statistics(&self) -> Result<MiningStats> {
        let stats = self.stats.read().await.clone();
        Ok(stats)
    }

    fn validate_config(&self, config: &crate::config::DaemonConfig) -> Result<()> {
        if let crate::config::OperationModeConfig::Proxy(proxy_config) = &config.mode {
            if proxy_config.upstream_pools.is_empty() {
                return Err(Error::Config("Proxy mode requires at least one upstream pool".to_string()));
            }

            for (i, pool) in proxy_config.upstream_pools.iter().enumerate() {
                if pool.url.is_empty() {
                    return Err(Error::Config(format!("Upstream pool {} URL cannot be empty", i)));
                }
                if pool.username.is_empty() {
                    return Err(Error::Config(format!("Upstream pool {} username cannot be empty", i)));
                }
            }

            if proxy_config.connection_retry_interval == 0 {
                return Err(Error::Config("connection_retry_interval must be greater than 0".to_string()));
            }

            if proxy_config.max_retry_attempts == 0 {
                return Err(Error::Config("max_retry_attempts must be greater than 0".to_string()));
            }

            Ok(())
        } else {
            Err(Error::Config("Invalid configuration for proxy mode".to_string()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::MockDatabaseOps, types::Protocol};
    use std::net::SocketAddr;

    fn create_test_config() -> ProxyConfig {
        ProxyConfig {
            upstream_pools: vec![
                UpstreamPool {
                    url: "stratum+tcp://pool1.example.com:4444".to_string(),
                    username: "worker1".to_string(),
                    password: "pass1".to_string(),
                    priority: 1,
                    weight: 1,
                },
                UpstreamPool {
                    url: "stratum+tcp://pool2.example.com:4444".to_string(),
                    username: "worker2".to_string(),
                    password: "pass2".to_string(),
                    priority: 2,
                    weight: 2,
                },
            ],
            failover_enabled: true,
            load_balancing: LoadBalancingStrategy::RoundRobin,
            connection_retry_interval: 30,
            max_retry_attempts: 5,
        }
    }

    #[tokio::test]
    async fn test_proxy_mode_creation() {
        let config = create_test_config();
        let database = Arc::new(MockDatabaseOps::new());
        let handler = ProxyModeHandler::new(config.clone(), database);
        
        assert_eq!(handler.config.upstream_pools.len(), 2);
        assert_eq!(handler.config.load_balancing, LoadBalancingStrategy::RoundRobin);
    }

    #[tokio::test]
    async fn test_load_balancer_round_robin() {
        let mut balancer = LoadBalancer::new(LoadBalancingStrategy::RoundRobin);
        
        let upstreams = vec![
            UpstreamConnection::new(UpstreamPool {
                url: "pool1".to_string(),
                username: "user1".to_string(),
                password: "pass1".to_string(),
                priority: 1,
                weight: 1,
            }),
            UpstreamConnection::new(UpstreamPool {
                url: "pool2".to_string(),
                username: "user2".to_string(),
                password: "pass2".to_string(),
                priority: 1,
                weight: 1,
            }),
        ];

        // Mock connections as connected
        let mut connected_upstreams = upstreams;
        for upstream in &mut connected_upstreams {
            upstream.status.connected = true;
        }

        let first = balancer.select_upstream(&connected_upstreams);
        let second = balancer.select_upstream(&connected_upstreams);
        
        assert!(first.is_some());
        assert!(second.is_some());
        assert_ne!(first.unwrap().pool.url, second.unwrap().pool.url);
    }

    #[tokio::test]
    async fn test_connection_handling() {
        let config = create_test_config();
        let database = Arc::new(MockDatabaseOps::new());
        let handler = ProxyModeHandler::new(config, database);
        
        let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
        let conn = Connection::new(addr, Protocol::Sv1);
        
        // This will fail because no upstream pools are actually connected in the test
        let result = handler.handle_connection(conn).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_upstream_connection_creation() {
        let pool = UpstreamPool {
            url: "stratum+tcp://test.pool.com:4444".to_string(),
            username: "testuser".to_string(),
            password: "testpass".to_string(),
            priority: 1,
            weight: 1,
        };
        
        let upstream = UpstreamConnection::new(pool.clone());
        assert_eq!(upstream.pool.url, pool.url);
        assert!(!upstream.is_connected());
        assert_eq!(upstream.connection_attempts, 0);
    }
}