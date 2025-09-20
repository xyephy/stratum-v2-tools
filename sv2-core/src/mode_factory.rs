use crate::{
    Result, Error,
    mode::ModeHandler,
    config::{DaemonConfig, OperationModeConfig},
    modes::{SoloModeHandler, PoolModeHandler, ProxyModeHandler, ClientModeHandler},
    database::{DatabasePool, DatabaseOps},
    bitcoin_rpc::BitcoinRpcClient,
};
use std::sync::Arc;
use tracing::{info, warn, error};

/// Factory for creating mode handlers
pub struct ModeHandlerFactory;

impl ModeHandlerFactory {
    /// Create a new mode handler based on configuration
    pub fn create_handler(
        config: &DaemonConfig,
        database: Arc<DatabasePool>,
    ) -> Result<Box<dyn ModeHandler>> {
        info!("Creating mode handler for {} mode", config.mode);
        
        let handler: Box<dyn ModeHandler> = match &config.mode {
            OperationModeConfig::Solo(solo_config) => {
                let bitcoin_client = BitcoinRpcClient::new(config.bitcoin.clone());
                Box::new(SoloModeHandler::new(solo_config.clone(), bitcoin_client, database))
            }
            OperationModeConfig::Pool(pool_config) => {
                let bitcoin_client = BitcoinRpcClient::new(config.bitcoin.clone());
                Box::new(PoolModeHandler::new(pool_config.clone(), bitcoin_client, database))
            }
            OperationModeConfig::Proxy(proxy_config) => {
                Box::new(ProxyModeHandler::new(proxy_config.clone(), database))
            }
            OperationModeConfig::Client(client_config) => {
                Box::new(ClientModeHandler::new(client_config.clone(), database))
            }
        };
        
        info!("Mode handler created successfully");
        Ok(handler)
    }

    /// Validate that a mode switch is allowed
    pub fn validate_mode_switch(
        current_config: &DaemonConfig,
        new_config: &DaemonConfig,
    ) -> Result<()> {
        // Check if mode is actually changing
        if std::mem::discriminant(&current_config.mode) == std::mem::discriminant(&new_config.mode) {
            info!("Mode not changing, switch allowed");
            return Ok(());
        }

        // Get mode types for comparison
        let current_mode = current_config.get_mode_type();
        let new_mode = new_config.get_mode_type();
        
        warn!("Mode switch requested: {} -> {}", current_mode, new_mode);

        // Define allowed mode transitions
        let allowed_transitions = [
            // Solo can switch to Pool (both use Bitcoin node)
            (crate::mode::OperationMode::Solo, crate::mode::OperationMode::Pool),
            (crate::mode::OperationMode::Pool, crate::mode::OperationMode::Solo),
            
            // Proxy can switch to Client (both are intermediary modes)
            (crate::mode::OperationMode::Proxy, crate::mode::OperationMode::Client),
            (crate::mode::OperationMode::Client, crate::mode::OperationMode::Proxy),
        ];

        let transition = (current_mode, new_mode);
        if allowed_transitions.contains(&transition) {
            info!("Mode transition allowed: {} -> {}", transition.0, transition.1);
            Ok(())
        } else {
            error!("Mode transition not allowed: {} -> {}", transition.0, transition.1);
            Err(Error::Config(format!(
                "Mode transition from {} to {} requires daemon restart",
                current_mode, new_mode
            )))
        }
    }

    /// Check if configurations are compatible for hot-swapping
    pub fn validate_config_compatibility(
        current_config: &DaemonConfig,
        new_config: &DaemonConfig,
    ) -> Result<()> {
        // Database configuration must remain the same
        if current_config.database != new_config.database {
            return Err(Error::Config(
                "Database configuration changes require daemon restart".to_string()
            ));
        }

        // Network configuration changes that affect binding require restart
        if current_config.network.bind_address != new_config.network.bind_address {
            return Err(Error::Config(
                "Bind address changes require daemon restart".to_string()
            ));
        }

        // Bitcoin RPC configuration can change for some modes
        match (&current_config.mode, &new_config.mode) {
            (OperationModeConfig::Solo(_), OperationModeConfig::Solo(_)) |
            (OperationModeConfig::Pool(_), OperationModeConfig::Pool(_)) |
            (OperationModeConfig::Solo(_), OperationModeConfig::Pool(_)) |
            (OperationModeConfig::Pool(_), OperationModeConfig::Solo(_)) => {
                // Bitcoin RPC changes are allowed for mining modes
                if current_config.bitcoin != new_config.bitcoin {
                    warn!("Bitcoin RPC configuration changed, will recreate connection");
                }
            }
            _ => {
                // For other modes, Bitcoin config shouldn't matter much
            }
        }

        Ok(())
    }

    /// Preserve state during mode switching
    pub async fn preserve_mode_state(
        current_handler: &dyn ModeHandler,
        database: Arc<DatabasePool>,
    ) -> Result<ModeState> {
        info!("Preserving mode state for transition");
        
        // Get current statistics
        let stats = current_handler.get_statistics().await?;
        
        // Get active connections from database
        let connections = database.list_connections(None).await?;
        
        // Get recent shares for context
        let recent_shares = database.get_shares(None, Some(100)).await?;
        
        Ok(ModeState {
            statistics: stats,
            active_connections: connections,
            recent_shares,
            preserved_at: chrono::Utc::now(),
        })
    }

    /// Restore state after mode switching
    pub async fn restore_mode_state(
        _new_handler: &dyn ModeHandler,
        state: ModeState,
        _database: Arc<DatabasePool>,
    ) -> Result<()> {
        info!("Restoring mode state after transition");
        
        // Log the transition
        info!(
            "Mode transition completed. Preserved {} connections, {} recent shares",
            state.active_connections.len(),
            state.recent_shares.len()
        );
        
        // The new handler will naturally pick up existing connections from the database
        // No explicit restoration needed as the database maintains continuity
        
        Ok(())
    }
}

/// State preserved during mode transitions
#[derive(Debug, Clone)]
pub struct ModeState {
    pub statistics: crate::MiningStats,
    pub active_connections: Vec<crate::ConnectionInfo>,
    pub recent_shares: Vec<crate::Share>,
    pub preserved_at: chrono::DateTime<chrono::Utc>,
}

/// Mode router for handling different operational modes
pub struct ModeRouter {
    current_handler: Option<Box<dyn ModeHandler>>,
    database: Arc<DatabasePool>,
    config: Option<DaemonConfig>,
}

impl ModeRouter {
    /// Create a new mode router
    pub fn new(database: Arc<DatabasePool>) -> Self {
        Self {
            current_handler: None,
            database,
            config: None,
        }
    }

    /// Initialize with a configuration
    pub async fn initialize(&mut self, config: DaemonConfig) -> Result<()> {
        info!("Initializing mode router with {} mode", config.mode);
        
        let handler = ModeHandlerFactory::create_handler(&config, Arc::clone(&self.database))?;
        handler.start().await?;
        
        self.current_handler = Some(handler);
        self.config = Some(config);
        
        info!("Mode router initialized successfully");
        Ok(())
    }

    /// Switch to a new mode with the given configuration
    pub async fn switch_mode(&mut self, new_config: DaemonConfig) -> Result<()> {
        let current_config = self.config.as_ref()
            .ok_or_else(|| Error::System("Mode router not initialized".to_string()))?;

        info!("Switching mode from {} to {}", current_config.mode, new_config.mode);

        // Validate the mode switch
        ModeHandlerFactory::validate_mode_switch(current_config, &new_config)?;
        ModeHandlerFactory::validate_config_compatibility(current_config, &new_config)?;

        // Preserve current state if we have a handler
        let preserved_state = if let Some(current_handler) = &self.current_handler {
            Some(ModeHandlerFactory::preserve_mode_state(
                current_handler.as_ref(),
                Arc::clone(&self.database)
            ).await?)
        } else {
            None
        };

        // Stop current handler
        if let Some(current_handler) = self.current_handler.take() {
            info!("Stopping current mode handler");
            current_handler.stop().await?;
        }

        // Create and start new handler
        info!("Creating new mode handler");
        let new_handler = ModeHandlerFactory::create_handler(&new_config, Arc::clone(&self.database))?;
        new_handler.start().await?;

        // Restore state if we had any
        if let Some(state) = preserved_state {
            ModeHandlerFactory::restore_mode_state(
                new_handler.as_ref(),
                state,
                Arc::clone(&self.database)
            ).await?;
        }

        // Update router state
        self.current_handler = Some(new_handler);
        self.config = Some(new_config);

        info!("Mode switch completed successfully");
        Ok(())
    }

    /// Update configuration without changing mode
    pub async fn update_config(&mut self, new_config: DaemonConfig) -> Result<()> {
        let current_config = self.config.as_ref()
            .ok_or_else(|| Error::System("Mode router not initialized".to_string()))?;

        // Check if this is actually a mode change
        if std::mem::discriminant(&current_config.mode) != std::mem::discriminant(&new_config.mode) {
            return self.switch_mode(new_config).await;
        }

        info!("Updating configuration for current mode");

        // Validate compatibility
        ModeHandlerFactory::validate_config_compatibility(current_config, &new_config)?;

        // For now, we'll recreate the handler with new config
        // In a more sophisticated implementation, we might support
        // hot-reloading of certain configuration parameters
        self.switch_mode(new_config).await
    }

    /// Get the current mode handler
    pub fn get_handler(&self) -> Option<&dyn ModeHandler> {
        self.current_handler.as_ref().map(|h| h.as_ref())
    }

    /// Get the current configuration
    pub fn get_config(&self) -> Option<&DaemonConfig> {
        self.config.as_ref()
    }

    /// Shutdown the router
    pub async fn shutdown(&mut self) -> Result<()> {
        info!("Shutting down mode router");
        
        if let Some(handler) = self.current_handler.take() {
            handler.stop().await?;
        }
        
        self.config = None;
        
        info!("Mode router shutdown complete");
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{SoloConfig, PoolConfig, NetworkConfig, DatabaseConfig, BitcoinConfig, MonitoringConfig};
    use tempfile::tempdir;

    fn create_test_database_config() -> DatabaseConfig {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        DatabaseConfig {
            url: format!("sqlite://{}", db_path.display()),
            max_connections: 5,
            connection_timeout: 30,
            enable_migrations: true,
        }
    }

    fn create_test_config(mode: OperationModeConfig) -> DaemonConfig {
        DaemonConfig {
            mode,
            network: NetworkConfig {
                bind_address: "127.0.0.1:0".parse().unwrap(),
                max_connections: 100,
                connection_timeout: 30,
                keepalive_interval: 60,
            },
            bitcoin: BitcoinConfig {
                rpc_url: "http://localhost:18443".to_string(),
                rpc_user: "test".to_string(),
                rpc_password: "test".to_string(),
                network: crate::config::BitcoinNetwork::Regtest,
                coinbase_address: None,
                block_template_timeout: 30,
            },
            database: create_test_database_config(),
            monitoring: MonitoringConfig {
                enable_metrics: true,
                metrics_bind_address: "127.0.0.1:0".parse().unwrap(),
                enable_health_checks: true,
                health_check_interval: 30,
                metrics: crate::config::MetricsConfig::default(),
                health: crate::config::HealthConfig::default(),
            },
            logging: crate::config::LoggingConfig::default(),
            security: crate::config::SecurityConfig::default(),
        }
    }

    #[test]
    fn test_validate_allowed_mode_transitions() {
        let solo_config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
        let pool_config = create_test_config(OperationModeConfig::Pool(PoolConfig::default()));

        // Solo to Pool should be allowed
        let result = ModeHandlerFactory::validate_mode_switch(&solo_config, &pool_config);
        assert!(result.is_ok());

        // Pool to Solo should be allowed
        let result = ModeHandlerFactory::validate_mode_switch(&pool_config, &solo_config);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_disallowed_mode_transitions() {
        let solo_config = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
        let proxy_config = create_test_config(OperationModeConfig::Proxy(crate::config::ProxyConfig::default()));

        // Solo to Proxy should not be allowed
        let result = ModeHandlerFactory::validate_mode_switch(&solo_config, &proxy_config);
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_same_mode_transition() {
        let solo_config1 = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
        let solo_config2 = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));

        // Same mode should always be allowed
        let result = ModeHandlerFactory::validate_mode_switch(&solo_config1, &solo_config2);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_config_compatibility() {
        let config1 = create_test_config(OperationModeConfig::Solo(SoloConfig::default()));
        let mut config2 = config1.clone();
        
        // Same config should be compatible
        let result = ModeHandlerFactory::validate_config_compatibility(&config1, &config2);
        assert!(result.is_ok());

        // Different database config should not be compatible
        config2.database.url = "sqlite:///different.db".to_string();
        let result = ModeHandlerFactory::validate_config_compatibility(&config1, &config2);
        assert!(result.is_err());
    }
}