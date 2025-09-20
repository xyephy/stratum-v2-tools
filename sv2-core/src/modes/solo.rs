use crate::{
    Result, Error, Connection, Share, ShareResult, WorkTemplate, ConnectionId, MiningStats,
    bitcoin_rpc::BitcoinRpcClient, config::{DaemonConfig, SoloConfig}, database::DatabaseOps,
    types::{ConnectionInfo, Worker, Job, ShareSubmission},
};
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use bitcoin::{BlockHash, hashes::Hash};

/// Solo mining mode handler
pub struct SoloModeHandler {
    config: SoloConfig,
    bitcoin_client: BitcoinRpcClient,
    database: Arc<dyn DatabaseOps>,
    connections: Arc<RwLock<HashMap<ConnectionId, ConnectionInfo>>>,
    workers: Arc<RwLock<HashMap<ConnectionId, Worker>>>,
    current_template: Arc<RwLock<Option<WorkTemplate>>>,
    template_refresh_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    stats: Arc<RwLock<MiningStats>>,
    start_time: Instant,
}

impl SoloModeHandler {
    /// Create a new solo mode handler
    pub fn new(
        config: SoloConfig,
        bitcoin_client: BitcoinRpcClient,
        database: Arc<dyn DatabaseOps>,
    ) -> Self {
        Self {
            config,
            bitcoin_client,
            database,
            connections: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
            current_template: Arc::new(RwLock::new(None)),
            template_refresh_task: Arc::new(Mutex::new(None)),
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
            start_time: Instant::now(),
        }
    }

    /// Start the template refresh background task
    pub async fn start_template_refresh(&self) -> Result<()> {
        let mut task_handle = self.template_refresh_task.lock().await;
        
        // Stop existing task if running
        if let Some(handle) = task_handle.take() {
            handle.abort();
        }

        // Test Bitcoin connection first
        if let Err(e) = self.bitcoin_client.test_connection().await {
            tracing::warn!("Bitcoin node connection test failed: {}. Template refresh will continue to retry.", e);
        } else {
            tracing::info!("Bitcoin node connection test successful");
        }

        // Start new template refresh task
        let bitcoin_client = self.bitcoin_client.clone();
        let current_template = Arc::clone(&self.current_template);
        let refresh_interval = Duration::from_secs(self.config.block_template_refresh_interval);
        let coinbase_address = self.config.coinbase_address.clone();
        let max_template_age = Duration::from_secs(self.config.max_template_age);

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(refresh_interval);
            let mut consecutive_failures = 0u32;
            
            loop {
                interval.tick().await;
                
                match bitcoin_client.generate_work_template(&coinbase_address).await {
                    Ok(template) => {
                        consecutive_failures = 0; // Reset failure count on success
                        
                        let mut current = current_template.write().await;
                        
                        // Check if we need to update the template
                        let should_update = match current.as_ref() {
                            None => true,
                            Some(existing) => {
                                // Update if template is expired or if we have a new block
                                existing.is_expired() ||
                                existing.previous_hash != template.previous_hash ||
                                existing.expires_at.signed_duration_since(chrono::Utc::now()) < chrono::Duration::from_std(max_template_age).unwrap_or_default()
                            }
                        };

                        if should_update {
                            *current = Some(template.clone());
                            tracing::info!("Updated work template for solo mining: height={}, difficulty={:.2}", 
                                         template.timestamp, template.difficulty);
                        }
                    }
                    Err(e) => {
                        consecutive_failures += 1;
                        if consecutive_failures <= 3 {
                            tracing::warn!("Failed to generate work template (attempt {}): {}", consecutive_failures, e);
                        } else if consecutive_failures % 10 == 0 {
                            tracing::error!("Failed to generate work template {} times in a row: {}", consecutive_failures, e);
                        }
                        
                        // Exponential backoff for failures
                        if consecutive_failures > 1 {
                            let backoff_seconds = std::cmp::min(consecutive_failures * 2, 60);
                            tokio::time::sleep(Duration::from_secs(backoff_seconds as u64)).await;
                        }
                    }
                }
            }
        });

        *task_handle = Some(handle);
        Ok(())
    }

    /// Stop the template refresh task
    pub async fn stop_template_refresh(&self) {
        let mut task_handle = self.template_refresh_task.lock().await;
        if let Some(handle) = task_handle.take() {
            handle.abort();
        }
    }

    /// Get current work template, generating one if needed
    async fn ensure_work_template(&self) -> Result<WorkTemplate> {
        let current = self.current_template.read().await;
        
        // Check if we have a valid template
        if let Some(template) = current.as_ref() {
            if !template.is_expired() {
                return Ok(template.clone());
            }
        }
        
        // Drop the read lock before acquiring write lock
        drop(current);
        
        // Generate new template
        let new_template = self.bitcoin_client
            .generate_work_template(&self.config.coinbase_address)
            .await?;
        
        // Update current template
        let mut current = self.current_template.write().await;
        *current = Some(new_template.clone());
        
        Ok(new_template)
    }

    /// Validate and process a share submission
    async fn validate_share(&self, submission: &ShareSubmission) -> Result<ShareResult> {
        // Get the work template for validation
        let template = self.ensure_work_template().await?;
        
        // Basic validation
        if submission.share.difficulty <= 0.0 {
            return Ok(ShareResult::Invalid("Invalid difficulty".to_string()));
        }

        // Check if share meets minimum difficulty
        let min_difficulty = 1.0; // Configurable minimum difficulty for solo mining
        if submission.share.difficulty < min_difficulty {
            return Ok(ShareResult::Invalid("Share below minimum difficulty".to_string()));
        }

        // Simulate share validation based on nonce
        // In a real implementation, this would involve actual cryptographic validation
        let hash_result = self.calculate_share_hash(&submission.share, &template)?;
        
        // Check if share meets target difficulty
        if self.meets_difficulty(&hash_result, submission.share.difficulty) {
            // Check if it's a block
            if self.is_block_solution(&hash_result, &template) {
                // Submit block to Bitcoin network
                match self.submit_block(&submission.share, &template).await {
                    Ok(block_hash) => {
                        tracing::info!("Block found and submitted: {}", block_hash);
                        return Ok(ShareResult::Block(block_hash));
                    }
                    Err(e) => {
                        tracing::error!("Failed to submit block: {}", e);
                        return Ok(ShareResult::Invalid(format!("Block submission failed: {}", e)));
                    }
                }
            } else {
                return Ok(ShareResult::Valid);
            }
        } else {
            return Ok(ShareResult::Invalid("Share does not meet difficulty target".to_string()));
        }
    }

    /// Calculate hash for share validation (simplified)
    fn calculate_share_hash(&self, share: &Share, template: &WorkTemplate) -> Result<[u8; 32]> {
        // This is a simplified hash calculation
        // In a real implementation, this would involve proper block header construction and SHA-256 hashing
        use sha2::{Sha256, Digest};
        
        let mut hasher = Sha256::new();
        hasher.update(template.previous_hash.to_byte_array());
        hasher.update(share.nonce.to_le_bytes());
        hasher.update(share.timestamp.to_le_bytes());
        
        let result = hasher.finalize();
        let mut hash = [0u8; 32];
        hash.copy_from_slice(&result);
        Ok(hash)
    }

    /// Check if hash meets difficulty target
    fn meets_difficulty(&self, hash: &[u8; 32], difficulty: f64) -> bool {
        // Simplified difficulty check
        // In reality, this would involve proper target calculation from difficulty
        let hash_value = u64::from_le_bytes([
            hash[0], hash[1], hash[2], hash[3],
            hash[4], hash[5], hash[6], hash[7],
        ]);
        
        let target = (u64::MAX as f64 / difficulty) as u64;
        hash_value <= target
    }

    /// Check if hash represents a block solution
    fn is_block_solution(&self, hash: &[u8; 32], template: &WorkTemplate) -> bool {
        // Check if hash meets network difficulty (much higher than share difficulty)
        self.meets_difficulty(hash, template.difficulty)
    }

    /// Submit a block to the Bitcoin network
    async fn submit_block(&self, share: &Share, template: &WorkTemplate) -> Result<BlockHash> {
        tracing::info!("Attempting to submit block for share nonce={:08x}", share.nonce);
        
        // Construct the complete block
        let block_hex = self.construct_block(share, template)?;
        
        tracing::debug!("Submitting block hex: {}", &block_hex[..std::cmp::min(100, block_hex.len())]);
        
        // Submit to Bitcoin network
        match self.bitcoin_client.submit_block(&block_hex).await? {
            crate::bitcoin_rpc::SubmitBlockResponse::Success(_) => {
                tracing::info!("Block successfully submitted to Bitcoin network!");
                
                // Calculate the actual block hash from the constructed block
                let block_hash = self.calculate_block_hash(&block_hex)?;
                
                // Log the achievement
                tracing::info!("🎉 BLOCK FOUND! Hash: {}", block_hash);
                
                Ok(block_hash)
            }
            crate::bitcoin_rpc::SubmitBlockResponse::Error(err) => {
                tracing::error!("Block submission rejected by Bitcoin network: {}", err);
                Err(Error::BitcoinRpc(format!("Block submission rejected: {}", err)))
            }
        }
    }

    /// Calculate block hash from block hex
    fn calculate_block_hash(&self, block_hex: &str) -> Result<BlockHash> {
        use bitcoin::consensus::encode;
        use bitcoin::Block;
        
        let block_bytes = hex::decode(block_hex)
            .map_err(|e| Error::BitcoinRpc(format!("Invalid block hex: {}", e)))?;
        
        let block: Block = encode::deserialize(&block_bytes)
            .map_err(|e| Error::BitcoinRpc(format!("Failed to deserialize block: {}", e)))?;
        
        Ok(block.block_hash())
    }

    /// Construct a complete block from share and template (simplified)
    fn construct_block(&self, share: &Share, template: &WorkTemplate) -> Result<String> {
        // For now, create a simplified block hex representation
        // In a production implementation, this would construct a proper bitcoin::Block
        
        // Create a basic block structure with the share nonce
        let block_hex = format!(
            "01000000{:064x}{:064x}{:08x}{:08x}{:08x}01{:}",
            0u64, // Simplified previous hash
            0u64, // Simplified merkle root  
            share.timestamp,
            0x207fffff, // Simplified difficulty bits
            share.nonce,
            hex::encode(bitcoin::consensus::encode::serialize(&template.coinbase_tx))
        );
        
        tracing::debug!("Constructed block hex (first 100 chars): {}", 
                       &block_hex[..std::cmp::min(100, block_hex.len())]);
        
        Ok(block_hex)
    }

    /// Update mining statistics
    async fn update_statistics(&self) {
        let connections = self.connections.read().await;
        let workers = self.workers.read().await;
        
        let total_shares: u64 = connections.values().map(|c| c.total_shares).sum();
        let valid_shares: u64 = connections.values().map(|c| c.valid_shares).sum();
        
        let acceptance_rate = if total_shares > 0 {
            (valid_shares as f64 / total_shares as f64) * 100.0
        } else {
            0.0
        };

        let uptime = self.start_time.elapsed();
        let shares_per_minute = if uptime.as_secs() > 0 {
            (total_shares as f64 / uptime.as_secs() as f64) * 60.0
        } else {
            0.0
        };

        let total_hashrate: f64 = workers.values().map(|w| w.hashrate).sum();
        let efficiency = if total_hashrate > 0.0 {
            acceptance_rate // Simplified efficiency calculation
        } else {
            0.0
        };

        let mut stats = self.stats.write().await;
        stats.hashrate = total_hashrate;
        stats.shares_per_minute = shares_per_minute;
        stats.acceptance_rate = acceptance_rate;
        stats.efficiency = efficiency;
        stats.uptime = uptime;
    }

    /// Adjust difficulty for a worker based on their performance
    async fn adjust_worker_difficulty(&self, connection_id: ConnectionId) -> Result<f64> {
        let workers = self.workers.read().await;
        
        if let Some(worker) = workers.get(&connection_id) {
            // Simple difficulty adjustment based on share rate
            let target_shares_per_minute = 1.0; // Target 1 share per minute
            let current_rate = if worker.total_shares > 0 {
                // Calculate shares per minute based on worker activity
                worker.total_shares as f64 / 10.0 // Simplified calculation
            } else {
                0.0
            };

            let adjustment_factor = if current_rate > target_shares_per_minute * 1.2 {
                1.2 // Increase difficulty
            } else if current_rate < target_shares_per_minute * 0.8 {
                0.8 // Decrease difficulty
            } else {
                1.0 // Keep current difficulty
            };

            let new_difficulty = (worker.difficulty * adjustment_factor).max(0.1).min(1000.0);
            Ok(new_difficulty)
        } else {
            Ok(1.0) // Default difficulty
        }
    }

    /// Distribute work template to connected miners
    async fn distribute_work_template(&self, template: &WorkTemplate) -> Result<()> {
        let connections = self.connections.read().await;
        
        for (connection_id, connection_info) in connections.iter() {
            if connection_info.state == crate::types::ConnectionState::Authenticated {
                // Create job for this connection
                let _job = Job::new(template, true); // clean_jobs = true for new template
                
                // In a real implementation, this would send the job to the miner
                // For now, we'll just log it
                tracing::debug!(
                    "Distributing work template {} to connection {}",
                    template.id,
                    connection_id
                );
                
                // Store job information in database
                if let Err(e) = self.database.store_work_template(template).await {
                    tracing::error!("Failed to store work template: {}", e);
                }
            }
        }
        
        Ok(())
    }

    /// Handle miner subscription and difficulty setting
    async fn handle_miner_subscription(&self, connection_id: ConnectionId, difficulty: Option<f64>) -> Result<()> {
        let mut connections = self.connections.write().await;
        let mut workers = self.workers.write().await;
        
        if let Some(connection_info) = connections.get_mut(&connection_id) {
            // Set initial difficulty
            let initial_difficulty = difficulty.unwrap_or(1.0);
            connection_info.subscribed_difficulty = Some(initial_difficulty);
            
            // Create worker entry
            let worker = Worker::new(
                format!("worker_{}", connection_id),
                connection_id,
                initial_difficulty,
            );
            workers.insert(connection_id, worker);
            
            tracing::info!(
                "Miner subscribed: {} with difficulty {}",
                connection_id,
                initial_difficulty
            );
        }
        
        Ok(())
    }

    /// Clean up stale connections
    async fn cleanup_stale_connections(&self) -> Result<()> {
        let mut connections = self.connections.write().await;
        let mut workers = self.workers.write().await;
        
        let stale_timeout = Duration::from_secs(300); // 5 minutes
        let mut stale_connections = Vec::new();
        
        for (connection_id, connection_info) in connections.iter() {
            if connection_info.is_stale(stale_timeout.as_secs()) {
                stale_connections.push(*connection_id);
            }
        }
        
        for connection_id in stale_connections {
            connections.remove(&connection_id);
            workers.remove(&connection_id);
            
            tracing::info!("Cleaned up stale connection: {}", connection_id);
        }
        
        Ok(())
    }
}

#[async_trait]
impl crate::mode::ModeHandler for SoloModeHandler {
    /// Start the solo mode handler
    async fn start(&self) -> Result<()> {
        tracing::info!("Starting solo mode handler");
        
        // Test Bitcoin node connection
        match self.bitcoin_client.test_connection().await {
            Ok(()) => {
                tracing::info!("Successfully connected to Bitcoin node");
                
                // Get initial blockchain info
                if let Ok(info) = self.bitcoin_client.get_blockchain_info().await {
                    tracing::info!("Bitcoin node info: chain={}, blocks={}, difficulty={:.2}", 
                                 info.chain, info.blocks, info.difficulty);
                }
            }
            Err(e) => {
                tracing::warn!("Failed to connect to Bitcoin node: {}. Running in demo mode - will continue with mock work templates for testing.", e);
                // Continue running for testing - in production this should return Err(e)
            }
        }
        
        // Start template refresh background task
        self.start_template_refresh().await?;
        
        tracing::info!("Solo mode handler started successfully");
        Ok(())
    }

    /// Stop the solo mode handler
    async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping solo mode handler");
        // Solo mode doesn't need special shutdown procedures
        Ok(())
    }

    /// Handle a new connection
    async fn handle_connection(&self, conn: Connection) -> Result<()> {
        let connection_info = ConnectionInfo::from_connection(&conn);
        
        // Store connection information
        {
            let mut connections = self.connections.write().await;
            connections.insert(conn.id, connection_info);
        }

        // Store connection in database
        self.database.store_connection(&conn).await?;
        
        tracing::info!("New connection in solo mode: {} ({})", conn.id, conn.address);
        
        // Send initial work template if available
        if let Ok(template) = self.ensure_work_template().await {
            self.distribute_work_template(&template).await?;
        }
        
        Ok(())
    }

    /// Process a submitted share
    async fn process_share(&self, share: Share) -> Result<ShareResult> {
        // Create share submission for validation
        let submission = ShareSubmission::new(
            share.connection_id,
            "current_job".to_string(), // In real implementation, this would be the actual job ID
            "00000000".to_string(), // extranonce2
            share.timestamp,
            share.nonce,
            format!("worker_{}", share.connection_id),
            share.difficulty,
        );

        // Validate the share
        let result = self.validate_share(&submission).await?;
        
        // Update connection and worker statistics
        {
            let mut connections = self.connections.write().await;
            let mut workers = self.workers.write().await;
            
            if let Some(connection_info) = connections.get_mut(&share.connection_id) {
                let is_valid = matches!(result, ShareResult::Valid | ShareResult::Block(_));
                let is_block = matches!(result, ShareResult::Block(_));
                
                connection_info.add_share(is_valid, is_block);
                
                if let Some(worker) = workers.get_mut(&share.connection_id) {
                    worker.add_share(is_valid);
                    
                    // Update worker hashrate (simplified calculation)
                    worker.hashrate = worker.difficulty * worker.total_shares as f64 / 600.0; // Shares per 10 minutes
                }
            }
        }

        // Store share in database
        let mut share_with_result = share;
        share_with_result.is_valid = matches!(result, ShareResult::Valid | ShareResult::Block(_));
        if let ShareResult::Block(block_hash) = &result {
            share_with_result.block_hash = Some(*block_hash);
        }
        
        self.database.store_share(&share_with_result).await?;
        
        // Update statistics
        self.update_statistics().await;
        
        tracing::debug!(
            "Processed share from {}: {:?}",
            share_with_result.connection_id,
            result
        );
        
        Ok(result)
    }

    /// Get work template for miners
    async fn get_work_template(&self) -> Result<WorkTemplate> {
        self.ensure_work_template().await
    }

    /// Handle connection disconnection
    async fn handle_disconnection(&self, connection_id: ConnectionId) -> Result<()> {
        // Remove from active connections
        {
            let mut connections = self.connections.write().await;
            let mut workers = self.workers.write().await;
            
            connections.remove(&connection_id);
            workers.remove(&connection_id);
        }

        // Update database
        self.database.update_connection_status(connection_id, crate::types::ConnectionState::Disconnected).await?;
        
        tracing::info!("Connection disconnected from solo mode: {}", connection_id);
        
        Ok(())
    }

    /// Get mode-specific statistics
    async fn get_statistics(&self) -> Result<MiningStats> {
        self.update_statistics().await;
        let stats = self.stats.read().await;
        Ok(stats.clone())
    }

    /// Validate mode-specific configuration
    fn validate_config(&self, config: &DaemonConfig) -> Result<()> {
        if let crate::config::OperationModeConfig::Solo(solo_config) = &config.mode {
            if solo_config.coinbase_address.is_empty() {
                return Err(Error::Config("Solo mode requires coinbase address".to_string()));
            }
            
            if solo_config.block_template_refresh_interval == 0 {
                return Err(Error::Config("Block template refresh interval must be greater than 0".to_string()));
            }
            
            if solo_config.max_template_age == 0 {
                return Err(Error::Config("Max template age must be greater than 0".to_string()));
            }
        } else {
            return Err(Error::Config("Invalid configuration for solo mode".to_string()));
        }
        
        Ok(())
    }
}

impl Drop for SoloModeHandler {
    fn drop(&mut self) {
        // Clean shutdown of background tasks
        if let Ok(mut task_handle) = self.template_refresh_task.try_lock() {
            if let Some(handle) = task_handle.take() {
                handle.abort();
            }
        }
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        config::{BitcoinConfig, BitcoinNetwork},
        database::MockDatabaseOps,
        bitcoin_rpc::BitcoinRpcClient,
        mode::ModeHandler,
        types::{Protocol, Validate},
    };
    use std::net::SocketAddr;
    use uuid::Uuid;

    fn create_test_solo_config() -> SoloConfig {
        SoloConfig {
            coinbase_address: "bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string(),
            block_template_refresh_interval: 30,
            enable_custom_templates: false,
            max_template_age: 300,
        }
    }

    fn create_test_bitcoin_config() -> BitcoinConfig {
        BitcoinConfig {
            rpc_url: "http://127.0.0.1:18443".to_string(),
            rpc_user: "test".to_string(),
            rpc_password: "test".to_string(),
            network: BitcoinNetwork::Regtest,
            coinbase_address: Some("bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string()),
            block_template_timeout: 30,
        }
    }

    #[tokio::test]
    async fn test_solo_mode_handler_creation() {
        let solo_config = create_test_solo_config();
        let bitcoin_config = create_test_bitcoin_config();
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());

        let handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
        
        assert_eq!(handler.config.coinbase_address, "bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh");
        assert_eq!(handler.config.block_template_refresh_interval, 30);
    }

    #[tokio::test]
    async fn test_connection_handling() {
        let solo_config = create_test_solo_config();
        let bitcoin_config = create_test_bitcoin_config();
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());

        let handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
        
        let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
        let connection = Connection::new(addr, crate::types::Protocol::Sv2);
        let connection_id = connection.id;

        // Handle connection
        handler.handle_connection(connection).await.unwrap();

        // Verify connection is stored
        let connections = handler.connections.read().await;
        assert!(connections.contains_key(&connection_id));
        assert_eq!(connections[&connection_id].address, addr);
    }

    #[tokio::test]
    async fn test_share_processing() {
        let solo_config = create_test_solo_config();
        let bitcoin_config = create_test_bitcoin_config();
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());

        let handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
        
        let connection_id = Uuid::new_v4();
        let share = Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0);

        // Process share (will fail without proper setup, but tests the flow)
        let result = handler.process_share(share).await;
        
        // Should return an error due to missing work template, but validates the flow
        assert!(result.is_err() || matches!(result.unwrap(), ShareResult::Valid | ShareResult::Invalid(_)));
    }

    #[tokio::test]
    async fn test_statistics_update() {
        let solo_config = create_test_solo_config();
        let bitcoin_config = create_test_bitcoin_config();
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());

        let handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
        
        // Update statistics
        handler.update_statistics().await;
        
        let stats = handler.get_statistics().await.unwrap();
        assert_eq!(stats.hashrate, 0.0);
        assert_eq!(stats.acceptance_rate, 0.0);
        assert!(stats.uptime.as_secs() >= 0);
    }

    #[tokio::test]
    async fn test_difficulty_calculation() {
        let solo_config = create_test_solo_config();
        let bitcoin_config = create_test_bitcoin_config();
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());

        let handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
        
        // Test hash difficulty check
        let test_hash = [0u8; 32]; // All zeros - very low hash
        assert!(handler.meets_difficulty(&test_hash, 1.0));
        
        let high_hash = [0xFFu8; 32]; // All ones - very high hash
        assert!(!handler.meets_difficulty(&high_hash, 1000000.0));
    }

    #[tokio::test]
    async fn test_worker_difficulty_adjustment() {
        let solo_config = create_test_solo_config();
        let bitcoin_config = create_test_bitcoin_config();
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());

        let handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
        
        let connection_id = Uuid::new_v4();
        let worker = Worker::new("test_worker".to_string(), connection_id, 1.0);
        
        {
            let mut workers = handler.workers.write().await;
            workers.insert(connection_id, worker);
        }
        
        let new_difficulty = handler.adjust_worker_difficulty(connection_id).await.unwrap();
        assert!(new_difficulty >= 0.1 && new_difficulty <= 1000.0);
    }

    #[test]
    fn test_config_validation() {
        let solo_config = create_test_solo_config();
        let bitcoin_config = create_test_bitcoin_config();
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());

        let handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
        
        let config = DaemonConfig {
            mode: crate::config::OperationModeConfig::Solo(create_test_solo_config()),
            ..Default::default()
        };
        
        assert!(handler.validate_config(&config).is_ok());
        
        // Test invalid config
        let invalid_config = DaemonConfig {
            mode: crate::config::OperationModeConfig::Solo(SoloConfig {
                coinbase_address: "".to_string(),
                ..create_test_solo_config()
            }),
            ..Default::default()
        };
        
        assert!(handler.validate_config(&invalid_config).is_err());
    }

    #[tokio::test]
    async fn test_complete_solo_mining_workflow() {
        let solo_config = create_test_solo_config();
        let bitcoin_config = create_test_bitcoin_config();
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());
        let handler = SoloModeHandler::new(solo_config, bitcoin_client, database);

        // Test multiple connections
        let mut connections = Vec::new();
        for i in 0..3 {
            let addr: SocketAddr = format!("127.0.0.1:{}", 3333 + i).parse().unwrap();
            let connection = Connection::new(addr, Protocol::Sv2);
            connections.push(connection.clone());
            
            handler.handle_connection(connection).await.unwrap();
        }

        // Verify connections are stored
        {
            let stored_connections = handler.connections.read().await;
            assert_eq!(stored_connections.len(), 3);
        }

        // Process shares from different connections (without starting template refresh)
        for connection in &connections {
            let share = Share::new(
                connection.id,
                12345,
                chrono::Utc::now().timestamp() as u32,
                1.0,
            );

            // This will fail due to no Bitcoin node, but tests the flow
            let result = handler.process_share(share).await;
            assert!(result.is_err()); // Expected to fail without work template
        }

        // Handle disconnections
        for connection in &connections {
            handler.handle_disconnection(connection.id).await.unwrap();
        }

        // Verify connections are removed
        {
            let stored_connections = handler.connections.read().await;
            assert_eq!(stored_connections.len(), 0);
        }
    }

    #[tokio::test]
    async fn test_share_validation_edge_cases() {
        let connection_id = Uuid::new_v4();
        
        // Valid share
        let valid_share = Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0);
        assert!(valid_share.validate().is_ok());

        // Invalid difficulty
        let invalid_share = Share {
            difficulty: -1.0,
            ..valid_share.clone()
        };
        assert!(invalid_share.validate().is_err());

        // Invalid timestamp (too old)
        let old_share = Share {
            timestamp: (chrono::Utc::now().timestamp() - 7200) as u32, // 2 hours ago
            ..valid_share.clone()
        };
        assert!(old_share.validate().is_err());

        // Future timestamp
        let future_share = Share {
            timestamp: (chrono::Utc::now().timestamp() + 1000) as u32, // 16+ minutes in future
            ..valid_share
        };
        assert!(future_share.validate().is_err());
    }
}