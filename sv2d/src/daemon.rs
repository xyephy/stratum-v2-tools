use sv2_core::{
    Daemon, DaemonConfig, DaemonStatus, Result, Error,
    database::{DatabasePool, DatabaseOps},
    mode_factory::ModeRouter,
    server::StratumServer,
    api_server::ApiServer,
    protocol::{NetworkProtocolMessage, StratumMessage},
    types::{DaemonStatus as CoreDaemonStatus, MiningStats, Connection, ConnectionId, Share, ShareResult},
};
use async_trait::async_trait;
use std::{time::{Duration, Instant}, sync::Arc, net::SocketAddr};
use tokio::sync::{RwLock, watch, mpsc};
use tokio::signal;
use tokio::net::TcpListener;
use axum::{
    routing::get,
    Router, Json, extract::State,
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn, error, debug};

/// Main sv2d daemon implementation
pub struct Sv2Daemon {
    start_time: Option<Instant>,
    is_running: bool,
    config: Arc<RwLock<Option<DaemonConfig>>>,
    database: Arc<RwLock<Option<DatabasePool>>>,
    mode_router: Arc<RwLock<Option<ModeRouter>>>,
    stratum_server: Option<StratumServer>,
    api_server: Option<ApiServer>,
    daemon_status: Arc<RwLock<CoreDaemonStatus>>,
    mining_stats: Arc<RwLock<MiningStats>>,
    shutdown_tx: Option<watch::Sender<bool>>,
    shutdown_rx: Option<watch::Receiver<bool>>,
    stats: Arc<RwLock<DaemonStats>>,
    api_server_handle: Option<tokio::task::JoinHandle<()>>,
    stratum_server_handle: Option<tokio::task::JoinHandle<()>>,
}

/// Internal daemon statistics
#[derive(Debug, Clone, Default)]
struct DaemonStats {
    connections: usize,
    total_shares: u64,
    valid_shares: u64,
    blocks_found: u64,
    current_difficulty: f64,
    hashrate: f64,
}

impl Sv2Daemon {
    pub fn new() -> Self {
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        
        Self {
            start_time: None,
            is_running: false,
            config: Arc::new(RwLock::new(None)),
            database: Arc::new(RwLock::new(None)),
            mode_router: Arc::new(RwLock::new(None)),
            stratum_server: None,
            api_server: None,
            daemon_status: Arc::new(RwLock::new(CoreDaemonStatus::default())),
            mining_stats: Arc::new(RwLock::new(MiningStats::default())),
            shutdown_tx: Some(shutdown_tx),
            shutdown_rx: Some(shutdown_rx),
            stats: Arc::new(RwLock::new(DaemonStats::default())),
            api_server_handle: None,
            stratum_server_handle: None,
        }
    }

    /// Initialize database connection
    async fn init_database(&self, config: &DaemonConfig) -> Result<DatabasePool> {
        info!("Initializing database connection");
        
        let database_url = &config.database.url;
        let max_connections = config.database.max_connections;
        
        let pool = DatabasePool::new(database_url, max_connections).await?;
        
        // Run migrations
        info!("Running database migrations");
        pool.migrate().await?;
        
        // Health check
        pool.health_check().await?;
        
        info!("Database initialized successfully");
        Ok(pool)
    }



    /// Start background tasks
    async fn start_background_tasks(&self) -> Result<()> {
        let shutdown_rx = self.shutdown_rx.as_ref().unwrap().clone();
        let stats = Arc::clone(&self.stats);
        let database = Arc::clone(&self.database);
        
        // Statistics collection task
        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_rx;
            let mut interval = tokio::time::interval(Duration::from_secs(30));
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::collect_stats(&stats, &database).await {
                            error!("Failed to collect statistics: {}", e);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Statistics collection task shutting down");
                            break;
                        }
                    }
                }
            }
        });

        // Database cleanup task
        let shutdown_rx = self.shutdown_rx.as_ref().unwrap().clone();
        let database = Arc::clone(&self.database);
        
        tokio::spawn(async move {
            let mut shutdown_rx = shutdown_rx;
            let mut interval = tokio::time::interval(Duration::from_secs(3600)); // Every hour
            
            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        if let Err(e) = Self::cleanup_database(&database).await {
                            error!("Failed to cleanup database: {}", e);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            debug!("Database cleanup task shutting down");
                            break;
                        }
                    }
                }
            }
        });

        Ok(())
    }

    /// Collect daemon statistics
    async fn collect_stats(stats: &Arc<RwLock<DaemonStats>>, database: &Arc<RwLock<Option<DatabasePool>>>) -> Result<()> {
        let db_guard = database.read().await;
        if let Some(db) = db_guard.as_ref() {
            let db_stats = db.get_stats().await?;
            let share_stats = db.get_share_stats(None).await?;
            
            let mut stats_guard = stats.write().await;
            stats_guard.connections = db_stats.total_connections as usize;
            stats_guard.total_shares = share_stats.total_shares;
            stats_guard.valid_shares = share_stats.valid_shares;
            stats_guard.blocks_found = share_stats.blocks_found;
            
            // Calculate hashrate (simplified)
            if let (Some(first), Some(last)) = (share_stats.first_share, share_stats.last_share) {
                let duration = (last - first).num_seconds() as f64;
                if duration > 0.0 {
                    stats_guard.hashrate = share_stats.total_shares as f64 / duration;
                }
            }
            
            debug!("Statistics updated: connections={}, shares={}, hashrate={:.2}", 
                   stats_guard.connections, stats_guard.total_shares, stats_guard.hashrate);
        }
        
        Ok(())
    }

    /// Cleanup old database entries
    async fn cleanup_database(database: &Arc<RwLock<Option<DatabasePool>>>) -> Result<()> {
        let db_guard = database.read().await;
        if let Some(db) = db_guard.as_ref() {
            let deleted = db.delete_expired_templates().await?;
            if deleted > 0 {
                info!("Cleaned up {} expired work templates", deleted);
            }
        }
        
        Ok(())
    }

    /// Handle configuration reload
    async fn handle_config_reload(&self, new_config: DaemonConfig) -> Result<()> {
        info!("Handling configuration reload");
        
        // Validate new configuration
        new_config.validate()?;
        
        let current_config = {
            let config_guard = self.config.read().await;
            config_guard.clone()
        };
        
        if let Some(current) = current_config {
            // Check if mode changed
            if std::mem::discriminant(&current.mode) != std::mem::discriminant(&new_config.mode) {
                warn!("Mode change detected, full restart required");
                return Err(Error::Config("Mode changes require daemon restart".to_string()));
            }
            
            // Check if database config changed
            if current.database != new_config.database {
                warn!("Database configuration change detected, full restart required");
                return Err(Error::Config("Database changes require daemon restart".to_string()));
            }
        }
        
        // Update configuration
        {
            let mut config_guard = self.config.write().await;
            *config_guard = Some(new_config.clone());
        }
        
        // Update mode router with new config
        {
            let mut router_guard = self.mode_router.write().await;
            if let Some(router) = router_guard.as_mut() {
                router.update_config(new_config.clone()).await?;
            } else {
                return Err(Error::System("Mode router not initialized".to_string()));
            }
        }
        
        info!("Configuration reloaded successfully");
        Ok(())
    }

    /// Setup signal handlers
    pub async fn setup_signal_handlers(&self) -> Result<()> {
        let shutdown_tx = self.shutdown_tx.as_ref().unwrap().clone();
        let config = Arc::clone(&self.config);
        
        // Handle Ctrl+C for graceful shutdown (cross-platform)
        tokio::spawn(async move {
            match signal::ctrl_c().await {
                Ok(()) => {
                    info!("Received Ctrl+C, initiating graceful shutdown");
                    let _ = shutdown_tx.send(true);
                }
                Err(err) => {
                    error!("Failed to listen for Ctrl+C: {}", err);
                }
            }
        });

        // Platform-specific signal handling for configuration reload
        #[cfg(unix)]
        {
            let config_clone = Arc::clone(&config);
            tokio::spawn(async move {
                let mut sighup = signal::unix::signal(signal::unix::SignalKind::hangup())
                    .expect("Failed to register SIGHUP handler");
                
                while sighup.recv().await.is_some() {
                    info!("Received SIGHUP, reloading configuration");
                    
                    // Try to reload configuration from file
                    // This is a simplified implementation - in practice you'd want to
                    // store the original config file path and reload from there
                    let mut new_config = DaemonConfig::default();
                    match new_config.merge_env() {
                        Ok(()) => {
                            let mut config_guard = config_clone.write().await;
                            *config_guard = Some(new_config);
                            info!("Configuration reloaded from environment");
                        }
                        Err(e) => {
                            error!("Failed to reload configuration: {}", e);
                        }
                    }
                }
            });
        }

        Ok(())
    }

    /// Start HTTP API server
    async fn start_api_server(&mut self, config: &DaemonConfig) -> Result<()> {
        let database = {
            let db_guard = self.database.read().await;
            db_guard.as_ref().ok_or_else(|| Error::System("Database not initialized".to_string()))?.clone()
        };

        // Use configured monitoring bind address for API server
        let api_bind_address = config.monitoring.metrics_bind_address;

        let api_server = ApiServer::new(
            api_bind_address,
            Arc::new(database),
            self.daemon_status.clone(),
            self.mining_stats.clone(),
        );

        let handle = tokio::spawn(async move {
            info!("Starting API server on {}", api_bind_address);
            if let Err(e) = api_server.start().await {
                error!("API server error: {}", e);
            } else {
                info!("API server stopped gracefully");
            }
        });

        self.api_server_handle = Some(handle);
        info!("API server started on http://{}", api_bind_address);
        Ok(())
    }

    /// Start Stratum server
    async fn start_stratum_server(&mut self, config: &DaemonConfig) -> Result<()> {
        let bind_address = config.network.bind_address;

        // Create message channel for protocol communication
        let (message_tx, mut message_rx) = mpsc::unbounded_channel::<NetworkProtocolMessage>();

        // Initialize Stratum server
        let mut stratum_server = StratumServer::new(bind_address, message_tx);

        // Start Stratum server in background task
        let server_handle = tokio::spawn(async move {
            info!("Starting Stratum server on {}", bind_address);
            if let Err(e) = stratum_server.start().await {
                error!("Stratum server error: {}", e);
            } else {
                info!("Stratum server stopped gracefully");
            }
        });

        // Start message processing loop
        let mode_router = Arc::clone(&self.mode_router);
        let daemon_status = self.daemon_status.clone();
        let mining_stats = self.mining_stats.clone();
        let database = Arc::clone(&self.database);
        let mut shutdown_rx = self.shutdown_rx.as_ref().unwrap().clone();

        let message_handle = tokio::spawn(async move {
            info!("Starting protocol message processor");
            loop {
                tokio::select! {
                    Some(message) = message_rx.recv() => {
                        // Process protocol messages
                        if let Err(e) = Self::process_protocol_message(
                            message,
                            &mode_router,
                            &daemon_status,
                            &mining_stats,
                            &database,
                        ).await {
                            error!("Error processing protocol message: {}", e);
                        }
                    }
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            info!("Shutting down message processor");
                            break;
                        }
                    }
                }
            }
        });

        self.stratum_server_handle = Some(server_handle);
        
        // Store message processor handle for cleanup
        // For now, we'll let it run until shutdown
        tokio::spawn(message_handle);
        
        info!("Stratum server started on {}", bind_address);
        Ok(())
    }

    /// Process incoming protocol messages
    async fn process_protocol_message(
        message: NetworkProtocolMessage,
        mode_router: &Arc<RwLock<Option<ModeRouter>>>,
        daemon_status: &Arc<RwLock<CoreDaemonStatus>>,
        mining_stats: &Arc<RwLock<MiningStats>>,
        database: &Arc<RwLock<Option<DatabasePool>>>,
    ) -> Result<()> {
        match message {
            NetworkProtocolMessage::Connect { connection_id, peer_addr, protocol } => {
                info!("New connection: {} from {} using {:?}", connection_id, peer_addr, protocol);
                
                // Create connection object with the correct ID
                let mut connection = Connection::new(peer_addr, protocol);
                connection.id = connection_id; // Set the correct connection ID
                
                // Forward to mode handler
                {
                    let router_guard = mode_router.read().await;
                    if let Some(router) = router_guard.as_ref() {
                        if let Some(handler) = router.get_handler() {
                            if let Err(e) = handler.handle_connection(connection).await {
                                error!("Mode handler failed to handle connection {}: {}", connection_id, e);
                            } else {
                                // Send initial work template if available
                                if let Ok(template) = handler.get_work_template().await {
                                    if let Err(e) = Self::send_work_template(connection_id, &template).await {
                                        error!("Failed to send initial work template to {}: {}", connection_id, e);
                                    }
                                }
                            }
                        }
                    }
                }
                
                // Update daemon status
                {
                    let mut status = daemon_status.write().await;
                    status.active_connections += 1;
                    status.total_connections += 1;
                }
            }
            NetworkProtocolMessage::Disconnect { connection_id, reason } => {
                info!("Connection disconnected: {} ({})", connection_id, reason);
                
                // Forward to mode handler
                {
                    let router_guard = mode_router.read().await;
                    if let Some(router) = router_guard.as_ref() {
                        if let Some(handler) = router.get_handler() {
                            if let Err(e) = handler.handle_disconnection(connection_id).await {
                                error!("Mode handler failed to handle disconnection {}: {}", connection_id, e);
                            }
                        }
                    }
                }
                
                // Update daemon status
                {
                    let mut status = daemon_status.write().await;
                    status.active_connections = status.active_connections.saturating_sub(1);
                }
            }
            NetworkProtocolMessage::StratumV1 { connection_id, message } => {
                debug!("Stratum V1 message from {}: {:?}", connection_id, message);
                
                // Handle specific Stratum V1 messages
                if let Err(e) = Self::handle_stratum_v1_message(
                    connection_id,
                    message,
                    mode_router,
                    database,
                    mining_stats,
                ).await {
                    error!("Failed to handle Stratum V1 message from {}: {}", connection_id, e);
                }
            }
            NetworkProtocolMessage::StratumV2 { connection_id, data } => {
                debug!("Stratum V2 message from {}: {} bytes", connection_id, data.len());
                
                // Handle Stratum V2 messages
                if let Err(e) = Self::handle_stratum_v2_message(
                    connection_id,
                    data,
                    mode_router,
                    database,
                ).await {
                    error!("Failed to handle Stratum V2 message from {}: {}", connection_id, e);
                }
            }
            NetworkProtocolMessage::SendResponse { connection_id, response } => {
                debug!("Sending response to {}: {}", connection_id, response);
                // This would be handled by the server's response mechanism
                // For now, we'll log it as the server handles responses directly
            }
            NetworkProtocolMessage::SendWork { connection_id, work_template } => {
                debug!("Sending work template to {}: {}", connection_id, work_template.id);
                if let Err(e) = Self::send_work_template(connection_id, &work_template).await {
                    error!("Failed to send work template to {}: {}", connection_id, e);
                }
            }
        }
        Ok(())
    }

    /// Handle Stratum V1 protocol messages
    async fn handle_stratum_v1_message(
        connection_id: ConnectionId,
        message: StratumMessage,
        mode_router: &Arc<RwLock<Option<ModeRouter>>>,
        database: &Arc<RwLock<Option<DatabasePool>>>,
        mining_stats: &Arc<RwLock<MiningStats>>,
    ) -> Result<()> {
        if let Some(method) = &message.method {
            match method.as_str() {
                "mining.subscribe" => {
                    info!("Mining subscription from {}", connection_id);
                    // Subscription is already handled by the server with immediate response
                    // Here we can do additional setup if needed
                }
                "mining.authorize" => {
                    info!("Mining authorization from {}", connection_id);
                    // Authorization is already handled by the server with immediate response
                    // Here we can do additional validation if needed
                    
                    // Send initial work template after authorization
                    {
                        let router_guard = mode_router.read().await;
                        if let Some(router) = router_guard.as_ref() {
                            if let Some(handler) = router.get_handler() {
                                if let Ok(template) = handler.get_work_template().await {
                                    if let Err(e) = Self::send_work_template(connection_id, &template).await {
                                        error!("Failed to send work template after authorization: {}", e);
                                    }
                                }
                            }
                        }
                    }
                }
                "mining.submit" => {
                    info!("Share submission from {}", connection_id);
                    
                    // Parse share submission
                    if let Some(params) = &message.params {
                        if let Err(e) = Self::process_share_submission(
                            connection_id,
                            params,
                            mode_router,
                            database,
                            mining_stats,
                        ).await {
                            error!("Failed to process share submission: {}", e);
                        }
                    }
                }
                _ => {
                    warn!("Unknown Stratum V1 method: {}", method);
                }
            }
        }
        Ok(())
    }

    /// Handle Stratum V2 protocol messages
    async fn handle_stratum_v2_message(
        connection_id: ConnectionId,
        _data: Vec<u8>,
        _mode_router: &Arc<RwLock<Option<ModeRouter>>>,
        _database: &Arc<RwLock<Option<DatabasePool>>>,
    ) -> Result<()> {
        // For now, just log SV2 messages
        // In a full implementation, this would parse SV2 binary protocol
        debug!("Received Stratum V2 message from {}", connection_id);
        Ok(())
    }

    /// Process share submission from miner
    async fn process_share_submission(
        connection_id: ConnectionId,
        params: &serde_json::Value,
        mode_router: &Arc<RwLock<Option<ModeRouter>>>,
        database: &Arc<RwLock<Option<DatabasePool>>>,
        mining_stats: &Arc<RwLock<MiningStats>>,
    ) -> Result<()> {
        // Parse share parameters (worker_name, job_id, extranonce2, ntime, nonce)
        if let Some(params_array) = params.as_array() {
            if params_array.len() >= 5 {
                let _worker_name = params_array[0].as_str().unwrap_or("unknown");
                let _job_id = params_array[1].as_str().unwrap_or("0");
                let _extranonce2 = params_array[2].as_str().unwrap_or("00000000");
                let ntime_str = params_array[3].as_str().unwrap_or("00000000");
                let nonce_str = params_array[4].as_str().unwrap_or("00000000");

                // Parse nonce and ntime
                let nonce = u32::from_str_radix(nonce_str, 16)
                    .map_err(|e| Error::Protocol(format!("Invalid nonce: {}", e)))?;
                let ntime = u32::from_str_radix(ntime_str, 16)
                    .map_err(|e| Error::Protocol(format!("Invalid ntime: {}", e)))?;

                // Create share object
                let share = Share::new(connection_id, nonce, ntime, 1.0);

                // Forward to mode handler for processing
                {
                    let router_guard = mode_router.read().await;
                    if let Some(router) = router_guard.as_ref() {
                        if let Some(handler) = router.get_handler() {
                            match handler.process_share(share).await {
                                Ok(ShareResult::Valid) => {
                                    info!("Valid share accepted from {}", connection_id);
                                    // Update mining stats
                                    {
                                        let mut stats = mining_stats.write().await;
                                        stats.shares_accepted += 1;
                                    }
                                }
                                Ok(ShareResult::Block(block_hash)) => {
                                    info!("BLOCK FOUND! Hash: {} from {}", block_hash, connection_id);
                                    // Update mining stats
                                    {
                                        let mut stats = mining_stats.write().await;
                                        stats.shares_accepted += 1;
                                        stats.blocks_found += 1;
                                    }
                                }
                                Ok(ShareResult::Invalid(reason)) => {
                                    warn!("Invalid share from {}: {}", connection_id, reason);
                                    // Update mining stats
                                    {
                                        let mut stats = mining_stats.write().await;
                                        stats.shares_rejected += 1;
                                    }
                                }
                                Err(e) => {
                                    error!("Error processing share from {}: {}", connection_id, e);
                                    // Update mining stats
                                    {
                                        let mut stats = mining_stats.write().await;
                                        stats.shares_rejected += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }

    /// Send work template to a miner
    async fn send_work_template(connection_id: ConnectionId, template: &sv2_core::WorkTemplate) -> Result<()> {
        // Convert work template to Stratum V1 mining.notify message
        let job_id = format!("{:x}", template.id.as_u128());
        let prevhash = template.previous_hash.to_string();
        let coinb1 = "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff08044c86041b020602ffffffff0100f2052a010000004341041b0e8c2567c12536aa13357b79a073dc4444acb83c4ec7a0e2f99dd7457516c5817242da796924ca4e99947d087fedf9ce467cb9f7c6287078f801df276fdf84ac00000000";
        let coinb2 = "00000000";
        let merkle_branch: Vec<String> = vec![]; // Simplified - would contain actual merkle branch
        let version = "20000000";
        let nbits = format!("{:08x}", 0x207fffff); // Simplified difficulty
        let ntime = format!("{:08x}", template.timestamp);
        let clean_jobs = true;

        let notify_msg = serde_json::json!({
            "id": null,
            "method": "mining.notify",
            "params": [
                job_id,
                prevhash,
                coinb1,
                coinb2,
                merkle_branch,
                version,
                nbits,
                ntime,
                clean_jobs
            ]
        });

        debug!("Sending work template to {}: {}", connection_id, notify_msg);
        
        // In a real implementation, this would send the message through the server
        // For now, we'll just log it as the server handles direct responses
        Ok(())
    }
}

#[async_trait]
impl Daemon for Sv2Daemon {
    async fn start(&mut self, config: DaemonConfig) -> Result<()> {
        // Initialize logging first
        sv2_core::init_logging(&config.logging)
            .map_err(|e| Error::System(format!("Failed to initialize logging: {}", e)))?;
        
        info!("Starting sv2d daemon in {} mode", config.get_mode_type());
        
        // Validate configuration
        config.validate()?;
        
        // Initialize database
        let database = self.init_database(&config).await?;
        let database_arc = Arc::new(database);
        
        // Store database reference
        {
            let mut db_guard = self.database.write().await;
            *db_guard = Some(database_arc.as_ref().clone());
        }
        
        // Create and initialize mode router
        let mut mode_router = ModeRouter::new(Arc::clone(&database_arc));
        mode_router.initialize(config.clone()).await?;
        
        // Store mode router
        {
            let mut router_guard = self.mode_router.write().await;
            *router_guard = Some(mode_router);
        }
        
        // Store configuration
        {
            let mut config_guard = self.config.write().await;
            *config_guard = Some(config.clone());
        }
        
        // Setup signal handlers
        self.setup_signal_handlers().await?;
        
        // Start background tasks
        self.start_background_tasks().await?;
        
        // Start API server
        self.start_api_server(&config).await?;
        
        // Start Stratum server
        self.start_stratum_server(&config).await?;
        
        // Mode router is already started during initialization
        
        self.start_time = Some(Instant::now());
        self.is_running = true;
        
        info!("sv2d daemon started successfully");
        Ok(())
    }

    async fn stop(&mut self) -> Result<()> {
        info!("Stopping sv2d daemon");
        
        if !self.is_running {
            warn!("Daemon is not running");
            return Ok(());
        }
        
        // Signal shutdown to background tasks
        if let Some(shutdown_tx) = &self.shutdown_tx {
            let _ = shutdown_tx.send(true);
        }
        
        // Stop API server
        if let Some(handle) = self.api_server_handle.take() {
            handle.abort();
            info!("API server stopped");
        }
        
        // Stop Stratum server
        if let Some(handle) = self.stratum_server_handle.take() {
            handle.abort();
            info!("Stratum server stopped");
        }
        
        // Stop mode router
        {
            let mut router_guard = self.mode_router.write().await;
            if let Some(mut router) = router_guard.take() {
                info!("Stopping mode router");
                router.shutdown().await?;
            }
        }
        
        // Close database connections
        {
            let mut db_guard = self.database.write().await;
            if db_guard.take().is_some() {
                info!("Closing database connections");
            }
        }
        
        self.is_running = false;
        
        info!("sv2d daemon stopped gracefully");
        Ok(())
    }

    async fn reload_config(&mut self, config: DaemonConfig) -> Result<()> {
        info!("Reloading configuration");
        
        if !self.is_running {
            return Err(Error::System("Cannot reload config when daemon is not running".to_string()));
        }
        
        self.handle_config_reload(config).await?;
        
        info!("Configuration reloaded successfully");
        Ok(())
    }

    fn get_status(&self) -> DaemonStatus {
        // This is a blocking call, so we need to use try_read
        let stats = self.stats.try_read()
            .map(|guard| guard.clone())
            .unwrap_or_default();
        
        DaemonStatus {
            running: self.is_running,
            uptime: self.uptime(),
            active_connections: stats.connections as u64,
            total_connections: stats.connections as u64, // TODO: Track separate total
            mode: "Solo".to_string(), // TODO: Get actual mode from config
            version: env!("CARGO_PKG_VERSION").to_string(),
            total_shares: stats.total_shares,
            valid_shares: stats.valid_shares,
            blocks_found: stats.blocks_found,
            current_difficulty: stats.current_difficulty,
            hashrate: stats.hashrate,
        }
    }

    fn is_running(&self) -> bool {
        self.is_running
    }

    fn uptime(&self) -> Duration {
        self.start_time
            .map(|start| start.elapsed())
            .unwrap_or_default()
    }
}

impl Sv2Daemon {
    /// Run the daemon until shutdown signal is received
    pub async fn run_until_shutdown(&mut self) -> Result<()> {
        if let Some(mut shutdown_rx) = self.shutdown_rx.take() {
            shutdown_rx.changed().await.map_err(|e| Error::System(format!("Shutdown signal error: {}", e)))?;
            
            if *shutdown_rx.borrow() {
                info!("Shutdown signal received");
                self.stop().await?;
            }
        }
        
        Ok(())
    }
}
// API Server Implementation

#[derive(Clone)]
struct ApiState {
    stats: Arc<RwLock<DaemonStats>>,
    database: Arc<RwLock<Option<DatabasePool>>>,
    mode_router: Arc<RwLock<Option<ModeRouter>>>,
}



#[derive(Serialize)]
struct ApiStats {
    connections: usize,
    total_shares: u64,
    valid_shares: u64,
    blocks_found: u64,
    current_difficulty: f64,
    hashrate: f64,
}

#[derive(Serialize)]
struct ApiConnection {
    id: String,
    address: String,
    protocol: String,
    connected_at: u64,
}

async fn get_status(State(state): State<ApiState>) -> std::result::Result<Json<DaemonStatus>, StatusCode> {
    let stats = state.stats.read().await;
    
    let status = DaemonStatus {
        running: true,
        uptime: std::time::Duration::from_secs(0), // Would calculate from start_time
        active_connections: stats.connections as u64,
        total_connections: stats.connections as u64, // TODO: Track separate total
        mode: "Solo".to_string(), // TODO: Get actual mode from config
        version: env!("CARGO_PKG_VERSION").to_string(),
        total_shares: stats.total_shares,
        valid_shares: stats.valid_shares,
        blocks_found: stats.blocks_found,
        current_difficulty: stats.current_difficulty,
        hashrate: stats.hashrate,
    };
    
    Ok(Json(status))
}

async fn get_stats(State(state): State<ApiState>) -> std::result::Result<Json<ApiStats>, StatusCode> {
    let stats = state.stats.read().await;
    
    let api_stats = ApiStats {
        connections: stats.connections,
        total_shares: stats.total_shares,
        valid_shares: stats.valid_shares,
        blocks_found: stats.blocks_found,
        current_difficulty: stats.current_difficulty,
        hashrate: stats.hashrate,
    };
    
    Ok(Json(api_stats))
}

async fn get_connections(State(_state): State<ApiState>) -> std::result::Result<Json<Vec<ApiConnection>>, StatusCode> {
    // For now, return empty list - would query database in real implementation
    Ok(Json(vec![]))
}

async fn health_check() -> std::result::Result<Json<serde_json::Value>, StatusCode> {
    Ok(Json(serde_json::json!({
        "status": "healthy",
        "timestamp": chrono::Utc::now().timestamp()
    })))
}

// Stratum Connection Handler

use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn handle_stratum_connection(
    stream: TcpStream,
    addr: SocketAddr,
    mode_router: Arc<RwLock<Option<ModeRouter>>>,
) -> Result<()> {
    let (reader, mut writer) = stream.into_split();
    let mut reader = BufReader::new(reader);
    let mut line = String::new();
    
    info!("Handling Stratum connection from {}", addr);
    
    // Send initial response
    let response = r#"{"id":null,"result":[[["mining.set_difficulty","1"],["mining.notify","1"]],"00000000",4],"error":null}"#;
    writer.write_all(format!("{}\n", response).as_bytes()).await
        .map_err(|e| Error::System(format!("Failed to write to connection: {}", e)))?;
    
    loop {
        line.clear();
        match reader.read_line(&mut line).await {
            Ok(0) => {
                info!("Connection {} closed", addr);
                break;
            }
            Ok(_) => {
                let line = line.trim();
                if !line.is_empty() {
                    debug!("Received from {}: {}", addr, line);
                    
                    // Parse and handle Stratum message
                    if let Err(e) = handle_stratum_message(line, &mut writer, &mode_router).await {
                        error!("Error handling message from {}: {}", addr, e);
                    }
                }
            }
            Err(e) => {
                error!("Error reading from {}: {}", addr, e);
                break;
            }
        }
    }
    
    Ok(())
}

async fn handle_stratum_message(
    message: &str,
    writer: &mut tokio::net::tcp::OwnedWriteHalf,
    _mode_router: &Arc<RwLock<Option<ModeRouter>>>,
) -> Result<()> {
    // Parse JSON-RPC message
    let parsed: serde_json::Value = serde_json::from_str(message)
        .map_err(|e| Error::System(format!("Invalid JSON: {}", e)))?;
    
    if let Some(method) = parsed.get("method").and_then(|m| m.as_str()) {
        let id = parsed.get("id");
        
        match method {
            "mining.subscribe" => {
                let response = serde_json::json!({
                    "id": id,
                    "result": [
                        [["mining.set_difficulty", "1"], ["mining.notify", "1"]],
                        "00000000",
                        4
                    ],
                    "error": null
                });
                writer.write_all(format!("{}\n", response).as_bytes()).await
                    .map_err(|e| Error::System(format!("Failed to write response: {}", e)))?;
            }
            "mining.authorize" => {
                let response = serde_json::json!({
                    "id": id,
                    "result": true,
                    "error": null
                });
                writer.write_all(format!("{}\n", response).as_bytes()).await
                    .map_err(|e| Error::System(format!("Failed to write response: {}", e)))?;
            }
            "mining.submit" => {
                // Handle share submission
                let response = serde_json::json!({
                    "id": id,
                    "result": true,
                    "error": null
                });
                writer.write_all(format!("{}\n", response).as_bytes()).await
                    .map_err(|e| Error::System(format!("Failed to write response: {}", e)))?;
                
                info!("Share submitted and accepted");
            }
            _ => {
                warn!("Unknown method: {}", method);
                let response = serde_json::json!({
                    "id": id,
                    "result": null,
                    "error": {"code": -1, "message": "Unknown method"}
                });
                writer.write_all(format!("{}\n", response).as_bytes()).await
                    .map_err(|e| Error::System(format!("Failed to write response: {}", e)))?;
            }
        }
    }
    
    Ok(())
}