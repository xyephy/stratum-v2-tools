//! Proxy mode implementation using SRI Translator
//! 
//! This module wraps the SRI Translator to provide proxy functionality.
//! It translates between Stratum V1 (for miners like Bitaxe) and Stratum V2 (to SRI Pool).

use crate::{Result, Error, config::ProxyConfig};
use std::fs::write;
use std::path::Path;
use tokio::process::Command;
use tracing::{info, error};

/// Proxy mode handler that uses SRI Translator
pub struct ProxyModeHandler {
    config: ProxyConfig,
}

impl ProxyModeHandler {
    pub fn new(config: ProxyConfig) -> Self {
        Self { config }
    }

    /// Create SRI Translator config file
    fn create_translator_config(&self) -> Result<String> {
        let config_content = format!(
            r#"# SRI Translator config for proxy mode
downstream_address = "0.0.0.0"
downstream_port = {}
max_supported_version = 2
min_supported_version = 2
downstream_extranonce2_size = 4
user_identity = "proxy_miner"
aggregate_channels = true

# Difficulty params
[downstream_difficulty_config]
min_individual_miner_hashrate = 500_000_000_000.0  # 500 GH/s
shares_per_minute = 6.0
enable_vardiff = true

# Connect to SRI pool
[[upstreams]]
address = "{}"
port = {}
authority_pubkey = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
"#,
            self.config.bind_port,
            self.config.upstream_address,
            self.config.upstream_port
        );

        let config_path = "/tmp/translator_config.toml";
        write(config_path, config_content)
            .map_err(|e| Error::Config(format!("Failed to write translator config: {}", e)))?;
        
        Ok(config_path.to_string())
    }

    /// Run the SRI Translator
    async fn run_translator(&self) -> Result<()> {
        let config_path = self.create_translator_config()?;
        
        // Path to the built SRI Translator
        let translator_path = "/Users/munje/dawn/stratum-v2-tools/stratum-reference/roles/target/debug/translator_sv2";
        
        if !Path::new(translator_path).exists() {
            return Err(Error::Config(
                "SRI Translator not found. Run: cd stratum-reference/roles && cargo build".to_string()
            ));
        }

        info!("Starting SRI Translator on port {}", self.config.bind_port);
        info!("Connecting to upstream pool at {}:{}", self.config.upstream_address, self.config.upstream_port);

        let mut child = Command::new(translator_path)
            .arg("-c")
            .arg(&config_path)
            .spawn()
            .map_err(|e| Error::Config(format!("Failed to start SRI Translator: {}", e)))?;

        // Wait for the translator to finish
        let status = child.wait().await
            .map_err(|e| Error::Config(format!("SRI Translator error: {}", e)))?;

        if !status.success() {
            return Err(Error::Config("SRI Translator exited with error".to_string()));
        }

        Ok(())
    }

    pub async fn start(&self) -> Result<()> {
        info!("Starting Proxy mode using SRI Translator");
        self.run_translator().await
    }
}