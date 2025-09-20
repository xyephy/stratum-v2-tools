//! Mode-specific implementations for different operational modes

pub mod solo;
pub mod pool;
pub mod proxy;
pub mod proxy_protocol;
pub mod client;

pub use solo::SoloModeHandler;
pub use pool::PoolModeHandler;
pub use proxy::ProxyModeHandler;
pub use client::ClientModeHandler;

use crate::{Result, Error, config::DaemonConfig, database::DatabaseOps, bitcoin_rpc::BitcoinRpcClient};
use std::sync::Arc;

/// Factory for creating mode handlers
pub struct ModeHandlerFactory;

impl ModeHandlerFactory {
    /// Create a mode handler based on configuration
    pub fn create_handler(
        config: &DaemonConfig,
        bitcoin_client: BitcoinRpcClient,
        database: Arc<dyn DatabaseOps>,
    ) -> Result<Box<dyn crate::mode::ModeHandler>> {
        match &config.mode {
            crate::config::OperationModeConfig::Solo(solo_config) => {
                let handler = SoloModeHandler::new(
                    solo_config.clone(),
                    bitcoin_client,
                    database,
                );
                Ok(Box::new(handler))
            }
            crate::config::OperationModeConfig::Pool(pool_config) => {
                let handler = PoolModeHandler::new(
                    pool_config.clone(),
                    bitcoin_client,
                    database,
                );
                Ok(Box::new(handler))
            }
            crate::config::OperationModeConfig::Proxy(proxy_config) => {
                let handler = ProxyModeHandler::new(
                    proxy_config.clone(),
                    database,
                );
                Ok(Box::new(handler))
            }
            crate::config::OperationModeConfig::Client(client_config) => {
                let handler = ClientModeHandler::new(
                    client_config.clone(),
                    database,
                );
                Ok(Box::new(handler))
            }
        }
    }
}