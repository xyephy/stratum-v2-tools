use crate::{
    Result, Error, Connection, Share, ShareResult, WorkTemplate, MiningStats,
    config::{DaemonConfig, PoolConfig},
    database::DatabaseOps,
    types::{ConnectionId, ConnectionInfo, ConnectionState, Worker, Job, ShareSubmission, PoolStats},
    bitcoin_rpc::{BitcoinRpcClient, GetBlockTemplateResponse},
};
use async_trait::async_trait;
use std::sync::Arc;
use std::collections::HashMap;
use tokio::sync::{RwLock, mpsc, Mutex};
use tokio::time::{Duration, Instant, interval};


/// Pool mode handler for managing multiple miners
pub struct PoolModeHandler {
    config: PoolConfig,
    bitcoin_client: BitcoinRpcClient,
    database: Arc<dyn DatabaseOps>,
    
    // Connection management
    connections: Arc<RwLock<HashMap<ConnectionId, ConnectionInfo>>>,
    workers: Arc<RwLock<HashMap<String, Worker>>>,
    
    // Work distribution
    current_template: Arc<RwLock<Option<WorkTemplate>>>,
    active_jobs: Arc<RwLock<HashMap<String, Job>>>,
    
    // Statistics and monitoring
    pool_stats: Arc<RwLock<PoolStats>>,
    last_difficulty_adjustment: Arc<Mutex<Instant>>,
    
    // Communication channels
    share_tx: mpsc::UnboundedSender<ShareSubmission>,
    share_rx: Arc<Mutex<Option<mpsc::UnboundedReceiver<ShareSubmission>>>>,
    
    // Background task handles
    task_handles: Arc<Mutex<Vec<tokio::task::JoinHandle<()>>>>,
}

impl PoolModeHandler {
    /// Create a new pool mode handler
    pub fn new(
        config: PoolConfig,
        bitcoin_client: BitcoinRpcClient,
        database: Arc<dyn DatabaseOps>,
    ) -> Self {
        let (share_tx, share_rx) = mpsc::unbounded_channel();
        
        Self {
            config,
            bitcoin_client,
            database,
            connections: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
            current_template: Arc::new(RwLock::new(None)),
            active_jobs: Arc::new(RwLock::new(HashMap::new())),
            pool_stats: Arc::new(RwLock::new(PoolStats::default())),
            last_difficulty_adjustment: Arc::new(Mutex::new(Instant::now())),
            share_tx,
            share_rx: Arc::new(Mutex::new(Some(share_rx))),
            task_handles: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Start background tasks for pool management
    pub async fn start(&self) -> Result<()> {
        let mut handles = self.task_handles.lock().await;
        
        // Start share processing task
        if let Some(share_rx) = self.share_rx.lock().await.take() {
            let share_processor = self.start_share_processor(share_rx);
            handles.push(share_processor);
        }
        
        // Start work template refresh task
        let template_refresher = self.start_template_refresher();
        handles.push(template_refresher);
        
        // Start difficulty adjustment task
        let difficulty_adjuster = self.start_difficulty_adjuster();
        handles.push(difficulty_adjuster);
        
        // Start connection cleanup task
        let connection_cleaner = self.start_connection_cleaner();
        handles.push(connection_cleaner);
        
        // Start statistics updater
        let stats_updater = self.start_stats_updater();
        handles.push(stats_updater);
        
        Ok(())
    }

    /// Stop all background tasks
    pub async fn stop(&self) -> Result<()> {
        let mut handles = self.task_handles.lock().await;
        for handle in handles.drain(..) {
            handle.abort();
        }
        Ok(())
    }

    /// Add a new connection to the pool
    async fn add_connection(&self, conn: Connection) -> Result<()> {
        let conn_info = ConnectionInfo::from_connection(&conn);
        
        // Store in database
        self.database.create_connection(&conn_info).await?;
        
        // Add to in-memory tracking
        {
            let mut connections = self.connections.write().await;
            connections.insert(conn.id, conn_info);
        }
        
        // Update pool statistics
        {
            let mut stats = self.pool_stats.write().await;
            stats.connected_miners += 1;
        }
        
        println!("New connection added to pool: {} ({})", conn.address, conn.id);
        Ok(())
    }

    /// Remove a connection from the pool
    async fn remove_connection(&self, connection_id: ConnectionId) -> Result<()> {
        // Remove from in-memory tracking
        let removed = {
            let mut connections = self.connections.write().await;
            connections.remove(&connection_id)
        };
        
        if removed.is_some() {
            // Update database
            self.database.delete_connection(connection_id).await?;
            
            // Remove associated workers
            {
                let mut workers = self.workers.write().await;
                workers.retain(|_, worker| worker.connection_id != connection_id);
            }
            
            // Update pool statistics
            {
                let mut stats = self.pool_stats.write().await;
                stats.connected_miners = stats.connected_miners.saturating_sub(1);
            }
            
            println!("Connection removed from pool: {}", connection_id);
        }
        
        Ok(())
    }

    /// Authorize a worker for a connection
    async fn authorize_worker(&self, connection_id: ConnectionId, worker_name: String, difficulty: f64) -> Result<()> {
        let worker = Worker::new(worker_name.clone(), connection_id, difficulty);
        
        // Add to workers tracking
        {
            let mut workers = self.workers.write().await;
            workers.insert(worker_name.clone(), worker);
        }
        
        // Update connection info
        {
            let mut connections = self.connections.write().await;
            if let Some(conn_info) = connections.get_mut(&connection_id) {
                conn_info.authorized_workers.push(worker_name.clone());
                conn_info.subscribed_difficulty = Some(difficulty);
                conn_info.state = ConnectionState::Authenticated;
                
                // Update in database
                self.database.update_connection(conn_info).await?;
            }
        }
        
        // Update pool statistics
        {
            let mut stats = self.pool_stats.write().await;
            stats.active_workers += 1;
        }
        
        println!("Worker authorized: {} for connection {}", worker_name, connection_id);
        Ok(())
    }

    /// Get work for a specific connection/worker
    async fn get_work_for_connection(&self, connection_id: ConnectionId) -> Result<Job> {
        let template = {
            let template_guard = self.current_template.read().await;
            template_guard.clone().ok_or_else(|| Error::Protocol("No work template available".to_string()))?
        };
        
        // Get connection difficulty
        let _difficulty = {
            let connections = self.connections.read().await;
            connections.get(&connection_id)
                .and_then(|conn| conn.subscribed_difficulty)
                .unwrap_or(self.config.share_difficulty)
        };
        
        // Create job with connection-specific difficulty
        let job = Job::new(&template, false);
        
        // Store job for later validation
        {
            let mut jobs = self.active_jobs.write().await;
            jobs.insert(job.id.clone(), job.clone());
        }
        
        println!("Generated work for connection {}: job {}", connection_id, job.id);
        Ok(job)
    }

    /// Process a share submission
    async fn process_share_submission(&self, mut submission: ShareSubmission) -> Result<ShareResult> {
        // Validate job exists
        let job = {
            let jobs = self.active_jobs.read().await;
            jobs.get(&submission.job_id).cloned()
                .ok_or_else(|| Error::Protocol("Unknown job ID".to_string()))?
        };
        
        // Get work template for validation
        let template = self.database.get_work_template(job.template_id).await?
            .ok_or_else(|| Error::Protocol("Work template not found".to_string()))?;
        
        // Validate the share
        let result = submission.validate(&template);
        
        // Update worker statistics
        {
            let mut workers = self.workers.write().await;
            if let Some(worker) = workers.get_mut(&submission.worker_name) {
                worker.add_share(submission.share.is_valid);
            }
        }
        
        // Update connection statistics
        {
            let mut connections = self.connections.write().await;
            if let Some(conn_info) = connections.get_mut(&submission.share.connection_id) {
                conn_info.add_share(submission.share.is_valid, submission.share.block_hash.is_some());
                self.database.update_connection(conn_info).await?;
            }
        }
        
        // Store share in database
        self.database.create_share(&submission.share).await?;
        
        // Update pool statistics
        {
            let mut stats = self.pool_stats.write().await;
            stats.shares_per_minute += 1.0; // This would be calculated properly over time
            if submission.share.is_valid {
                // Update acceptance rate calculation
            }
            if submission.share.block_hash.is_some() {
                stats.blocks_found_24h += 1;
            }
        }
        
        println!("Processed share from {}: {:?}", submission.worker_name, result);
        Ok(result)
    }

    /// Adjust difficulty for variable difficulty mode
    async fn adjust_difficulty(&self) -> Result<()> {
        if !self.config.variable_difficulty {
            return Ok(());
        }
        
        let now = Instant::now();
        let mut last_adjustment = self.last_difficulty_adjustment.lock().await;
        
        if now.duration_since(*last_adjustment) < Duration::from_secs(self.config.difficulty_adjustment_interval) {
            return Ok(());
        }
        
        let mut workers = self.workers.write().await;
        let mut connections = self.connections.write().await;
        
        for worker in workers.values_mut() {
            // Calculate target share rate (e.g., 1 share per 30 seconds)
            let target_share_interval = 30.0; // seconds
            let current_rate = if worker.total_shares > 0 {
                // Simplified calculation - in reality would use time-based windows
                worker.total_shares as f64 / 60.0 // shares per minute approximation
            } else {
                0.0
            };
            
            let target_rate = 60.0 / target_share_interval; // target shares per minute
            
            if current_rate > target_rate * 1.2 {
                // Increase difficulty
                worker.difficulty = (worker.difficulty * 1.1).min(self.config.max_difficulty);
            } else if current_rate < target_rate * 0.8 && current_rate > 0.0 {
                // Decrease difficulty
                worker.difficulty = (worker.difficulty * 0.9).max(self.config.min_difficulty);
            }
            
            // Update connection info
            if let Some(conn_info) = connections.get_mut(&worker.connection_id) {
                conn_info.subscribed_difficulty = Some(worker.difficulty);
            }
        }
        
        *last_adjustment = now;
        println!("Difficulty adjustment completed");
        Ok(())
    }

    /// Start share processing background task
    fn start_share_processor(&self, mut share_rx: mpsc::UnboundedReceiver<ShareSubmission>) -> tokio::task::JoinHandle<()> {
        let handler = Arc::new(self.clone());
        
        tokio::spawn(async move {
            while let Some(submission) = share_rx.recv().await {
                if let Err(e) = handler.process_share_submission(submission).await {
                    eprintln!("Error processing share: {}", e);
                }
            }
        })
    }

    /// Start work template refresh background task
    fn start_template_refresher(&self) -> tokio::task::JoinHandle<()> {
        let handler = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(30)); // Refresh every 30 seconds
            
            loop {
                interval.tick().await;
                
                if let Err(e) = handler.refresh_work_template().await {
                    eprintln!("Error refreshing work template: {}", e);
                }
            }
        })
    }

    /// Start difficulty adjustment background task
    fn start_difficulty_adjuster(&self) -> tokio::task::JoinHandle<()> {
        let handler = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(handler.config.difficulty_adjustment_interval));
            
            loop {
                interval.tick().await;
                
                if let Err(e) = handler.adjust_difficulty().await {
                    eprintln!("Error adjusting difficulty: {}", e);
                }
            }
        })
    }

    /// Start connection cleanup background task
    fn start_connection_cleaner(&self) -> tokio::task::JoinHandle<()> {
        let handler = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(60)); // Check every minute
            
            loop {
                interval.tick().await;
                
                if let Err(e) = handler.cleanup_stale_connections().await {
                    eprintln!("Error cleaning up connections: {}", e);
                }
            }
        })
    }

    /// Start statistics updater background task
    fn start_stats_updater(&self) -> tokio::task::JoinHandle<()> {
        let handler = Arc::new(self.clone());
        
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(10)); // Update every 10 seconds
            
            loop {
                interval.tick().await;
                
                if let Err(e) = handler.update_pool_statistics().await {
                    eprintln!("Error updating pool statistics: {}", e);
                }
            }
        })
    }

    /// Refresh work template from Bitcoin node
    async fn refresh_work_template(&self) -> Result<()> {
        let block_template_response = self.bitcoin_client.get_block_template(None).await?;
        
        // Convert GetBlockTemplateResponse to WorkTemplate
        let template = self.convert_block_template_response(block_template_response)?;
        
        // Store template in database
        self.database.create_work_template(&template).await?;
        
        // Update current template
        {
            let mut current = self.current_template.write().await;
            *current = Some(template.clone());
        }
        
        // Clean up old jobs
        {
            let mut jobs = self.active_jobs.write().await;
            jobs.retain(|_, job| !job.is_expired());
        }
        
        println!("Work template refreshed: {}", template.id);
        Ok(())
    }

    /// Convert GetBlockTemplateResponse to WorkTemplate
    fn convert_block_template_response(&self, response: GetBlockTemplateResponse) -> Result<WorkTemplate> {
        use bitcoin::{BlockHash, Transaction};
        use std::str::FromStr;
        
        // Parse previous block hash
        let previous_hash = BlockHash::from_str(&response.previousblockhash)
            .map_err(|e| Error::Protocol(format!("Invalid previous block hash: {}", e)))?;
        
        // Create a simple coinbase transaction (simplified)
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        
        // Convert transactions (simplified - in reality would parse hex)
        let transactions = Vec::new(); // Simplified
        
        // Calculate difficulty from bits (parse hex string)
        let bits_value = u32::from_str_radix(&response.bits, 16)
            .map_err(|e| Error::Protocol(format!("Invalid bits format: {}", e)))?;
        let difficulty = self.bits_to_difficulty(bits_value);
        
        Ok(WorkTemplate::new(previous_hash, coinbase_tx, transactions, difficulty))
    }
    
    /// Convert bits to difficulty (simplified)
    fn bits_to_difficulty(&self, bits: u32) -> f64 {
        // Simplified difficulty calculation
        // In reality, this would use proper Bitcoin difficulty calculation
        if bits == 0 {
            1.0
        } else {
            0x1d00ffff as f64 / bits as f64
        }
    }

    /// Clean up stale connections
    async fn cleanup_stale_connections(&self) -> Result<()> {
        let stale_timeout = 300; // 5 minutes
        let mut stale_connections = Vec::new();
        
        {
            let connections = self.connections.read().await;
            for (id, conn_info) in connections.iter() {
                if conn_info.is_stale(stale_timeout) {
                    stale_connections.push(*id);
                }
            }
        }
        
        for connection_id in stale_connections {
            self.remove_connection(connection_id).await?;
        }
        
        Ok(())
    }

    /// Update pool statistics
    async fn update_pool_statistics(&self) -> Result<()> {
        let connections = self.connections.read().await;
        let workers = self.workers.read().await;
        
        let connected_miners = connections.len() as u64;
        let active_workers = workers.values().filter(|w| w.is_active(5)).count() as u64;
        
        // Calculate total hashrate (simplified)
        let total_hashrate: f64 = workers.values().map(|w| w.hashrate).sum();
        
        // Get share statistics from database
        let share_stats = self.database.get_share_stats(None).await?;
        let efficiency = share_stats.acceptance_rate;
        
        {
            let mut stats = self.pool_stats.write().await;
            stats.connected_miners = connected_miners;
            stats.active_workers = active_workers;
            stats.total_hashrate = total_hashrate;
            stats.efficiency = efficiency;
            // Other statistics would be calculated here
        }
        
        Ok(())
    }

    /// Get current pool statistics
    pub async fn get_pool_stats(&self) -> PoolStats {
        self.pool_stats.read().await.clone()
    }

    /// Get connection count with resource limits check
    pub async fn get_connection_count(&self) -> usize {
        self.connections.read().await.len()
    }

    /// Check if pool can accept new connections
    pub async fn can_accept_connection(&self) -> bool {
        let current_count = self.get_connection_count().await;
        current_count < 1000 // Max connections limit from requirements
    }

    /// Submit share to processing queue
    pub async fn submit_share(&self, submission: ShareSubmission) -> Result<()> {
        self.share_tx.send(submission)
            .map_err(|_| Error::Protocol("Share processing queue is full".to_string()))?;
        Ok(())
    }
}

// Implement Clone for background task spawning
impl Clone for PoolModeHandler {
    fn clone(&self) -> Self {
        let (share_tx, share_rx) = mpsc::unbounded_channel();
        
        Self {
            config: self.config.clone(),
            bitcoin_client: self.bitcoin_client.clone(),
            database: Arc::clone(&self.database),
            connections: Arc::clone(&self.connections),
            workers: Arc::clone(&self.workers),
            current_template: Arc::clone(&self.current_template),
            active_jobs: Arc::clone(&self.active_jobs),
            pool_stats: Arc::clone(&self.pool_stats),
            last_difficulty_adjustment: Arc::clone(&self.last_difficulty_adjustment),
            share_tx,
            share_rx: Arc::new(Mutex::new(Some(share_rx))),
            task_handles: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

#[async_trait]
impl crate::mode::ModeHandler for PoolModeHandler {
    /// Start the pool mode handler
    async fn start(&self) -> Result<()> {
        tracing::info!("Starting pool mode handler");
        // Pool mode doesn't need special startup procedures
        Ok(())
    }

    /// Stop the pool mode handler
    async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping pool mode handler");
        // Pool mode doesn't need special shutdown procedures
        Ok(())
    }

    async fn handle_connection(&self, conn: Connection) -> Result<()> {
        // Check connection limits
        if !self.can_accept_connection().await {
            return Err(Error::Protocol("Pool at maximum capacity".to_string()));
        }
        
        self.add_connection(conn).await
    }

    async fn process_share(&self, share: Share) -> Result<ShareResult> {
        // Create a share submission for processing
        let submission = ShareSubmission {
            share,
            job_id: "unknown".to_string(), // Would be provided by the protocol layer
            extranonce2: "".to_string(),
            ntime: chrono::Utc::now().timestamp() as u32,
            worker_name: "unknown".to_string(), // Would be provided by the protocol layer
            user_agent: None,
            validation_result: None,
        };
        
        self.process_share_submission(submission).await
    }

    async fn get_work_template(&self) -> Result<WorkTemplate> {
        let template = self.current_template.read().await;
        template.clone().ok_or_else(|| Error::Protocol("No work template available".to_string()))
    }

    async fn handle_disconnection(&self, connection_id: ConnectionId) -> Result<()> {
        self.remove_connection(connection_id).await
    }

    async fn get_statistics(&self) -> Result<MiningStats> {
        let pool_stats = self.get_pool_stats().await;
        
        Ok(MiningStats {
            hashrate: pool_stats.total_hashrate,
            shares_per_minute: pool_stats.shares_per_minute,
            acceptance_rate: pool_stats.efficiency,
            efficiency: pool_stats.efficiency,
            uptime: pool_stats.uptime,
            shares_accepted: 0, // TODO: implement share tracking
            shares_rejected: 0, // TODO: implement share tracking
            blocks_found: 0, // TODO: implement block tracking
        })
    }

    fn validate_config(&self, config: &DaemonConfig) -> Result<()> {
        if let crate::config::OperationModeConfig::Pool(pool_config) = &config.mode {
            if pool_config.share_difficulty <= 0.0 {
                return Err(Error::Config("Pool share difficulty must be positive".to_string()));
            }
            
            if pool_config.variable_difficulty {
                if pool_config.min_difficulty >= pool_config.max_difficulty {
                    return Err(Error::Config("Pool max difficulty must be greater than min difficulty".to_string()));
                }
            }
            
            if pool_config.fee_percentage < 0.0 || pool_config.fee_percentage > 100.0 {
                return Err(Error::Config("Pool fee percentage must be between 0 and 100".to_string()));
            }
        }
        
        Ok(())
    }
}

impl Default for PoolStats {
    fn default() -> Self {
        Self {
            total_hashrate: 0.0,
            connected_miners: 0,
            active_workers: 0,
            shares_per_minute: 0.0,
            blocks_found_24h: 0,
            efficiency: 0.0,
            uptime: Duration::from_secs(0),
            network_difficulty: 1.0,
            pool_difficulty: 1.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{database::MockDatabaseOps, mode::ModeHandler, config::BitcoinConfig};
    use std::net::SocketAddr;
    use crate::types::Protocol;

    fn create_test_bitcoin_config() -> BitcoinConfig {
        BitcoinConfig {
            rpc_url: "http://localhost:8332".to_string(),
            rpc_user: "user".to_string(),
            rpc_password: "pass".to_string(),
            network: crate::config::BitcoinNetwork::Regtest,
            coinbase_address: None,
            block_template_timeout: 30,
        }
    }

    #[tokio::test]
    async fn test_pool_handler_creation() {
        let config = PoolConfig::default();
        let bitcoin_client = BitcoinRpcClient::new(create_test_bitcoin_config());
        let database = Arc::new(MockDatabaseOps::new());
        
        let handler = PoolModeHandler::new(config, bitcoin_client, database);
        assert_eq!(handler.get_connection_count().await, 0);
        assert!(handler.can_accept_connection().await);
    }

    #[tokio::test]
    async fn test_connection_management() {
        let config = PoolConfig::default();
        let bitcoin_client = BitcoinRpcClient::new(create_test_bitcoin_config());
        let database = Arc::new(MockDatabaseOps::new());
        
        let handler = PoolModeHandler::new(config, bitcoin_client, database);
        
        let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
        let conn = Connection::new(addr, Protocol::Sv2);
        let conn_id = conn.id;
        
        // Test adding connection
        handler.handle_connection(conn).await.unwrap();
        assert_eq!(handler.get_connection_count().await, 1);
        
        // Test removing connection
        handler.handle_disconnection(conn_id).await.unwrap();
        assert_eq!(handler.get_connection_count().await, 0);
    }

    #[tokio::test]
    async fn test_worker_authorization() {
        let config = PoolConfig::default();
        let bitcoin_client = BitcoinRpcClient::new(create_test_bitcoin_config());
        let database = Arc::new(MockDatabaseOps::new());
        
        let handler = PoolModeHandler::new(config, bitcoin_client, database);
        
        let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
        let conn = Connection::new(addr, Protocol::Sv2);
        let conn_id = conn.id;
        
        handler.handle_connection(conn).await.unwrap();
        handler.authorize_worker(conn_id, "worker1".to_string(), 1.0).await.unwrap();
        
        let workers = handler.workers.read().await;
        assert!(workers.contains_key("worker1"));
        assert_eq!(workers.get("worker1").unwrap().connection_id, conn_id);
    }

    #[tokio::test]
    async fn test_pool_statistics() {
        let config = PoolConfig::default();
        let bitcoin_client = BitcoinRpcClient::new(create_test_bitcoin_config());
        let database = Arc::new(MockDatabaseOps::new());
        
        let handler = PoolModeHandler::new(config, bitcoin_client, database);
        
        let stats = handler.get_pool_stats().await;
        assert_eq!(stats.connected_miners, 0);
        assert_eq!(stats.active_workers, 0);
        assert_eq!(stats.total_hashrate, 0.0);
    }

    #[tokio::test]
    async fn test_config_validation() {
        let config = PoolConfig::default();
        let bitcoin_client = BitcoinRpcClient::new(create_test_bitcoin_config());
        let database = Arc::new(MockDatabaseOps::new());
        
        let handler = PoolModeHandler::new(config, bitcoin_client, database);
        
        // Create a pool mode daemon config
        let pool_daemon_config = DaemonConfig {
            mode: crate::config::OperationModeConfig::Pool(PoolConfig::default()),
            ..DaemonConfig::default()
        };
        assert!(handler.validate_config(&pool_daemon_config).is_ok());
        
        // Test invalid config
        let invalid_pool_config = PoolConfig {
            share_difficulty: -1.0,
            ..PoolConfig::default()
        };
        let invalid_daemon_config = DaemonConfig {
            mode: crate::config::OperationModeConfig::Pool(invalid_pool_config),
            ..DaemonConfig::default()
        };
        assert!(handler.validate_config(&invalid_daemon_config).is_err());
    }
}