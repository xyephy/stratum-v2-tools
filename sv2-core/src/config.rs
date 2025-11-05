use crate::{Result, Error};
use crate::mode::OperationMode;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::collections::HashMap;


/// Main daemon configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaemonConfig {
    pub mode: OperationModeConfig,
    pub network: NetworkConfig,
    pub bitcoin: BitcoinConfig,
    pub database: DatabaseConfig,
    pub monitoring: MonitoringConfig,
    pub logging: LoggingConfig,
    pub security: SecurityConfig,
}

/// Operation mode with mode-specific configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "config")]
pub enum OperationModeConfig {
    Solo(SoloConfig),
    Pool(PoolConfig),
    Proxy(ProxyConfig),
    Client(ClientConfig),
}

/// Solo mining mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoloConfig {
    pub coinbase_address: String,
    pub block_template_refresh_interval: u64,
    pub enable_custom_templates: bool,
    pub max_template_age: u64,
}

/// Pool mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolConfig {
    pub share_difficulty: f64,
    pub variable_difficulty: bool,
    pub min_difficulty: f64,
    pub max_difficulty: f64,
    pub difficulty_adjustment_interval: u64,
    pub payout_threshold: f64,
    pub fee_percentage: f64,
}

/// Proxy mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProxyConfig {
    pub upstream_pools: Vec<UpstreamPool>,
    pub failover_enabled: bool,
    pub load_balancing: LoadBalancingStrategy,
    pub connection_retry_interval: u64,
    pub max_retry_attempts: u32,
    // Legacy fields for backwards compatibility
    #[serde(default = "default_bind_port")]
    pub bind_port: u16,
    #[serde(default)]
    pub upstream_address: String,
    #[serde(default = "default_upstream_port")]
    pub upstream_port: u16,
}

fn default_bind_port() -> u16 {
    3333
}

fn default_upstream_port() -> u16 {
    50124
}

/// Client mode configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfig {
    pub upstream_pool: UpstreamPool,
    pub enable_job_negotiation: bool,
    pub custom_template_enabled: bool,
    pub reconnect_interval: u64,
    pub max_reconnect_attempts: u32,
}

/// Upstream pool configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpstreamPool {
    pub url: String,
    pub username: String,
    pub password: String,
    pub priority: u32,
    pub weight: u32,
}

/// Load balancing strategies for proxy mode
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LoadBalancingStrategy {
    RoundRobin,
    WeightedRoundRobin,
    LeastConnections,
    Random,
}

/// Network configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub bind_address: SocketAddr,
    pub max_connections: usize,
    pub connection_timeout: u64,
    pub keepalive_interval: u64,
}

/// Bitcoin node configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitcoinConfig {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
    pub network: BitcoinNetwork,
    pub coinbase_address: Option<String>,
    pub block_template_timeout: u64,
}

/// Bitcoin network types
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum BitcoinNetwork {
    Mainnet,
    Testnet,
    Signet,
    Regtest,
}

/// Database configuration
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub connection_timeout: u64,
    pub enable_migrations: bool,
}

/// Monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub enable_metrics: bool,
    pub metrics_bind_address: SocketAddr,
    pub enable_health_checks: bool,
    pub health_check_interval: u64,
    pub metrics: MetricsConfig,
    pub health: HealthConfig,
}

/// Health monitoring configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthConfig {
    /// Enable health monitoring
    pub enabled: bool,
    /// Health check interval in seconds
    pub check_interval: u64,
    /// Timeout for individual health checks
    pub check_timeout: u64,
    /// Alert thresholds
    pub alert_thresholds: AlertThresholds,
}

/// Alert threshold configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertThresholds {
    /// CPU usage threshold (percentage)
    pub cpu_usage: f64,
    /// Memory usage threshold (percentage)
    pub memory_usage: f64,
    /// Connection count threshold
    pub connection_count: u32,
    /// Share rejection rate threshold (percentage)
    pub rejection_rate: f64,
    /// Response time threshold (milliseconds)
    pub response_time: u64,
    /// Database connection threshold
    pub database_connections: u32,
}

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics collection interval in seconds
    pub collection_interval: u64,
    /// Prometheus metrics endpoint port
    pub prometheus_port: u16,
    /// Enable system resource monitoring
    pub system_monitoring: bool,
    /// Custom metrics labels
    pub labels: HashMap<String, String>,
}

/// Logging configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// Global log level (trace, debug, info, warn, error)
    pub level: String,
    /// Per-component log levels
    pub component_levels: HashMap<String, String>,
    /// Output format (json, pretty, compact)
    pub format: LogFormat,
    /// Log output destination
    pub output: LogOutput,
    /// Whether to include correlation IDs
    pub enable_correlation_ids: bool,
    /// Whether to redact sensitive data
    pub redact_sensitive_data: bool,
    /// Maximum log file size in MB
    pub max_file_size_mb: Option<u64>,
    /// Number of log files to retain
    pub max_files: Option<u32>,
}

/// Log format options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogFormat {
    Json,
    Pretty,
    Compact,
}

/// Log output options
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LogOutput {
    Stdout,
    File(PathBuf),
    Both(PathBuf),
}

/// Security configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    pub enable_authentication: bool,
    pub api_key: Option<String>,
    pub rate_limit_per_minute: u32,
    pub enable_tls: bool,
    pub tls_cert_path: Option<PathBuf>,
    pub tls_key_path: Option<PathBuf>,
    pub auth: crate::auth::AuthConfig,
}

impl Default for DaemonConfig {
    fn default() -> Self {
        Self {
            mode: OperationModeConfig::Solo(SoloConfig::default()),
            network: NetworkConfig::default(),
            bitcoin: BitcoinConfig::default(),
            database: DatabaseConfig::default(),
            monitoring: MonitoringConfig::default(),
            logging: LoggingConfig::default(),
            security: SecurityConfig::default(),
        }
    }
}

impl Default for SoloConfig {
    fn default() -> Self {
        Self {
            coinbase_address: "".to_string(), // Must be set by user
            block_template_refresh_interval: 30,
            enable_custom_templates: false,
            max_template_age: 300,
        }
    }
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            share_difficulty: 1.0,
            variable_difficulty: true,
            min_difficulty: 0.1,
            max_difficulty: 1000000.0,
            difficulty_adjustment_interval: 120,
            payout_threshold: 0.001,
            fee_percentage: 1.0,
        }
    }
}

impl Default for ProxyConfig {
    fn default() -> Self {
        Self {
            upstream_pools: vec![],
            failover_enabled: true,
            load_balancing: LoadBalancingStrategy::RoundRobin,
            connection_retry_interval: 30,
            max_retry_attempts: 5,
        }
    }
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            upstream_pool: UpstreamPool::default(),
            enable_job_negotiation: false,
            custom_template_enabled: false,
            reconnect_interval: 30,
            max_reconnect_attempts: 10,
        }
    }
}

impl Default for UpstreamPool {
    fn default() -> Self {
        Self {
            url: "stratum+tcp://pool.example.com:4444".to_string(),
            username: "worker".to_string(),
            password: "password".to_string(),
            priority: 1,
            weight: 1,
        }
    }
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            bind_address: "127.0.0.1:3333".parse().unwrap(),
            max_connections: 1000,
            connection_timeout: 30,
            keepalive_interval: 60,
        }
    }
}

impl Default for BitcoinConfig {
    fn default() -> Self {
        Self {
            rpc_url: "http://127.0.0.1:8332".to_string(),
            rpc_user: "bitcoin".to_string(),
            rpc_password: "password".to_string(),
            network: BitcoinNetwork::Regtest,
            coinbase_address: None,
            block_template_timeout: 30,
        }
    }
}

impl Default for DatabaseConfig {
    fn default() -> Self {
        Self {
            url: "sqlite://sv2d.db".to_string(),
            max_connections: 10,
            connection_timeout: 30,
            enable_migrations: true,
        }
    }
}

impl Default for MonitoringConfig {
    fn default() -> Self {
        Self {
            enable_metrics: true,
            metrics_bind_address: "127.0.0.1:9090".parse().unwrap(),
            enable_health_checks: true,
            health_check_interval: 30,
            metrics: MetricsConfig::default(),
            health: HealthConfig::default(),
        }
    }
}

impl Default for HealthConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            check_interval: 30,
            check_timeout: 10,
            alert_thresholds: AlertThresholds::default(),
        }
    }
}

impl Default for AlertThresholds {
    fn default() -> Self {
        Self {
            cpu_usage: 80.0,
            memory_usage: 85.0,
            connection_count: 900,
            rejection_rate: 10.0,
            response_time: 5000,
            database_connections: 8,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: 10,
            prometheus_port: 9090,
            system_monitoring: true,
            labels: HashMap::new(),
        }
    }
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            level: "info".to_string(),
            component_levels: HashMap::new(),
            format: LogFormat::Json,
            output: LogOutput::Stdout,
            enable_correlation_ids: true,
            redact_sensitive_data: true,
            max_file_size_mb: Some(100),
            max_files: Some(10),
        }
    }
}

impl Default for SecurityConfig {
    fn default() -> Self {
        Self {
            enable_authentication: false,
            api_key: None,
            rate_limit_per_minute: 60,
            enable_tls: false,
            tls_cert_path: None,
            tls_key_path: None,
            auth: crate::auth::AuthConfig::default(),
        }
    }
}

impl DaemonConfig {
    /// Load configuration from file
    pub fn from_file(path: &std::path::Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| Error::Config(format!("Failed to read config file: {}", e)))?;
        
        let config: Self = toml::from_str(&content)
            .map_err(|e| Error::Config(format!("Failed to parse config: {}", e)))?;
        
        config.validate()?;
        Ok(config)
    }

    /// Save configuration to file
    pub fn to_file(&self, path: &std::path::Path) -> Result<()> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| Error::Config(format!("Failed to serialize config: {}", e)))?;
        
        std::fs::write(path, content)
            .map_err(|e| Error::Config(format!("Failed to write config file: {}", e)))?;
        
        Ok(())
    }

    /// Validate configuration
    pub fn validate(&self) -> Result<()> {
        // Validate network configuration
        self.validate_network()?;
        
        // Validate Bitcoin configuration
        self.validate_bitcoin()?;
        
        // Validate database configuration
        self.validate_database()?;
        
        // Validate monitoring configuration
        self.validate_monitoring()?;
        
        // Validate logging configuration
        self.validate_logging()?;
        
        // Validate security configuration
        self.validate_security()?;
        
        // Mode-specific validation
        self.validate_mode()?;

        // Cross-component dependency validation
        self.validate_dependencies()?;

        Ok(())
    }

    /// Validate dependencies between different configuration sections
    fn validate_dependencies(&self) -> Result<()> {
        // Solo mode requires Bitcoin node configuration
        if matches!(self.mode, OperationModeConfig::Solo(_)) {
            if self.bitcoin.rpc_url.is_empty() {
                return Err(Error::Config("Solo mode requires Bitcoin RPC configuration".to_string()));
            }
        }

        // Pool mode requires database for share tracking
        if matches!(self.mode, OperationModeConfig::Pool(_)) {
            if self.database.url.is_empty() {
                return Err(Error::Config("Pool mode requires database configuration".to_string()));
            }
        }

        // Proxy mode with multiple upstreams requires load balancing strategy
        if let OperationModeConfig::Proxy(proxy_config) = &self.mode {
            if proxy_config.upstream_pools.len() > 1 {
                // Ensure load balancing strategy is appropriate
                match proxy_config.load_balancing {
                    LoadBalancingStrategy::WeightedRoundRobin => {
                        // Check that weights are properly configured
                        let total_weight: u32 = proxy_config.upstream_pools.iter().map(|p| p.weight).sum();
                        if total_weight == 0 {
                            return Err(Error::Config("Weighted round robin requires non-zero weights".to_string()));
                        }
                    }
                    _ => {} // Other strategies don't require special validation
                }
            }
        }

        // TLS configuration consistency
        if self.security.enable_tls {
            if self.security.tls_cert_path.is_none() || self.security.tls_key_path.is_none() {
                return Err(Error::Config("TLS requires both certificate and key paths".to_string()));
            }
        }

        // Monitoring configuration consistency
        if self.monitoring.enable_metrics {
            // Ensure metrics bind address doesn't conflict with main bind address
            if self.monitoring.metrics_bind_address == self.network.bind_address {
                return Err(Error::Config("Metrics bind address cannot be the same as main bind address".to_string()));
            }
        }

        // Logging configuration consistency
        if let LogOutput::File(ref path) | LogOutput::Both(ref path) = self.logging.output {
            if let Some(parent) = path.parent() {
                if !parent.exists() {
                    return Err(Error::Config(format!("Log directory does not exist: {}", parent.display())));
                }
            }
        }

        Ok(())
    }

    fn validate_network(&self) -> Result<()> {
        if self.network.max_connections == 0 {
            return Err(Error::Config("max_connections must be greater than 0".to_string()));
        }
        
        if self.network.connection_timeout == 0 {
            return Err(Error::Config("connection_timeout must be greater than 0".to_string()));
        }
        
        if self.network.keepalive_interval == 0 {
            return Err(Error::Config("keepalive_interval must be greater than 0".to_string()));
        }
        
        Ok(())
    }

    fn validate_bitcoin(&self) -> Result<()> {
        if self.bitcoin.rpc_url.is_empty() {
            return Err(Error::Config("Bitcoin RPC URL cannot be empty".to_string()));
        }
        
        if self.bitcoin.rpc_user.is_empty() {
            return Err(Error::Config("Bitcoin RPC user cannot be empty".to_string()));
        }
        
        if self.bitcoin.rpc_password.is_empty() {
            return Err(Error::Config("Bitcoin RPC password cannot be empty".to_string()));
        }
        
        if self.bitcoin.block_template_timeout == 0 {
            return Err(Error::Config("block_template_timeout must be greater than 0".to_string()));
        }
        
        Ok(())
    }

    fn validate_database(&self) -> Result<()> {
        if self.database.url.is_empty() {
            return Err(Error::Config("Database URL cannot be empty".to_string()));
        }
        
        if self.database.max_connections == 0 {
            return Err(Error::Config("Database max_connections must be greater than 0".to_string()));
        }
        
        if self.database.connection_timeout == 0 {
            return Err(Error::Config("Database connection_timeout must be greater than 0".to_string()));
        }
        
        Ok(())
    }

    fn validate_monitoring(&self) -> Result<()> {
        if self.monitoring.enable_health_checks && self.monitoring.health_check_interval == 0 {
            return Err(Error::Config("health_check_interval must be greater than 0 when health checks are enabled".to_string()));
        }
        
        Ok(())
    }

    fn validate_logging(&self) -> Result<()> {
        let valid_levels = ["trace", "debug", "info", "warn", "error"];
        if !valid_levels.contains(&self.logging.level.as_str()) {
            return Err(Error::Config(format!("Invalid log level: {}. Must be one of: {:?}", self.logging.level, valid_levels)));
        }
        
        Ok(())
    }

    fn validate_security(&self) -> Result<()> {
        if self.security.enable_authentication && self.security.api_key.is_none() {
            return Err(Error::Config("API key is required when authentication is enabled".to_string()));
        }
        
        if self.security.enable_tls {
            if self.security.tls_cert_path.is_none() {
                return Err(Error::Config("TLS certificate path is required when TLS is enabled".to_string()));
            }
            if self.security.tls_key_path.is_none() {
                return Err(Error::Config("TLS key path is required when TLS is enabled".to_string()));
            }
        }

        // Validate auth configuration
        if self.security.auth.enabled {
            if self.security.auth.session_timeout == 0 {
                return Err(Error::Config("Session timeout must be greater than 0".to_string()));
            }
            if self.security.auth.rate_limit_per_minute == 0 {
                return Err(Error::Config("Rate limit per minute must be greater than 0".to_string()));
            }
            if self.security.auth.max_sessions_per_key == 0 {
                return Err(Error::Config("Max sessions per key must be greater than 0".to_string()));
            }
        }
        
        Ok(())
    }

    fn validate_mode(&self) -> Result<()> {
        match &self.mode {
            OperationModeConfig::Solo(config) => self.validate_solo_config(config),
            OperationModeConfig::Pool(config) => self.validate_pool_config(config),
            OperationModeConfig::Proxy(config) => self.validate_proxy_config(config),
            OperationModeConfig::Client(config) => self.validate_client_config(config),
        }
    }

    fn validate_solo_config(&self, config: &SoloConfig) -> Result<()> {
        if config.coinbase_address.is_empty() {
            return Err(Error::Config("Solo mode requires a coinbase address".to_string()));
        }
        
        // Basic Bitcoin address validation (simplified)
        if !config.coinbase_address.starts_with('1') && 
           !config.coinbase_address.starts_with('3') && 
           !config.coinbase_address.starts_with("bc1") &&
           !config.coinbase_address.starts_with("tb1") {
            return Err(Error::Config("Invalid coinbase address format".to_string()));
        }
        
        if config.block_template_refresh_interval == 0 {
            return Err(Error::Config("block_template_refresh_interval must be greater than 0".to_string()));
        }
        
        if config.max_template_age == 0 {
            return Err(Error::Config("max_template_age must be greater than 0".to_string()));
        }
        
        Ok(())
    }

    fn validate_pool_config(&self, config: &PoolConfig) -> Result<()> {
        if config.share_difficulty <= 0.0 {
            return Err(Error::Config("share_difficulty must be greater than 0".to_string()));
        }
        
        if config.variable_difficulty {
            if config.min_difficulty <= 0.0 {
                return Err(Error::Config("min_difficulty must be greater than 0".to_string()));
            }
            if config.max_difficulty <= config.min_difficulty {
                return Err(Error::Config("max_difficulty must be greater than min_difficulty".to_string()));
            }
            if config.difficulty_adjustment_interval == 0 {
                return Err(Error::Config("difficulty_adjustment_interval must be greater than 0".to_string()));
            }
        }
        
        if config.payout_threshold < 0.0 {
            return Err(Error::Config("payout_threshold cannot be negative".to_string()));
        }
        
        if config.fee_percentage < 0.0 || config.fee_percentage > 100.0 {
            return Err(Error::Config("fee_percentage must be between 0 and 100".to_string()));
        }
        
        Ok(())
    }

    fn validate_proxy_config(&self, config: &ProxyConfig) -> Result<()> {
        if config.upstream_pools.is_empty() {
            return Err(Error::Config("Proxy mode requires at least one upstream pool".to_string()));
        }
        
        for (i, pool) in config.upstream_pools.iter().enumerate() {
            if pool.url.is_empty() {
                return Err(Error::Config(format!("Upstream pool {} URL cannot be empty", i)));
            }
            if pool.username.is_empty() {
                return Err(Error::Config(format!("Upstream pool {} username cannot be empty", i)));
            }
        }
        
        if config.connection_retry_interval == 0 {
            return Err(Error::Config("connection_retry_interval must be greater than 0".to_string()));
        }
        
        if config.max_retry_attempts == 0 {
            return Err(Error::Config("max_retry_attempts must be greater than 0".to_string()));
        }
        
        Ok(())
    }

    fn validate_client_config(&self, config: &ClientConfig) -> Result<()> {
        if config.upstream_pool.url.is_empty() {
            return Err(Error::Config("Client mode requires upstream pool URL".to_string()));
        }
        
        if config.upstream_pool.username.is_empty() {
            return Err(Error::Config("Client mode requires upstream pool username".to_string()));
        }
        
        if config.reconnect_interval == 0 {
            return Err(Error::Config("reconnect_interval must be greater than 0".to_string()));
        }
        
        if config.max_reconnect_attempts == 0 {
            return Err(Error::Config("max_reconnect_attempts must be greater than 0".to_string()));
        }
        
        Ok(())
    }

    /// Merge with environment variables
    pub fn merge_env(&mut self) -> Result<()> {
        // Network configuration
        if let Ok(bind_addr) = std::env::var("SV2D_BIND_ADDRESS") {
            self.network.bind_address = bind_addr.parse()
                .map_err(|e| Error::Config(format!("Invalid bind address: {}", e)))?;
        }

        if let Ok(max_conn) = std::env::var("SV2D_MAX_CONNECTIONS") {
            self.network.max_connections = max_conn.parse()
                .map_err(|e| Error::Config(format!("Invalid max_connections: {}", e)))?;
        }

        // Bitcoin configuration
        if let Ok(rpc_url) = std::env::var("SV2D_BITCOIN_RPC_URL") {
            self.bitcoin.rpc_url = rpc_url;
        }

        if let Ok(rpc_user) = std::env::var("SV2D_BITCOIN_RPC_USER") {
            self.bitcoin.rpc_user = rpc_user;
        }

        if let Ok(rpc_password) = std::env::var("SV2D_BITCOIN_RPC_PASSWORD") {
            self.bitcoin.rpc_password = rpc_password;
        }

        if let Ok(network) = std::env::var("SV2D_BITCOIN_NETWORK") {
            self.bitcoin.network = match network.to_lowercase().as_str() {
                "mainnet" => BitcoinNetwork::Mainnet,
                "testnet" => BitcoinNetwork::Testnet,
                "signet" => BitcoinNetwork::Signet,
                "regtest" => BitcoinNetwork::Regtest,
                _ => return Err(Error::Config(format!("Invalid Bitcoin network: {}", network))),
            };
        }

        // Database configuration
        if let Ok(db_url) = std::env::var("SV2D_DATABASE_URL") {
            self.database.url = db_url;
        }

        // Logging configuration
        if let Ok(log_level) = std::env::var("SV2D_LOG_LEVEL") {
            self.logging.level = log_level;
        }

        // Mode-specific environment variables
        match &mut self.mode {
            OperationModeConfig::Solo(config) => {
                if let Ok(coinbase_addr) = std::env::var("SV2D_COINBASE_ADDRESS") {
                    config.coinbase_address = coinbase_addr;
                }
            }
            OperationModeConfig::Pool(config) => {
                if let Ok(difficulty) = std::env::var("SV2D_SHARE_DIFFICULTY") {
                    config.share_difficulty = difficulty.parse()
                        .map_err(|e| Error::Config(format!("Invalid share difficulty: {}", e)))?;
                }
                if let Ok(fee) = std::env::var("SV2D_FEE_PERCENTAGE") {
                    config.fee_percentage = fee.parse()
                        .map_err(|e| Error::Config(format!("Invalid fee percentage: {}", e)))?;
                }
            }
            OperationModeConfig::Proxy(_) => {
                // Proxy mode env vars would be more complex due to multiple upstream pools
                // For now, we'll handle this in configuration files
            }
            OperationModeConfig::Client(config) => {
                if let Ok(upstream_url) = std::env::var("SV2D_UPSTREAM_URL") {
                    config.upstream_pool.url = upstream_url;
                }
                if let Ok(upstream_user) = std::env::var("SV2D_UPSTREAM_USERNAME") {
                    config.upstream_pool.username = upstream_user;
                }
                if let Ok(upstream_pass) = std::env::var("SV2D_UPSTREAM_PASSWORD") {
                    config.upstream_pool.password = upstream_pass;
                }
            }
        }

        Ok(())
    }

    /// Get the operation mode type
    pub fn get_mode_type(&self) -> OperationMode {
        match &self.mode {
            OperationModeConfig::Solo(_) => OperationMode::Solo,
            OperationModeConfig::Pool(_) => OperationMode::Pool,
            OperationModeConfig::Proxy(_) => OperationMode::Proxy,
            OperationModeConfig::Client(_) => OperationMode::Client,
        }
    }

    /// Load configuration with environment variable override
    pub fn load_with_env(path: Option<&std::path::Path>) -> Result<Self> {
        let mut config = if let Some(path) = path {
            Self::from_file(path)?
        } else {
            Self::default()
        };
        
        config.merge_env()?;
        config.validate()?;
        Ok(config)
    }

    /// Load configuration from multiple sources with priority:
    /// 1. Command line arguments (highest priority)
    /// 2. Environment variables
    /// 3. Configuration file
    /// 4. Default values (lowest priority)
    pub fn load_from_sources(
        config_path: Option<&std::path::Path>,
        env_overrides: Option<HashMap<String, String>>,
    ) -> Result<Self> {
        // Start with default configuration
        let mut config = Self::default();

        // Load from file if provided
        if let Some(path) = config_path {
            if path.exists() {
                config = Self::from_file(path)?;
            }
        }

        // Apply environment variables
        config.merge_env()?;

        // Apply any additional overrides
        if let Some(overrides) = env_overrides {
            config.apply_overrides(overrides)?;
        }

        // Validate final configuration
        config.validate()?;
        Ok(config)
    }

    /// Apply configuration overrides from a key-value map
    pub fn apply_overrides(&mut self, overrides: HashMap<String, String>) -> Result<()> {
        for (key, value) in overrides {
            self.apply_single_override(&key, &value)?;
        }
        Ok(())
    }

    /// Apply a single configuration override
    fn apply_single_override(&mut self, key: &str, value: &str) -> Result<()> {
        match key {
            "network.bind_address" => {
                self.network.bind_address = value.parse()
                    .map_err(|e| Error::Config(format!("Invalid bind address '{}': {}", value, e)))?;
            }
            "network.max_connections" => {
                self.network.max_connections = value.parse()
                    .map_err(|e| Error::Config(format!("Invalid max_connections '{}': {}", value, e)))?;
            }
            "bitcoin.rpc_url" => {
                self.bitcoin.rpc_url = value.to_string();
            }
            "bitcoin.rpc_user" => {
                self.bitcoin.rpc_user = value.to_string();
            }
            "bitcoin.rpc_password" => {
                self.bitcoin.rpc_password = value.to_string();
            }
            "bitcoin.network" => {
                self.bitcoin.network = match value.to_lowercase().as_str() {
                    "mainnet" => BitcoinNetwork::Mainnet,
                    "testnet" => BitcoinNetwork::Testnet,
                    "signet" => BitcoinNetwork::Signet,
                    "regtest" => BitcoinNetwork::Regtest,
                    _ => return Err(Error::Config(format!("Invalid Bitcoin network: {}", value))),
                };
            }
            "database.url" => {
                self.database.url = value.to_string();
            }
            "logging.level" => {
                self.logging.level = value.to_string();
            }
            "mode.solo.coinbase_address" => {
                if let OperationModeConfig::Solo(ref mut config) = self.mode {
                    config.coinbase_address = value.to_string();
                }
            }
            "mode.pool.share_difficulty" => {
                if let OperationModeConfig::Pool(ref mut config) = self.mode {
                    config.share_difficulty = value.parse()
                        .map_err(|e| Error::Config(format!("Invalid share difficulty '{}': {}", value, e)))?;
                }
            }
            _ => {
                return Err(Error::Config(format!("Unknown configuration key: {}", key)));
            }
        }
        Ok(())
    }

    /// Create a configuration template for a specific mode
    pub fn template_for_mode(mode: OperationMode) -> Self {
        let mode_config = match mode {
            OperationMode::Solo => OperationModeConfig::Solo(SoloConfig::default()),
            OperationMode::Pool => OperationModeConfig::Pool(PoolConfig::default()),
            OperationMode::Proxy => OperationModeConfig::Proxy(ProxyConfig::default()),
            OperationMode::Client => OperationModeConfig::Client(ClientConfig::default()),
        };

        Self {
            mode: mode_config,
            ..Default::default()
        }
    }
}
impl OperationModeConfig {
    /// Get the mode type
    pub fn mode_type(&self) -> OperationMode {
        match self {
            OperationModeConfig::Solo(_) => OperationMode::Solo,
            OperationModeConfig::Pool(_) => OperationMode::Pool,
            OperationModeConfig::Proxy(_) => OperationMode::Proxy,
            OperationModeConfig::Client(_) => OperationMode::Client,
        }
    }

    /// Get mode-specific configuration as a string for display
    pub fn mode_description(&self) -> String {
        match self {
            OperationModeConfig::Solo(config) => {
                format!("Solo mining to address: {}", config.coinbase_address)
            }
            OperationModeConfig::Pool(config) => {
                format!("Pool mode with {}% fee", config.fee_percentage)
            }
            OperationModeConfig::Proxy(config) => {
                format!("Proxy mode with {} upstream pools", config.upstream_pools.len())
            }
            OperationModeConfig::Client(config) => {
                format!("Client mode connecting to: {}", config.upstream_pool.url)
            }
        }
    }
}

impl std::fmt::Display for OperationModeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OperationModeConfig::Solo(_) => write!(f, "solo"),
            OperationModeConfig::Pool(_) => write!(f, "pool"),
            OperationModeConfig::Proxy(_) => write!(f, "proxy"),
            OperationModeConfig::Client(_) => write!(f, "client"),
        }
    }
}

impl UpstreamPool {
    /// Validate upstream pool configuration
    pub fn validate(&self) -> Result<()> {
        if self.url.is_empty() {
            return Err(Error::Config("Upstream pool URL cannot be empty".to_string()));
        }
        
        if self.username.is_empty() {
            return Err(Error::Config("Upstream pool username cannot be empty".to_string()));
        }
        
        // Validate URL format
        if !self.url.starts_with("stratum+tcp://") && !self.url.starts_with("stratum+ssl://") {
            return Err(Error::Config("Upstream pool URL must use stratum+tcp:// or stratum+ssl:// scheme".to_string()));
        }
        
        // Validate that we can parse host and port
        self.host_port()?;
        
        Ok(())
    }
    
    /// Extract host and port from URL
    pub fn host_port(&self) -> Result<(String, u16)> {
        let url = if self.url.starts_with("stratum+tcp://") {
            &self.url[14..] // Remove "stratum+tcp://"
        } else if self.url.starts_with("stratum+ssl://") {
            &self.url[14..] // Remove "stratum+ssl://"
        } else {
            return Err(Error::Config("Invalid upstream pool URL scheme".to_string()));
        };
        
        let parts: Vec<&str> = url.split(':').collect();
        if parts.len() != 2 {
            return Err(Error::Config("Invalid upstream pool URL format, expected host:port".to_string()));
        }
        
        let host = parts[0].to_string();
        let port = parts[1].parse::<u16>()
            .map_err(|_| Error::Config("Invalid port number in upstream pool URL".to_string()))?;
        
        Ok((host, port))
    }
    
    /// Check if pool uses SSL
    pub fn uses_ssl(&self) -> bool {
        self.url.starts_with("stratum+ssl://")
    }
}

impl SoloConfig {
    /// Validate solo configuration
    pub fn validate(&self) -> Result<()> {
        if self.coinbase_address.is_empty() {
            return Err(Error::Config("Solo mode requires a coinbase address".to_string()));
        }
        
        // Basic Bitcoin address validation (simplified)
        if !self.coinbase_address.starts_with('1') && 
           !self.coinbase_address.starts_with('3') && 
           !self.coinbase_address.starts_with("bc1") &&
           !self.coinbase_address.starts_with("tb1") {
            return Err(Error::Config("Invalid coinbase address format".to_string()));
        }
        
        if self.block_template_refresh_interval == 0 {
            return Err(Error::Config("block_template_refresh_interval must be greater than 0".to_string()));
        }
        
        if self.max_template_age == 0 {
            return Err(Error::Config("max_template_age must be greater than 0".to_string()));
        }
        
        Ok(())
    }
}

impl PoolConfig {
    /// Validate pool configuration
    pub fn validate(&self) -> Result<()> {
        if self.share_difficulty <= 0.0 {
            return Err(Error::Config("share_difficulty must be greater than 0".to_string()));
        }
        
        if self.variable_difficulty {
            if self.min_difficulty <= 0.0 {
                return Err(Error::Config("min_difficulty must be greater than 0".to_string()));
            }
            if self.max_difficulty <= self.min_difficulty {
                return Err(Error::Config("max_difficulty must be greater than min_difficulty".to_string()));
            }
            if self.difficulty_adjustment_interval == 0 {
                return Err(Error::Config("difficulty_adjustment_interval must be greater than 0".to_string()));
            }
        }
        
        if self.payout_threshold < 0.0 {
            return Err(Error::Config("payout_threshold cannot be negative".to_string()));
        }
        
        if self.fee_percentage < 0.0 || self.fee_percentage > 100.0 {
            return Err(Error::Config("fee_percentage must be between 0 and 100".to_string()));
        }
        
        Ok(())
    }
}

impl ProxyConfig {
    /// Validate proxy configuration
    pub fn validate(&self) -> Result<()> {
        if self.upstream_pools.is_empty() {
            return Err(Error::Config("Proxy mode requires at least one upstream pool".to_string()));
        }
        
        for (i, pool) in self.upstream_pools.iter().enumerate() {
            pool.validate().map_err(|e| Error::Config(format!("Upstream pool {}: {}", i, e)))?;
        }
        
        if self.connection_retry_interval == 0 {
            return Err(Error::Config("connection_retry_interval must be greater than 0".to_string()));
        }
        
        if self.max_retry_attempts == 0 {
            return Err(Error::Config("max_retry_attempts must be greater than 0".to_string()));
        }
        
        // Validate load balancing strategy
        if matches!(self.load_balancing, LoadBalancingStrategy::WeightedRoundRobin) {
            let total_weight: u32 = self.upstream_pools.iter().map(|p| p.weight).sum();
            if total_weight == 0 {
                return Err(Error::Config("Weighted round robin requires non-zero weights".to_string()));
            }
        }
        
        Ok(())
    }
    
    /// Get primary pool (lowest priority number)
    pub fn primary_pool(&self) -> Option<&UpstreamPool> {
        self.upstream_pools.iter().min_by_key(|pool| pool.priority)
    }
    
    /// Get pools sorted by priority
    pub fn pools_by_priority(&self) -> Vec<&UpstreamPool> {
        let mut pools: Vec<&UpstreamPool> = self.upstream_pools.iter().collect();
        pools.sort_by_key(|pool| pool.priority);
        pools
    }
}

impl ClientConfig {
    /// Validate client configuration
    pub fn validate(&self) -> Result<()> {
        self.upstream_pool.validate()?;
        
        if self.reconnect_interval == 0 {
            return Err(Error::Config("reconnect_interval must be greater than 0".to_string()));
        }
        
        if self.max_reconnect_attempts == 0 {
            return Err(Error::Config("max_reconnect_attempts must be greater than 0".to_string()));
        }
        
        Ok(())
    }
}

impl std::str::FromStr for OperationModeConfig {
    type Err = Error;
    
    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "solo" => Ok(OperationModeConfig::Solo(SoloConfig::default())),
            "pool" => Ok(OperationModeConfig::Pool(PoolConfig::default())),
            "proxy" => Ok(OperationModeConfig::Proxy(ProxyConfig::default())),
            "client" => Ok(OperationModeConfig::Client(ClientConfig::default())),
            _ => Err(Error::Config(format!("Invalid operation mode: {}", s))),
        }
    }
}





#[cfg(test)]
mod tests {
    use super::*;

    use tempfile::tempdir;

    #[test]
    fn test_default_config_validation() {
        let mut config = DaemonConfig::default();
        
        // Default config should fail validation due to empty coinbase address
        assert!(config.validate().is_err());
        
        // Fix the coinbase address for solo mode
        if let OperationModeConfig::Solo(ref mut solo_config) = config.mode {
            solo_config.coinbase_address = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string();
        }
        
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_config_serialization() {
        let config = DaemonConfig::template_for_mode(OperationMode::Pool);
        let toml_str = toml::to_string_pretty(&config).unwrap();
        let deserialized: DaemonConfig = toml::from_str(&toml_str).unwrap();
        
        assert_eq!(config.get_mode_type(), deserialized.get_mode_type());
    }

    #[test]
    fn test_environment_variable_override() {
        std::env::set_var("SV2D_BITCOIN_RPC_URL", "http://test:8332");
        std::env::set_var("SV2D_LOG_LEVEL", "debug");
        
        let mut config = DaemonConfig::default();
        config.merge_env().unwrap();
        
        assert_eq!(config.bitcoin.rpc_url, "http://test:8332");
        assert_eq!(config.logging.level, "debug");
        
        std::env::remove_var("SV2D_BITCOIN_RPC_URL");
        std::env::remove_var("SV2D_LOG_LEVEL");
    }

    #[test]
    fn test_upstream_pool_validation() {
        let mut pool = UpstreamPool::default();
        pool.url = "invalid-url".to_string();
        assert!(pool.validate().is_err());
        
        pool.url = "stratum+tcp://pool.example.com:4444".to_string();
        assert!(pool.validate().is_ok());
        
        let (host, port) = pool.host_port().unwrap();
        assert_eq!(host, "pool.example.com");
        assert_eq!(port, 4444);
        assert!(!pool.uses_ssl());
    }

    #[test]
    fn test_config_file_operations() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test_config.toml");
        
        let mut config = DaemonConfig::template_for_mode(OperationMode::Solo);
        if let OperationModeConfig::Solo(ref mut solo_config) = config.mode {
            solo_config.coinbase_address = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string();
        }
        
        // Save config
        config.to_file(&config_path).unwrap();
        
        // Load config
        let loaded_config = DaemonConfig::from_file(&config_path).unwrap();
        assert_eq!(config.get_mode_type(), loaded_config.get_mode_type());
    }

    #[test]
    fn test_mode_specific_validation() {
        // Test solo mode validation
        let solo_config = SoloConfig {
            coinbase_address: "".to_string(),
            ..Default::default()
        };
        assert!(solo_config.validate().is_err());
        
        // Test pool mode validation
        let pool_config = PoolConfig {
            share_difficulty: -1.0,
            ..Default::default()
        };
        assert!(pool_config.validate().is_err());
        
        // Test proxy mode validation
        let proxy_config = ProxyConfig {
            upstream_pools: vec![],
            ..Default::default()
        };
        assert!(proxy_config.validate().is_err());
    }

    #[test]
    fn test_configuration_overrides() {
        let mut config = DaemonConfig::default();
        let mut overrides = HashMap::new();
        
        overrides.insert("network.bind_address".to_string(), "0.0.0.0:4444".to_string());
        overrides.insert("bitcoin.rpc_url".to_string(), "http://localhost:18443".to_string());
        overrides.insert("logging.level".to_string(), "trace".to_string());
        
        config.apply_overrides(overrides).unwrap();
        
        assert_eq!(config.network.bind_address.to_string(), "0.0.0.0:4444");
        assert_eq!(config.bitcoin.rpc_url, "http://localhost:18443");
        assert_eq!(config.logging.level, "trace");
    }

    #[test]
    fn test_invalid_configuration_overrides() {
        let mut config = DaemonConfig::default();
        let mut overrides = HashMap::new();
        
        // Test invalid bind address
        overrides.insert("network.bind_address".to_string(), "invalid-address".to_string());
        assert!(config.apply_overrides(overrides.clone()).is_err());
        
        // Test unknown configuration key
        overrides.clear();
        overrides.insert("unknown.key".to_string(), "value".to_string());
        assert!(config.apply_overrides(overrides).is_err());
    }

    #[test]
    fn test_dependency_validation() {
        // Test solo mode without Bitcoin configuration
        let mut config = DaemonConfig::template_for_mode(OperationMode::Solo);
        config.bitcoin.rpc_url = "".to_string();
        if let OperationModeConfig::Solo(ref mut solo_config) = config.mode {
            solo_config.coinbase_address = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string();
        }
        assert!(config.validate().is_err());
        
        // Test pool mode without database configuration
        let mut config = DaemonConfig::template_for_mode(OperationMode::Pool);
        config.database.url = "".to_string();
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_weighted_round_robin_validation() {
        let mut config = DaemonConfig::template_for_mode(OperationMode::Proxy);
        if let OperationModeConfig::Proxy(ref mut proxy_config) = config.mode {
            proxy_config.load_balancing = LoadBalancingStrategy::WeightedRoundRobin;
            proxy_config.upstream_pools = vec![
                UpstreamPool {
                    url: "stratum+tcp://pool1.example.com:4444".to_string(),
                    username: "user1".to_string(),
                    password: "pass1".to_string(),
                    priority: 1,
                    weight: 0, // Invalid weight
                },
                UpstreamPool {
                    url: "stratum+tcp://pool2.example.com:4444".to_string(),
                    username: "user2".to_string(),
                    password: "pass2".to_string(),
                    priority: 2,
                    weight: 0, // Invalid weight
                },
            ];
        }
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_tls_configuration_validation() {
        let mut config = DaemonConfig::default();
        config.security.enable_tls = true;
        // Missing certificate and key paths
        assert!(config.validate().is_err());
        
        config.security.tls_cert_path = Some(PathBuf::from("/path/to/cert.pem"));
        // Still missing key path
        assert!(config.validate().is_err());
        
        config.security.tls_key_path = Some(PathBuf::from("/path/to/key.pem"));
        // Now should pass TLS validation (but may fail on other validations)
        let tls_validation_result = config.validate_security();
        assert!(tls_validation_result.is_ok());
    }

    #[test]
    fn test_metrics_bind_address_conflict() {
        let mut config = DaemonConfig::default();
        config.monitoring.enable_metrics = true;
        config.monitoring.metrics_bind_address = config.network.bind_address;
        
        // Should fail due to address conflict
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_bitcoin_address_validation() {
        // Test valid addresses
        let valid_addresses = vec![
            "1A1zP1eP5QGefi2DMPTfTL5SLmv7DivfNa", // P2PKH
            "3J98t1WpEZ73CNmQviecrnyiWrnqRhWNLy", // P2SH
            "bc1qw508d6qejxtdg4y5r3zarvary0c5xw7kv8f3t4", // Bech32
            "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx", // Testnet Bech32
        ];
        
        for addr in valid_addresses {
            let mut config = DaemonConfig::template_for_mode(OperationMode::Solo);
            if let OperationModeConfig::Solo(ref mut solo_config) = config.mode {
                solo_config.coinbase_address = addr.to_string();
            }
            assert!(config.validate().is_ok(), "Address {} should be valid", addr);
        }
        
        // Test invalid addresses
        let invalid_addresses = vec![
            "", // Empty
            "invalid", // Not a Bitcoin address
            "2MzQwSSnBHWHqSAqtTVQ6v47XtaisrJa1Vc", // Invalid format
        ];
        
        for addr in invalid_addresses {
            let mut config = DaemonConfig::template_for_mode(OperationMode::Solo);
            if let OperationModeConfig::Solo(ref mut solo_config) = config.mode {
                solo_config.coinbase_address = addr.to_string();
            }
            assert!(config.validate().is_err(), "Address {} should be invalid", addr);
        }
    }

    #[test]
    fn test_upstream_pool_url_parsing() {
        let mut pool = UpstreamPool::default();
        
        // Test valid URLs
        pool.url = "stratum+tcp://pool.example.com:4444".to_string();
        let (host, port) = pool.host_port().unwrap();
        assert_eq!(host, "pool.example.com");
        assert_eq!(port, 4444);
        assert!(!pool.uses_ssl());
        
        pool.url = "stratum+ssl://secure.pool.com:443".to_string();
        let (host, port) = pool.host_port().unwrap();
        assert_eq!(host, "secure.pool.com");
        assert_eq!(port, 443);
        assert!(pool.uses_ssl());
        
        // Test invalid URLs
        pool.url = "http://invalid.com:80".to_string();
        assert!(pool.host_port().is_err());
        
        pool.url = "stratum+tcp://noport.com".to_string();
        assert!(pool.host_port().is_err());
    }

    #[test]
    fn test_proxy_pool_priority_sorting() {
        let mut config = DaemonConfig::template_for_mode(OperationMode::Proxy);
        if let OperationModeConfig::Proxy(ref mut proxy_config) = config.mode {
            proxy_config.upstream_pools = vec![
                UpstreamPool {
                    url: "stratum+tcp://pool3.example.com:4444".to_string(),
                    username: "user3".to_string(),
                    password: "pass3".to_string(),
                    priority: 3,
                    weight: 1,
                },
                UpstreamPool {
                    url: "stratum+tcp://pool1.example.com:4444".to_string(),
                    username: "user1".to_string(),
                    password: "pass1".to_string(),
                    priority: 1,
                    weight: 1,
                },
                UpstreamPool {
                    url: "stratum+tcp://pool2.example.com:4444".to_string(),
                    username: "user2".to_string(),
                    password: "pass2".to_string(),
                    priority: 2,
                    weight: 1,
                },
            ];
            
            let primary = proxy_config.primary_pool().unwrap();
            assert_eq!(primary.priority, 1);
            
            let sorted_pools = proxy_config.pools_by_priority();
            assert_eq!(sorted_pools[0].priority, 1);
            assert_eq!(sorted_pools[1].priority, 2);
            assert_eq!(sorted_pools[2].priority, 3);
        }
    }

    #[test]
    fn test_load_from_sources() {
        let dir = tempdir().unwrap();
        let config_path = dir.path().join("test_config.toml");
        
        // Create a test config file
        let mut file_config = DaemonConfig::template_for_mode(OperationMode::Pool);
        file_config.network.bind_address = "127.0.0.1:3333".parse().unwrap();
        file_config.to_file(&config_path).unwrap();
        
        // Set environment variables
        std::env::set_var("SV2D_BITCOIN_RPC_URL", "http://env:8332");
        
        // Create overrides
        let mut overrides = HashMap::new();
        overrides.insert("network.bind_address".to_string(), "0.0.0.0:4444".to_string());
        
        // Load configuration with all sources
        let config = DaemonConfig::load_from_sources(
            Some(&config_path),
            Some(overrides),
        ).unwrap();
        
        // Verify priority: overrides > env > file > default
        assert_eq!(config.network.bind_address.to_string(), "0.0.0.0:4444"); // Override
        assert_eq!(config.bitcoin.rpc_url, "http://env:8332"); // Environment
        assert!(matches!(config.mode, OperationModeConfig::Pool(_))); // File
        
        std::env::remove_var("SV2D_BITCOIN_RPC_URL");
    }

    #[test]
    fn test_configuration_templates() {
        let modes = vec![
            OperationMode::Solo,
            OperationMode::Pool,
            OperationMode::Proxy,
            OperationMode::Client,
        ];
        
        for mode in modes {
            let config = DaemonConfig::template_for_mode(mode.clone());
            assert_eq!(config.get_mode_type(), mode);
            
            // Each template should have appropriate defaults
            match mode {
                OperationMode::Solo => {
                    assert!(matches!(config.mode, OperationModeConfig::Solo(_)));
                }
                OperationMode::Pool => {
                    assert!(matches!(config.mode, OperationModeConfig::Pool(_)));
                }
                OperationMode::Proxy => {
                    assert!(matches!(config.mode, OperationModeConfig::Proxy(_)));
                }
                OperationMode::Client => {
                    assert!(matches!(config.mode, OperationModeConfig::Client(_)));
                }
            }
        }
    }

    #[test]
    fn test_log_level_validation() {
        let mut config = DaemonConfig::default();
        
        // Test valid log levels
        let valid_levels = vec!["trace", "debug", "info", "warn", "error"];
        for level in valid_levels {
            config.logging.level = level.to_string();
            assert!(config.validate_logging().is_ok());
        }
        
        // Test invalid log level
        config.logging.level = "invalid".to_string();
        assert!(config.validate_logging().is_err());
    }

    #[test]
    fn test_difficulty_range_validation() {
        let mut config = DaemonConfig::template_for_mode(OperationMode::Pool);
        
        if let OperationModeConfig::Pool(ref mut pool_config) = config.mode {
            // Test valid difficulty range
            pool_config.variable_difficulty = true;
            pool_config.min_difficulty = 1.0;
            pool_config.max_difficulty = 1000.0;
            assert!(pool_config.validate().is_ok());
            
            // Test invalid range (max < min)
            pool_config.max_difficulty = 0.5;
            assert!(pool_config.validate().is_err());
            
            // Test zero min difficulty
            pool_config.min_difficulty = 0.0;
            pool_config.max_difficulty = 1000.0;
            assert!(pool_config.validate().is_err());
        }
    }

    #[test]
    fn test_example_configurations() {
        // Test solo config example
        let solo_config_path = std::path::Path::new("examples/solo_config.toml");
        if solo_config_path.exists() {
            let config = DaemonConfig::from_file(solo_config_path).unwrap();
            assert!(matches!(config.mode, OperationModeConfig::Solo(_)));
            assert_eq!(config.get_mode_type(), OperationMode::Solo);
        }

        // Test pool config example
        let pool_config_path = std::path::Path::new("examples/pool_config.toml");
        if pool_config_path.exists() {
            let config = DaemonConfig::from_file(pool_config_path).unwrap();
            assert!(matches!(config.mode, OperationModeConfig::Pool(_)));
            assert_eq!(config.get_mode_type(), OperationMode::Pool);
        }

        // Test proxy config example
        let proxy_config_path = std::path::Path::new("examples/proxy_config.toml");
        if proxy_config_path.exists() {
            let config = DaemonConfig::from_file(proxy_config_path).unwrap();
            assert!(matches!(config.mode, OperationModeConfig::Proxy(_)));
            assert_eq!(config.get_mode_type(), OperationMode::Proxy);
            
            if let OperationModeConfig::Proxy(proxy_config) = &config.mode {
                assert_eq!(proxy_config.upstream_pools.len(), 2);
                assert_eq!(proxy_config.load_balancing, LoadBalancingStrategy::WeightedRoundRobin);
            }
        }
    }
}