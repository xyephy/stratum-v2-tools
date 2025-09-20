use crate::{
    Result, Error, Connection, Share, ShareResult, WorkTemplate, ConnectionId, MiningStats,
    config::{DaemonConfig, ClientConfig}, database::DatabaseOps,
    types::{ConnectionInfo, Worker, Job, UpstreamStatus, ConnectionState, BlockTemplate},
    mode::ModeHandler,
};
use bitcoin::hashes::Hash;
use async_trait::async_trait;
use std::sync::Arc;
use tokio::sync::{RwLock, Mutex};
use std::collections::HashMap;
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
// URL parsing will be done manually to avoid adding new dependencies

/// Client mode handler for connecting to upstream SV2 pools
pub struct ClientModeHandler {
    config: ClientConfig,
    database: Arc<dyn DatabaseOps>,
    connections: Arc<RwLock<HashMap<ConnectionId, ConnectionInfo>>>,
    workers: Arc<RwLock<HashMap<ConnectionId, Worker>>>,
    upstream_connection: Arc<RwLock<Option<TcpStream>>>,
    upstream_status: Arc<RwLock<UpstreamStatus>>,
    current_template: Arc<RwLock<Option<WorkTemplate>>>,
    custom_templates: Arc<RwLock<HashMap<uuid::Uuid, BlockTemplate>>>,
    job_negotiation_token: Arc<RwLock<Option<String>>>,
    reconnect_task: Arc<Mutex<Option<tokio::task::JoinHandle<()>>>>,
    stats: Arc<RwLock<MiningStats>>,
    start_time: Instant,
    job_negotiation_enabled: bool,
}

impl ClientModeHandler {
    /// Create a new client mode handler
    pub fn new(
        config: ClientConfig,
        database: Arc<dyn DatabaseOps>,
    ) -> Self {
        let upstream_status = UpstreamStatus {
            url: config.upstream_pool.url.clone(),
            connected: false,
            last_connected: None,
            connection_attempts: 0,
            last_error: None,
            latency: None,
            shares_submitted: 0,
            shares_accepted: 0,
            shares_rejected: 0,
        };

        Self {
            job_negotiation_enabled: config.enable_job_negotiation,
            config,
            database,
            connections: Arc::new(RwLock::new(HashMap::new())),
            workers: Arc::new(RwLock::new(HashMap::new())),
            upstream_connection: Arc::new(RwLock::new(None)),
            upstream_status: Arc::new(RwLock::new(upstream_status)),
            current_template: Arc::new(RwLock::new(None)),
            custom_templates: Arc::new(RwLock::new(HashMap::new())),
            job_negotiation_token: Arc::new(RwLock::new(None)),
            reconnect_task: Arc::new(Mutex::new(None)),
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

    /// Start the upstream connection and reconnection management
    pub async fn start_upstream_connection(&self) -> Result<()> {
        // Start initial connection
        self.connect_to_upstream().await?;

        // Start reconnection task
        let mut task_handle = self.reconnect_task.lock().await;
        
        // Stop existing task if running
        if let Some(handle) = task_handle.take() {
            handle.abort();
        }

        let upstream_status = Arc::clone(&self.upstream_status);
        let upstream_connection = Arc::clone(&self.upstream_connection);
        let config = self.config.clone();
        let reconnect_interval = Duration::from_secs(self.config.reconnect_interval);

        let handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(reconnect_interval);
            
            loop {
                interval.tick().await;
                
                // Check if connection is still alive
                let needs_reconnect = {
                    let status = upstream_status.read().await;
                    !status.connected
                };

                if needs_reconnect {
                    tracing::info!("Attempting to reconnect to upstream pool: {}", config.upstream_pool.url);
                    
                    match Self::establish_connection(&config.upstream_pool.url).await {
                        Ok(stream) => {
                            {
                                let mut connection = upstream_connection.write().await;
                                *connection = Some(stream);
                            }
                            
                            {
                                let mut status = upstream_status.write().await;
                                status.connected = true;
                                status.last_connected = Some(chrono::Utc::now());
                                status.connection_attempts += 1;
                                status.last_error = None;
                            }
                            
                            tracing::info!("Successfully reconnected to upstream pool");
                        }
                        Err(e) => {
                            let mut status = upstream_status.write().await;
                            status.connected = false;
                            status.connection_attempts += 1;
                            status.last_error = Some(e.to_string());
                            
                            tracing::error!("Failed to reconnect to upstream pool: {}", e);
                        }
                    }
                }
            }
        });

        *task_handle = Some(handle);
        Ok(())
    }

    /// Stop the reconnection task
    pub async fn stop_upstream_connection(&self) {
        let mut task_handle = self.reconnect_task.lock().await;
        if let Some(handle) = task_handle.take() {
            handle.abort();
        }

        // Close upstream connection
        let mut connection = self.upstream_connection.write().await;
        *connection = None;

        let mut status = self.upstream_status.write().await;
        status.connected = false;
    }

    /// Establish connection to upstream pool
    async fn connect_to_upstream(&self) -> Result<()> {
        let stream = Self::establish_connection(&self.config.upstream_pool.url).await?;
        
        // Perform SV2 handshake
        self.perform_sv2_handshake(&stream).await?;
        
        // Store connection
        {
            let mut connection = self.upstream_connection.write().await;
            *connection = Some(stream);
        }

        // Update status
        {
            let mut status = self.upstream_status.write().await;
            status.connected = true;
            status.last_connected = Some(chrono::Utc::now());
            status.connection_attempts += 1;
            status.last_error = None;
        }

        tracing::info!("Connected to upstream pool: {}", self.config.upstream_pool.url);
        Ok(())
    }

    /// Establish TCP connection to upstream pool
    async fn establish_connection(url: &str) -> Result<TcpStream> {
        // Parse URL manually to extract host and port
        let (host, port) = Self::parse_stratum_url(url)?;
        
        let address = format!("{}:{}", host, port);
        
        let stream = TcpStream::connect(&address).await
            .map_err(|e| Error::Connection(format!("Failed to connect to {}: {}", address, e)))?;
        
        Ok(stream)
    }

    /// Parse Stratum URL to extract host and port
    fn parse_stratum_url(url: &str) -> Result<(String, u16)> {
        // Remove protocol prefix if present
        let url = url.strip_prefix("stratum+tcp://")
            .or_else(|| url.strip_prefix("stratum://"))
            .or_else(|| url.strip_prefix("tcp://"))
            .unwrap_or(url);
        
        // Split host and port
        if let Some(colon_pos) = url.rfind(':') {
            let host = url[..colon_pos].to_string();
            let port_str = &url[colon_pos + 1..];
            
            if host.is_empty() {
                return Err(Error::Config("Empty host in upstream URL".to_string()));
            }
            
            let port = port_str.parse::<u16>()
                .map_err(|_| Error::Config(format!("Invalid port in upstream URL: {}", port_str)))?;
            
            Ok((host, port))
        } else {
            // No port specified, use default
            if url.is_empty() {
                return Err(Error::Config("Empty upstream URL".to_string()));
            }
            Ok((url.to_string(), 4444)) // Default Stratum port
        }
    }

    /// Perform SV2 protocol handshake
    async fn perform_sv2_handshake(&self, _stream: &TcpStream) -> Result<()> {
        // This is a simplified SV2 handshake implementation
        // In a real implementation, this would use the SRI crates for proper SV2 protocol handling
        
        // For now, we'll simulate the handshake process
        tracing::debug!("Performing SV2 handshake (simulated)");
        
        // Simulate setup connection message
        let _setup_msg = self.create_setup_connection_message()?;
        
        // Simulate response validation
        let simulated_response = vec![0x02, 0x00, 0x00, 0x10]; // SetupConnectionSuccess
        if !self.validate_setup_response(&simulated_response)? {
            return Err(Error::Protocol("Invalid setup response from upstream".to_string()));
        }

        // If job negotiation is enabled, simulate negotiation setup
        if self.job_negotiation_enabled {
            self.simulate_job_negotiation().await?;
        }
        
        tracing::info!("SV2 handshake completed successfully");
        Ok(())
    }

    /// Create SV2 setup connection message
    fn create_setup_connection_message(&self) -> Result<Vec<u8>> {
        // Simplified SV2 setup connection message
        // In a real implementation, this would use proper SV2 message serialization
        let mut message = Vec::new();
        
        // Message header (simplified)
        message.extend_from_slice(&[0x01, 0x00]); // Message type: SetupConnection
        message.extend_from_slice(&[0x00, 0x20]); // Message length: 32 bytes
        
        // Protocol version
        message.extend_from_slice(&[0x02, 0x00]); // Version 2
        
        // Flags
        message.extend_from_slice(&[0x00, 0x00]); // No special flags
        
        // Endpoint host (simplified)
        let endpoint = "sv2-client".as_bytes();
        message.extend_from_slice(&(endpoint.len() as u16).to_le_bytes());
        message.extend_from_slice(endpoint);
        
        // Pad to expected length
        while message.len() < 36 {
            message.push(0);
        }
        
        Ok(message)
    }

    /// Validate setup connection response
    fn validate_setup_response(&self, response: &[u8]) -> Result<bool> {
        // Simplified response validation
        // In a real implementation, this would properly parse SV2 messages
        if response.len() < 4 {
            return Ok(false);
        }
        
        // Check for success response (simplified)
        let message_type = u16::from_le_bytes([response[0], response[1]]);
        Ok(message_type == 0x02) // SetupConnectionSuccess
    }

    /// Simulate job negotiation protocol setup
    async fn simulate_job_negotiation(&self) -> Result<()> {
        if !self.config.enable_job_negotiation {
            return Ok(());
        }

        // Simulate allocate mining job token message
        let _allocate_msg = self.create_allocate_mining_job_token_message()?;

        // Simulate response
        let simulated_response = vec![0x51, 0x00, 0x00, 0x10]; // AllocateMiningJobTokenSuccess

        // Validate response (simplified)
        if !self.validate_allocate_response(&simulated_response)? {
            tracing::warn!("Job negotiation not supported by upstream pool, falling back to standard mode");
            return Ok(());
        }

        // Store job negotiation token (simulated)
        {
            let mut token = self.job_negotiation_token.write().await;
            *token = Some(format!("token_{}", uuid::Uuid::new_v4()));
        }

        tracing::info!("Job negotiation protocol enabled");
        Ok(())
    }

    /// Create allocate mining job token message
    fn create_allocate_mining_job_token_message(&self) -> Result<Vec<u8>> {
        // Simplified job negotiation message
        let mut message = Vec::new();
        
        // Message header
        message.extend_from_slice(&[0x50, 0x00]); // AllocateMiningJobToken
        message.extend_from_slice(&[0x00, 0x10]); // Message length: 16 bytes
        
        // User identifier (simplified)
        let user_id = self.config.upstream_pool.username.as_bytes();
        message.extend_from_slice(&(user_id.len() as u16).to_le_bytes());
        message.extend_from_slice(user_id);
        
        // Pad to expected length
        while message.len() < 20 {
            message.push(0);
        }
        
        Ok(message)
    }

    /// Validate allocate mining job token response
    fn validate_allocate_response(&self, response: &[u8]) -> Result<bool> {
        if response.len() < 4 {
            return Ok(false);
        }
        
        let message_type = u16::from_le_bytes([response[0], response[1]]);
        Ok(message_type == 0x51) // AllocateMiningJobTokenSuccess
    }

    /// Submit share to upstream pool
    async fn submit_share_to_upstream(&self, share: &Share) -> Result<ShareResult> {
        let connection = self.upstream_connection.read().await;
        
        if let Some(ref _stream) = connection.as_ref() {
            // Create share submission message
            let share_msg = self.create_share_submission_message(share)?;
            
            // Send share - we need to work around the borrow checker
            // In a real implementation, this would use proper async stream handling
            let share_msg_clone = share_msg.clone();
            
            // This is a simplified implementation - in reality we'd need proper stream management
            // For now, we'll simulate the network operation
            tracing::debug!("Would submit share with {} bytes to upstream", share_msg_clone.len());
            
            // Simulate response parsing
            let response = vec![0x07, 0x00, 0x00, 0x04]; // Simulate success response
            
            // Parse response
            let result = self.parse_share_response(&response)?;
            
            // Update upstream statistics
            {
                let mut status = self.upstream_status.write().await;
                status.shares_submitted += 1;
                
                match result {
                    ShareResult::Valid | ShareResult::Block(_) => {
                        status.shares_accepted += 1;
                    }
                    ShareResult::Invalid(_) => {
                        status.shares_rejected += 1;
                    }
                }
            }
            
            Ok(result)
        } else {
            Err(Error::Connection("No upstream connection available".to_string()))
        }
    }

    /// Create share submission message
    fn create_share_submission_message(&self, share: &Share) -> Result<Vec<u8>> {
        // Simplified share submission message
        let mut message = Vec::new();
        
        // Message header
        message.extend_from_slice(&[0x06, 0x00]); // SubmitSharesStandard
        message.extend_from_slice(&[0x00, 0x20]); // Message length: 32 bytes
        
        // Channel ID (simplified)
        message.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        
        // Sequence number (simplified)
        message.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        
        // Job ID (simplified)
        message.extend_from_slice(&[0x01, 0x00, 0x00, 0x00]);
        
        // Nonce
        message.extend_from_slice(&share.nonce.to_le_bytes());
        
        // Timestamp
        message.extend_from_slice(&share.timestamp.to_le_bytes());
        
        // Version (simplified)
        message.extend_from_slice(&[0x20, 0x00, 0x00, 0x00]);
        
        // Pad to expected length
        while message.len() < 36 {
            message.push(0);
        }
        
        Ok(message)
    }

    /// Parse share submission response
    fn parse_share_response(&self, response: &[u8]) -> Result<ShareResult> {
        if response.len() < 4 {
            return Ok(ShareResult::Invalid("Invalid response format".to_string()));
        }
        
        let message_type = u16::from_le_bytes([response[0], response[1]]);
        
        match message_type {
            0x07 => Ok(ShareResult::Valid), // SubmitSharesSuccess
            0x08 => {
                // SubmitSharesError - parse error message
                let error_msg = if response.len() > 8 {
                    String::from_utf8_lossy(&response[8..]).to_string()
                } else {
                    "Unknown error".to_string()
                };
                Ok(ShareResult::Invalid(error_msg))
            }
            0x09 => {
                // NewTemplate (block found)
                // In a real implementation, this would parse the block hash
                use bitcoin::hashes::Hash;
                let block_hash = bitcoin::BlockHash::all_zeros(); // Placeholder
                Ok(ShareResult::Block(block_hash))
            }
            _ => Ok(ShareResult::Invalid("Unknown response type".to_string())),
        }
    }

    /// Receive work from upstream pool
    async fn receive_work_from_upstream(&self) -> Result<Option<WorkTemplate>> {
        let connection = self.upstream_connection.read().await;
        
        if connection.is_some() {
            // In a real implementation, this would continuously listen for new work
            // For now, we'll simulate receiving work from upstream
            
            // Simulate work message
            let simulated_message = vec![0x71, 0x00, 0x00, 0x40]; // NewTemplate message
            
            // Parse received message
            if let Ok(template) = self.parse_work_message(&simulated_message) {
                return Ok(Some(template));
            }
        }
        
        Ok(None)
    }

    /// Parse work message from upstream
    fn parse_work_message(&self, message: &[u8]) -> Result<WorkTemplate> {
        // Simplified work message parsing
        // In a real implementation, this would properly parse SV2 NewTemplate messages
        
        if message.len() < 16 {
            return Err(Error::Protocol("Invalid work message format".to_string()));
        }
        
        let message_type = u16::from_le_bytes([message[0], message[1]]);
        
        if message_type != 0x71 { // NewTemplate
            return Err(Error::Protocol("Not a work template message".to_string()));
        }
        
        // Create a simplified work template
        // In a real implementation, this would parse the actual template data
        use bitcoin::{BlockHash, Transaction, TxIn, TxOut, hashes::Hash};
        
        let previous_hash = BlockHash::all_zeros(); // Would be parsed from message
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![TxIn::default()],
            output: vec![TxOut::default()],
        };
        
        let template = WorkTemplate::new(
            previous_hash,
            coinbase_tx,
            vec![], // Transactions would be parsed from message
            1.0,    // Difficulty would be parsed from message
        );
        
        Ok(template)
    }

    /// Update mining statistics
    async fn update_statistics(&self) {
        let connections = self.connections.read().await;
        let workers = self.workers.read().await;
        let upstream_status = self.upstream_status.read().await;
        
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
        
        // Calculate efficiency based on upstream acceptance rate
        let efficiency = if upstream_status.shares_submitted > 0 {
            (upstream_status.shares_accepted as f64 / upstream_status.shares_submitted as f64) * 100.0
        } else {
            acceptance_rate
        };

        let mut stats = self.stats.write().await;
        stats.hashrate = total_hashrate;
        stats.shares_per_minute = shares_per_minute;
        stats.acceptance_rate = acceptance_rate;
        stats.efficiency = efficiency;
        stats.uptime = uptime;
    }

    /// Get upstream connection status
    pub async fn get_upstream_status(&self) -> UpstreamStatus {
        self.upstream_status.read().await.clone()
    }

    /// Handle miner subscription in client mode
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
                "Miner subscribed in client mode: {} with difficulty {}",
                connection_id,
                initial_difficulty
            );
        }
        
        Ok(())
    }

    /// Distribute work template to connected miners
    async fn distribute_work_template(&self, template: &WorkTemplate) -> Result<()> {
        let connections = self.connections.read().await;
        
        for (connection_id, connection_info) in connections.iter() {
            if connection_info.state == ConnectionState::Authenticated {
                // Create job for this connection
                let _job = Job::new(template, true); // clean_jobs = true for new template
                
                // In a real implementation, this would send the job to the miner
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
}

impl ClientModeHandler {
    /// Propose a custom block template for job negotiation
    pub async fn propose_custom_template(&self, template: BlockTemplate) -> Result<String> {
        if !self.job_negotiation_enabled {
            return Err(Error::Protocol("Job negotiation is not enabled".to_string()));
        }

        // Check if we have a job negotiation token
        let token = {
            let token_guard = self.job_negotiation_token.read().await;
            token_guard.clone()
        };

        let job_token = token.ok_or_else(|| {
            Error::Protocol("No job negotiation token available".to_string())
        })?;

        // Validate the custom template
        self.validate_custom_template(&template).await?;

        // Store the custom template
        let template_id = template.template.id;
        {
            let mut templates = self.custom_templates.write().await;
            templates.insert(template_id, template.clone());
        }

        // Create and send declare mining job message
        let declare_msg = self.create_declare_mining_job_message(&template, &job_token)?;
        
        // In a real implementation, this would send the message to upstream
        tracing::debug!("Would send declare mining job message with {} bytes", declare_msg.len());

        // Simulate response - in reality this would come from upstream
        let job_id = format!("custom_{}", template_id);
        
        tracing::info!("Proposed custom template with job ID: {}", job_id);
        Ok(job_id)
    }

    /// Validate a custom block template against consensus rules
    async fn validate_custom_template(&self, template: &BlockTemplate) -> Result<()> {
        // Basic validation checks - coinbase is separate from transactions in our model

        // Validate coinbase transaction
        let coinbase = &template.template.coinbase_tx;
        if coinbase.input.is_empty() {
            return Err(Error::Template("Coinbase transaction must have at least one input".to_string()));
        }

        if coinbase.output.is_empty() {
            return Err(Error::Template("Coinbase transaction must have at least one output".to_string()));
        }

        // Check block weight limits
        if template.weight > 4_000_000 {
            return Err(Error::Template("Block weight exceeds maximum limit".to_string()));
        }

        // Check sigops limit
        if template.sigops > 80_000 {
            return Err(Error::Template("Block sigops exceed maximum limit".to_string()));
        }

        // Validate timestamp
        let now = chrono::Utc::now().timestamp() as u32;
        if template.min_time > now {
            return Err(Error::Template("Block min_time is in the future".to_string()));
        }

        if template.max_time < now {
            return Err(Error::Template("Block max_time is in the past".to_string()));
        }

        // Check difficulty
        if template.template.difficulty <= 0.0 {
            return Err(Error::Template("Template difficulty must be positive".to_string()));
        }

        tracing::debug!("Custom template validation passed");
        Ok(())
    }

    /// Create declare mining job message for job negotiation
    fn create_declare_mining_job_message(&self, template: &BlockTemplate, job_token: &str) -> Result<Vec<u8>> {
        let mut message = Vec::new();
        
        // Message header
        message.extend_from_slice(&[0x52, 0x00]); // DeclareMiningJob
        message.extend_from_slice(&[0x00, 0x80]); // Message length: 128 bytes (simplified)
        
        // Job token
        let token_bytes = job_token.as_bytes();
        message.extend_from_slice(&(token_bytes.len() as u16).to_le_bytes());
        message.extend_from_slice(token_bytes);
        
        // Template ID
        message.extend_from_slice(template.template.id.as_bytes());
        
        // Block version
        message.extend_from_slice(&0x20000000u32.to_le_bytes());
        
        // Previous block hash
        message.extend_from_slice(&template.template.previous_hash.to_byte_array());
        
        // Merkle root (simplified - would be calculated from transactions)
        message.extend_from_slice(&[0u8; 32]);
        
        // Timestamp
        message.extend_from_slice(&template.template.timestamp.to_le_bytes());
        
        // Bits (difficulty target)
        message.extend_from_slice(&0x207fffffu32.to_le_bytes());
        
        // Pad to expected length
        while message.len() < 132 {
            message.push(0);
        }
        
        Ok(message)
    }

    /// Handle declare mining job response
    pub async fn handle_declare_job_response(&self, response: &[u8]) -> Result<Option<String>> {
        if response.len() < 4 {
            return Ok(None);
        }
        
        let message_type = u16::from_le_bytes([response[0], response[1]]);
        
        match message_type {
            0x53 => {
                // DeclareMiningJobSuccess
                if response.len() >= 8 {
                    let job_id_len = u16::from_le_bytes([response[4], response[5]]) as usize;
                    if response.len() >= 6 + job_id_len {
                        let job_id = String::from_utf8_lossy(&response[6..6 + job_id_len]).to_string();
                        tracing::info!("Custom job accepted with ID: {}", job_id);
                        return Ok(Some(job_id));
                    }
                }
                Ok(None)
            }
            0x54 => {
                // DeclareMiningJobError
                let error_msg = if response.len() > 8 {
                    String::from_utf8_lossy(&response[8..]).to_string()
                } else {
                    "Unknown error".to_string()
                };
                tracing::warn!("Custom job rejected: {}", error_msg);
                Err(Error::Protocol(format!("Job negotiation failed: {}", error_msg)))
            }
            _ => Ok(None),
        }
    }

    /// Create a custom block template with preferred transactions
    pub async fn create_custom_template(&self, preferred_transactions: Vec<bitcoin::Transaction>) -> Result<BlockTemplate> {
        // Get current network state (simplified)
        let current_template = self.current_template.read().await;
        let base_template = current_template.as_ref()
            .ok_or_else(|| Error::Template("No base template available".to_string()))?;

        // Create custom coinbase transaction
        let coinbase_tx = self.create_custom_coinbase_transaction()?;

        // Combine preferred transactions with base template transactions
        let mut transactions = vec![coinbase_tx.clone()];
        
        // Add preferred transactions first
        transactions.extend(preferred_transactions);
        
        // Add remaining transactions from base template (up to block size limit)
        let mut total_weight = self.calculate_transaction_weight(&coinbase_tx);
        
        for tx in &base_template.transactions {
            let tx_weight = self.calculate_transaction_weight(tx);
            if total_weight + tx_weight <= 3_900_000 { // Leave some room for safety
                transactions.push(tx.clone());
                total_weight += tx_weight;
            }
        }

        // Calculate fees
        let fees = self.calculate_total_fees(&transactions[1..]); // Exclude coinbase

        // Create block template
        let template = WorkTemplate::new(
            base_template.previous_hash,
            coinbase_tx,
            transactions[1..].to_vec(), // Exclude coinbase from transactions list
            base_template.difficulty,
        );

        let block_template = BlockTemplate {
            template,
            height: 0, // Would be obtained from Bitcoin node
            reward: 625_000_000, // Current block reward in satoshis
            fees,
            weight: total_weight,
            sigops: self.calculate_total_sigops(&transactions),
            min_time: chrono::Utc::now().timestamp() as u32 - 3600, // 1 hour ago
            max_time: chrono::Utc::now().timestamp() as u32 + 7200, // 2 hours from now
            mutable: vec!["time".to_string(), "transactions".to_string(), "prevblock".to_string()],
            noncerange: "00000000ffffffff".to_string(),
            capabilities: vec!["proposal".to_string()],
        };

        Ok(block_template)
    }

    /// Create custom coinbase transaction
    fn create_custom_coinbase_transaction(&self) -> Result<bitcoin::Transaction> {
        use bitcoin::{Transaction, TxIn, TxOut, OutPoint, ScriptBuf, Amount};
        
        // Create coinbase input
        let coinbase_input = TxIn {
            previous_output: OutPoint::null(),
            script_sig: ScriptBuf::new(), // Simplified coinbase script
            sequence: bitcoin::Sequence::MAX,
            witness: bitcoin::Witness::new(),
        };

        // Create coinbase output (simplified)
        let coinbase_output = TxOut {
            value: bitcoin::Amount::from_sat(625_000_000).to_sat(), // Block reward
            script_pubkey: ScriptBuf::new(), // Would be actual payout script
        };

        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![coinbase_input],
            output: vec![coinbase_output],
        };

        Ok(coinbase_tx)
    }

    /// Calculate transaction weight (simplified)
    fn calculate_transaction_weight(&self, tx: &bitcoin::Transaction) -> u64 {
        // Simplified weight calculation
        // In reality, this would properly calculate witness and non-witness data
        let serialized = bitcoin::consensus::encode::serialize(tx);
        serialized.len() as u64 * 4 // Simplified: assume no witness data
    }

    /// Calculate total fees for transactions
    fn calculate_total_fees(&self, transactions: &[bitcoin::Transaction]) -> u64 {
        // Simplified fee calculation
        // In reality, this would calculate input values minus output values
        transactions.len() as u64 * 1000 // Simplified: 1000 sats per transaction
    }

    /// Calculate total sigops for transactions
    fn calculate_total_sigops(&self, transactions: &[bitcoin::Transaction]) -> u64 {
        // Simplified sigops calculation
        // In reality, this would count signature operations in scripts
        transactions.len() as u64 * 2 // Simplified: 2 sigops per transaction
    }

    /// Handle job negotiation fallback to standard templates
    pub async fn fallback_to_standard_template(&self) -> Result<WorkTemplate> {
        tracing::info!("Falling back to standard template due to job negotiation failure");
        
        // Clear any custom templates
        {
            let mut templates = self.custom_templates.write().await;
            templates.clear();
        }

        // Get standard template from upstream
        ModeHandler::get_work_template(self).await
    }

    /// Get job negotiation status
    pub async fn get_job_negotiation_status(&self) -> (bool, Option<String>, usize) {
        let enabled = self.job_negotiation_enabled;
        let token = self.job_negotiation_token.read().await.clone();
        let custom_templates_count = self.custom_templates.read().await.len();
        
        (enabled, token, custom_templates_count)
    }
}

#[async_trait]
impl crate::mode::ModeHandler for ClientModeHandler {
    /// Start the client mode handler
    async fn start(&self) -> Result<()> {
        tracing::info!("Starting client mode handler");
        // Client mode doesn't need special startup procedures
        Ok(())
    }

    /// Stop the client mode handler
    async fn stop(&self) -> Result<()> {
        tracing::info!("Stopping client mode handler");
        // Client mode doesn't need special shutdown procedures
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
        
        tracing::info!("New connection in client mode: {} ({})", conn.id, conn.address);
        
        // Send current work template if available
        if let Some(template) = self.current_template.read().await.as_ref() {
            self.distribute_work_template(template).await?;
        }
        
        Ok(())
    }

    /// Process a submitted share
    async fn process_share(&self, share: Share) -> Result<ShareResult> {
        // Submit share to upstream pool
        let result = self.submit_share_to_upstream(&share).await?;
        
        // Update local connection and worker statistics
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
            "Processed share from {} via upstream: {:?}",
            share_with_result.connection_id,
            result
        );
        
        Ok(result)
    }

    /// Get work template for miners
    async fn get_work_template(&self) -> Result<WorkTemplate> {
        // Try to get work from upstream first
        if let Ok(Some(new_template)) = self.receive_work_from_upstream().await {
            let mut current = self.current_template.write().await;
            *current = Some(new_template.clone());
            return Ok(new_template);
        }
        
        // Return current template if available
        if let Some(template) = self.current_template.read().await.as_ref() {
            if !template.is_expired() {
                return Ok(template.clone());
            }
        }
        
        Err(Error::Protocol("No work template available from upstream".to_string()))
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
        self.database.update_connection_status(connection_id, ConnectionState::Disconnected).await?;
        
        tracing::info!("Connection disconnected from client mode: {}", connection_id);
        
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
        if let crate::config::OperationModeConfig::Client(client_config) = &config.mode {
            if client_config.upstream_pool.url.is_empty() {
                return Err(Error::Config("Client mode requires upstream pool URL".to_string()));
            }
            
            if client_config.upstream_pool.username.is_empty() {
                return Err(Error::Config("Client mode requires upstream pool username".to_string()));
            }
            
            if client_config.reconnect_interval == 0 {
                return Err(Error::Config("Reconnect interval must be greater than 0".to_string()));
            }
            
            if client_config.max_reconnect_attempts == 0 {
                return Err(Error::Config("Max reconnect attempts must be greater than 0".to_string()));
            }
        } else {
            return Err(Error::Config("Invalid configuration for client mode".to_string()));
        }
        
        Ok(())
    }
}

impl Drop for ClientModeHandler {
    fn drop(&mut self) {
        // Clean shutdown of background tasks
        if let Ok(mut task_handle) = self.reconnect_task.try_lock() {
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
        config::{UpstreamPool},
        database::MockDatabaseOps,
        mode::ModeHandler,
        types::{Protocol},
    };
    use std::net::SocketAddr;
    use uuid::Uuid;

    fn create_test_client_config() -> ClientConfig {
        ClientConfig {
            upstream_pool: UpstreamPool {
                url: "stratum+tcp://pool.example.com:4444".to_string(),
                username: "test_worker".to_string(),
                password: "test_password".to_string(),
                priority: 1,
                weight: 1,
            },
            enable_job_negotiation: false,
            custom_template_enabled: false,
            reconnect_interval: 30,
            max_reconnect_attempts: 5,
        }
    }

    #[tokio::test]
    async fn test_client_mode_handler_creation() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config.clone(), database);
        
        assert_eq!(handler.config.upstream_pool.url, "stratum+tcp://pool.example.com:4444");
        assert_eq!(handler.config.upstream_pool.username, "test_worker");
        assert!(!handler.job_negotiation_enabled);
    }

    #[tokio::test]
    async fn test_client_mode_handler_with_job_negotiation() {
        let mut client_config = create_test_client_config();
        client_config.enable_job_negotiation = true;
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        assert!(handler.job_negotiation_enabled);
    }

    #[tokio::test]
    async fn test_connection_handling() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
        let connection = Connection::new(addr, Protocol::Sv2);
        let connection_id = connection.id;

        // Handle connection
        handler.handle_connection(connection).await.unwrap();

        // Verify connection is stored
        let connections = handler.connections.read().await;
        assert!(connections.contains_key(&connection_id));
        assert_eq!(connections[&connection_id].address, addr);
    }

    #[tokio::test]
    async fn test_share_processing_without_upstream() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        let connection_id = Uuid::new_v4();
        let share = Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0);

        // Process share (should fail without upstream connection)
        let result = handler.process_share(share).await;
        
        // Should return an error due to no upstream connection
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No upstream connection"));
    }

    #[tokio::test]
    async fn test_statistics_update() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Update statistics
        handler.update_statistics().await;
        
        let stats = handler.get_statistics().await.unwrap();
        assert_eq!(stats.hashrate, 0.0);
        assert_eq!(stats.acceptance_rate, 0.0);
        assert!(stats.uptime.as_secs() >= 0);
    }

    #[tokio::test]
    async fn test_upstream_status() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config.clone(), database);
        
        let status = handler.get_upstream_status().await;
        assert_eq!(status.url, client_config.upstream_pool.url);
        assert!(!status.connected);
        assert_eq!(status.shares_submitted, 0);
        assert_eq!(status.shares_accepted, 0);
        assert_eq!(status.shares_rejected, 0);
    }

    #[test]
    fn test_setup_connection_message_creation() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        let message = handler.create_setup_connection_message().unwrap();
        
        // Verify message structure (simplified validation)
        assert!(message.len() >= 36);
        assert_eq!(message[0], 0x01); // SetupConnection message type
        assert_eq!(message[1], 0x00);
    }

    #[test]
    fn test_share_submission_message_creation() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        let connection_id = Uuid::new_v4();
        let share = Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0);
        
        let message = handler.create_share_submission_message(&share).unwrap();
        
        // Verify message structure (simplified validation)
        assert!(message.len() >= 36);
        assert_eq!(message[0], 0x06); // SubmitSharesStandard message type
        assert_eq!(message[1], 0x00);
    }

    #[test]
    fn test_setup_response_validation() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Valid response
        let valid_response = vec![0x02, 0x00, 0x00, 0x10]; // SetupConnectionSuccess
        assert!(handler.validate_setup_response(&valid_response).unwrap());
        
        // Invalid response
        let invalid_response = vec![0x03, 0x00, 0x00, 0x10]; // Different message type
        assert!(!handler.validate_setup_response(&invalid_response).unwrap());
        
        // Too short response
        let short_response = vec![0x02];
        assert!(!handler.validate_setup_response(&short_response).unwrap());
    }

    #[test]
    fn test_share_response_parsing() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Success response
        let success_response = vec![0x07, 0x00, 0x00, 0x04]; // SubmitSharesSuccess
        let result = handler.parse_share_response(&success_response).unwrap();
        assert!(matches!(result, ShareResult::Valid));
        
        // Error response
        let error_response = vec![0x08, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x00, 
                                 b'E', b'r', b'r', b'o', b'r']; // SubmitSharesError with message
        let result = handler.parse_share_response(&error_response).unwrap();
        assert!(matches!(result, ShareResult::Invalid(_)));
        
        // Block response
        let block_response = vec![0x09, 0x00, 0x00, 0x20]; // NewTemplate (block found)
        let result = handler.parse_share_response(&block_response).unwrap();
        assert!(matches!(result, ShareResult::Block(_)));
    }

    #[test]
    fn test_config_validation() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config.clone(), database);
        
        let config = DaemonConfig {
            mode: crate::config::OperationModeConfig::Client(client_config),
            ..Default::default()
        };
        
        assert!(handler.validate_config(&config).is_ok());
        
        // Test invalid config - empty URL
        let invalid_config = DaemonConfig {
            mode: crate::config::OperationModeConfig::Client(ClientConfig {
                upstream_pool: UpstreamPool {
                    url: "".to_string(),
                    ..create_test_client_config().upstream_pool
                },
                ..create_test_client_config()
            }),
            ..Default::default()
        };
        
        assert!(handler.validate_config(&invalid_config).is_err());
        
        // Test invalid config - empty username
        let invalid_config2 = DaemonConfig {
            mode: crate::config::OperationModeConfig::Client(ClientConfig {
                upstream_pool: UpstreamPool {
                    username: "".to_string(),
                    ..create_test_client_config().upstream_pool
                },
                ..create_test_client_config()
            }),
            ..Default::default()
        };
        
        assert!(handler.validate_config(&invalid_config2).is_err());
        
        // Test invalid config - zero reconnect interval
        let invalid_config3 = DaemonConfig {
            mode: crate::config::OperationModeConfig::Client(ClientConfig {
                reconnect_interval: 0,
                ..create_test_client_config()
            }),
            ..Default::default()
        };
        
        assert!(handler.validate_config(&invalid_config3).is_err());
    }

    #[tokio::test]
    async fn test_miner_subscription() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Add a connection first
        let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
        let connection = Connection::new(addr, Protocol::Sv2);
        let connection_id = connection.id; // Use the same connection ID
        handler.handle_connection(connection).await.unwrap();
        
        // Handle miner subscription
        handler.handle_miner_subscription(connection_id, Some(2.0)).await.unwrap();
        
        // Verify worker is created
        let workers = handler.workers.read().await;
        assert!(workers.contains_key(&connection_id));
        assert_eq!(workers[&connection_id].difficulty, 2.0);
        
        // Verify connection info is updated
        let connections = handler.connections.read().await;
        if let Some(conn_info) = connections.get(&connection_id) {
            assert_eq!(conn_info.subscribed_difficulty, Some(2.0));
        }
    }

    #[tokio::test]
    async fn test_disconnection_handling() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
        let connection = Connection::new(addr, Protocol::Sv2);
        let connection_id = connection.id;

        // Handle connection
        handler.handle_connection(connection).await.unwrap();
        
        // Add worker
        handler.handle_miner_subscription(connection_id, Some(1.0)).await.unwrap();
        
        // Verify connection and worker exist
        {
            let connections = handler.connections.read().await;
            let workers = handler.workers.read().await;
            assert!(connections.contains_key(&connection_id));
            assert!(workers.contains_key(&connection_id));
        }
        
        // Handle disconnection
        handler.handle_disconnection(connection_id).await.unwrap();
        
        // Verify connection and worker are removed
        {
            let connections = handler.connections.read().await;
            let workers = handler.workers.read().await;
            assert!(!connections.contains_key(&connection_id));
            assert!(!workers.contains_key(&connection_id));
        }
    }

    #[tokio::test]
    async fn test_work_template_handling() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Try to get work template without upstream connection
        let result = handler.get_work_template().await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No work template available"));
    }

    #[test]
    fn test_job_negotiation_message_creation() {
        let mut client_config = create_test_client_config();
        client_config.enable_job_negotiation = true;
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        let message = handler.create_allocate_mining_job_token_message().unwrap();
        
        // Verify message structure (simplified validation)
        assert!(message.len() >= 20);
        assert_eq!(message[0], 0x50); // AllocateMiningJobToken message type
        assert_eq!(message[1], 0x00);
    }

    #[test]
    fn test_allocate_response_validation() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Valid response
        let valid_response = vec![0x51, 0x00, 0x00, 0x10]; // AllocateMiningJobTokenSuccess
        assert!(handler.validate_allocate_response(&valid_response).unwrap());
        
        // Invalid response
        let invalid_response = vec![0x52, 0x00, 0x00, 0x10]; // Different message type
        assert!(!handler.validate_allocate_response(&invalid_response).unwrap());
    }

    #[tokio::test]
    async fn test_complete_client_workflow() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());
        let handler = ClientModeHandler::new(client_config, database);

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

        // Subscribe miners
        for connection in &connections {
            handler.handle_miner_subscription(connection.id, Some(1.0)).await.unwrap();
        }

        // Verify workers are created
        {
            let workers = handler.workers.read().await;
            assert_eq!(workers.len(), 3);
        }

        // Handle disconnections
        for connection in &connections {
            handler.handle_disconnection(connection.id).await.unwrap();
        }

        // Verify connections and workers are removed
        {
            let stored_connections = handler.connections.read().await;
            let workers = handler.workers.read().await;
            assert_eq!(stored_connections.len(), 0);
            assert_eq!(workers.len(), 0);
        }
    }

    #[tokio::test]
    async fn test_job_negotiation_functionality() {
        let mut client_config = create_test_client_config();
        client_config.enable_job_negotiation = true;
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Test job negotiation status
        let (enabled, token, count) = handler.get_job_negotiation_status().await;
        assert!(enabled);
        assert!(token.is_none()); // No token initially
        assert_eq!(count, 0);
        
        // Simulate job negotiation setup
        handler.simulate_job_negotiation().await.unwrap();
        
        // Check status after setup
        let (enabled, token, count) = handler.get_job_negotiation_status().await;
        assert!(enabled);
        assert!(token.is_some()); // Token should be available now
        assert_eq!(count, 0); // No custom templates yet
    }

    #[tokio::test]
    async fn test_custom_template_creation() {
        let mut client_config = create_test_client_config();
        client_config.enable_job_negotiation = true;
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Set up a base template first
        use bitcoin::{BlockHash, Transaction, TxIn, TxOut, hashes::Hash};
        let base_template = WorkTemplate::new(
            BlockHash::all_zeros(),
            Transaction {
                version: 1,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![TxIn::default()],
                output: vec![TxOut::default()],
            },
            vec![],
            1.0,
        );
        
        {
            let mut current = handler.current_template.write().await;
            *current = Some(base_template);
        }
        
        // Create custom template with preferred transactions
        let preferred_txs = vec![];
        let result = handler.create_custom_template(preferred_txs).await;
        
        assert!(result.is_ok());
        let custom_template = result.unwrap();
        assert!(custom_template.weight > 0);
        assert!(custom_template.sigops >= 0);
        assert_eq!(custom_template.reward, 625_000_000);
    }

    #[tokio::test]
    async fn test_custom_template_validation() {
        let mut client_config = create_test_client_config();
        client_config.enable_job_negotiation = true;
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Create a valid block template
        use bitcoin::{BlockHash, Transaction, TxIn, TxOut, hashes::Hash};
        let template = WorkTemplate::new(
            BlockHash::all_zeros(),
            Transaction {
                version: 1,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![TxIn::default()],
                output: vec![TxOut::default()],
            },
            vec![],
            1.0,
        );
        
        let block_template = BlockTemplate {
            template,
            height: 800000,
            reward: 625_000_000,
            fees: 50000,
            weight: 1000000,
            sigops: 1000,
            min_time: chrono::Utc::now().timestamp() as u32 - 3600,
            max_time: chrono::Utc::now().timestamp() as u32 + 3600,
            mutable: vec!["time".to_string()],
            noncerange: "00000000ffffffff".to_string(),
            capabilities: vec!["proposal".to_string()],
        };
        
        // Valid template should pass
        let result = handler.validate_custom_template(&block_template).await;
        assert!(result.is_ok());
        
        // Invalid template (excessive weight) should fail
        let invalid_template = BlockTemplate {
            weight: 5_000_000, // Exceeds limit
            ..block_template
        };
        
        let result = handler.validate_custom_template(&invalid_template).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("weight exceeds"));
    }

    #[tokio::test]
    async fn test_propose_custom_template() {
        let mut client_config = create_test_client_config();
        client_config.enable_job_negotiation = true;
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Set up job negotiation first
        handler.simulate_job_negotiation().await.unwrap();
        
        // Create a valid block template
        use bitcoin::{BlockHash, Transaction, TxIn, TxOut, hashes::Hash};
        let template = WorkTemplate::new(
            BlockHash::all_zeros(),
            Transaction {
                version: 1,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![TxIn::default()],
                output: vec![TxOut::default()],
            },
            vec![],
            1.0,
        );
        
        let block_template = BlockTemplate {
            template,
            height: 800000,
            reward: 625_000_000,
            fees: 50000,
            weight: 1000000,
            sigops: 1000,
            min_time: chrono::Utc::now().timestamp() as u32 - 3600,
            max_time: chrono::Utc::now().timestamp() as u32 + 3600,
            mutable: vec!["time".to_string()],
            noncerange: "00000000ffffffff".to_string(),
            capabilities: vec!["proposal".to_string()],
        };
        
        // Propose custom template
        let result = handler.propose_custom_template(block_template).await;
        assert!(result.is_ok());
        
        let job_id = result.unwrap();
        assert!(job_id.starts_with("custom_"));
        
        // Check that template was stored
        let (_, _, count) = handler.get_job_negotiation_status().await;
        assert_eq!(count, 1);
    }

    #[test]
    fn test_declare_mining_job_message_creation() {
        let mut client_config = create_test_client_config();
        client_config.enable_job_negotiation = true;
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Create a block template
        use bitcoin::{BlockHash, Transaction, TxIn, TxOut, hashes::Hash};
        let template = WorkTemplate::new(
            BlockHash::all_zeros(),
            Transaction {
                version: 1,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![TxIn::default()],
                output: vec![TxOut::default()],
            },
            vec![],
            1.0,
        );
        
        let block_template = BlockTemplate {
            template,
            height: 800000,
            reward: 625_000_000,
            fees: 50000,
            weight: 1000000,
            sigops: 1000,
            min_time: chrono::Utc::now().timestamp() as u32 - 3600,
            max_time: chrono::Utc::now().timestamp() as u32 + 3600,
            mutable: vec!["time".to_string()],
            noncerange: "00000000ffffffff".to_string(),
            capabilities: vec!["proposal".to_string()],
        };
        
        let job_token = "test_token";
        let message = handler.create_declare_mining_job_message(&block_template, job_token).unwrap();
        
        // Verify message structure
        assert!(message.len() >= 132);
        assert_eq!(message[0], 0x52); // DeclareMiningJob message type
        assert_eq!(message[1], 0x00);
    }

    #[tokio::test]
    async fn test_declare_job_response_handling() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Test success response
        let success_response = vec![
            0x53, 0x00, 0x00, 0x20, // DeclareMiningJobSuccess
            0x08, 0x00, // Job ID length: 8
            b'j', b'o', b'b', b'_', b'1', b'2', b'3', b'4', // Job ID
        ];
        
        let result = handler.handle_declare_job_response(&success_response).await.unwrap();
        assert_eq!(result, Some("job_1234".to_string()));
        
        // Test error response
        let error_response = vec![
            0x54, 0x00, 0x00, 0x20, // DeclareMiningJobError
            0x00, 0x00, 0x00, 0x00, // Error code
            b'I', b'n', b'v', b'a', b'l', b'i', b'd', // Error message
        ];
        
        let result = handler.handle_declare_job_response(&error_response).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Job negotiation failed"));
        
        // Test unknown response
        let unknown_response = vec![0x99, 0x00, 0x00, 0x04];
        let result = handler.handle_declare_job_response(&unknown_response).await.unwrap();
        assert_eq!(result, None);
    }

    #[tokio::test]
    async fn test_fallback_to_standard_template() {
        let mut client_config = create_test_client_config();
        client_config.enable_job_negotiation = true;
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Add some custom templates first
        use bitcoin::{BlockHash, Transaction, TxIn, TxOut, hashes::Hash};
        let template = WorkTemplate::new(
            BlockHash::all_zeros(),
            Transaction {
                version: 1,
                lock_time: bitcoin::absolute::LockTime::ZERO,
                input: vec![TxIn::default()],
                output: vec![TxOut::default()],
            },
            vec![],
            1.0,
        );
        
        let block_template = BlockTemplate {
            template: template.clone(),
            height: 800000,
            reward: 625_000_000,
            fees: 50000,
            weight: 1000000,
            sigops: 1000,
            min_time: chrono::Utc::now().timestamp() as u32 - 3600,
            max_time: chrono::Utc::now().timestamp() as u32 + 3600,
            mutable: vec!["time".to_string()],
            noncerange: "00000000ffffffff".to_string(),
            capabilities: vec!["proposal".to_string()],
        };
        
        {
            let mut templates = handler.custom_templates.write().await;
            templates.insert(template.id, block_template);
        }
        
        // Verify we have custom templates
        let (_, _, count) = handler.get_job_negotiation_status().await;
        assert_eq!(count, 1);
        
        // Test fallback (will fail due to no upstream connection, but should clear templates)
        let result = handler.fallback_to_standard_template().await;
        assert!(result.is_err()); // Expected to fail without upstream
        
        // Verify custom templates were cleared
        let (_, _, count) = handler.get_job_negotiation_status().await;
        assert_eq!(count, 0);
    }

    #[test]
    fn test_transaction_calculations() {
        let client_config = create_test_client_config();
        let database = Arc::new(MockDatabaseOps::new());

        let handler = ClientModeHandler::new(client_config, database);
        
        // Create a simple transaction
        use bitcoin::{Transaction, TxIn, TxOut};
        let tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![TxIn::default()],
            output: vec![TxOut::default()],
        };
        
        // Test weight calculation
        let weight = handler.calculate_transaction_weight(&tx);
        assert!(weight > 0);
        
        // Test fee calculation
        let fees = handler.calculate_total_fees(&[tx.clone()]);
        assert_eq!(fees, 1000); // Simplified: 1000 sats per tx
        
        // Test sigops calculation
        let sigops = handler.calculate_total_sigops(&[tx]);
        assert_eq!(sigops, 2); // Simplified: 2 sigops per tx
    }

    #[test]
    fn test_url_parsing() {
        // Test valid URLs
        let valid_urls = vec![
            ("stratum+tcp://pool.example.com:4444", "pool.example.com", 4444),
            ("stratum+tcp://192.168.1.100:3333", "192.168.1.100", 3333),
            ("pool.test.com:8080", "pool.test.com", 8080),
            ("pool.example.com", "pool.example.com", 4444), // Default port
        ];

        for (url, expected_host, expected_port) in valid_urls {
            let result = ClientModeHandler::parse_stratum_url(url);
            assert!(result.is_ok(), "Failed to parse URL: {}", url);
            
            let (host, port) = result.unwrap();
            assert_eq!(host, expected_host);
            assert_eq!(port, expected_port);
        }

        // Test invalid URLs
        let invalid_urls = vec![
            "",
            "stratum+tcp://",
            "pool.example.com:invalid_port",
            ":4444", // Missing host
        ];

        for url in invalid_urls {
            let result = ClientModeHandler::parse_stratum_url(url);
            assert!(result.is_err(), "URL should be invalid: {}", url);
        }
    }
}