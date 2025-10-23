use anyhow::{Context, Result};
use clap::{Arg, Command};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::net::TcpListener;
use tokio::process::{Child, Command as TokioCommand};
use tokio::signal;
use tokio::sync::RwLock;
use tokio::time::{sleep, Duration};
use tracing::{error, info, warn};
use std::sync::Arc;
use std::str::FromStr;

mod bitcoin;
use bitcoin::{Network, ensure_bitcoin_running};

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DaemonConfig {
    pub daemon: DaemonSettings,
    pub bitcoin: BitcoinConfig,
    pub pool: PoolConfig,
    pub translator: TranslatorConfig,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct DaemonSettings {
    pub mode: String, // "proxy" for now
    pub network: String, // "signet", "regtest", "mainnet"
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct BitcoinConfig {
    pub rpc_url: String,
    pub rpc_user: String,
    pub rpc_password: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PoolConfig {
    pub signature: String,
    pub coinbase_address: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TranslatorConfig {
    pub bind_address: String,
    pub min_extranonce2_size: u32,
}

#[derive(Debug, Clone)]
pub struct ComponentStatus {
    pub running: bool,
    pub pid: Option<u32>,
    pub last_check: std::time::Instant,
    pub start_time: Option<std::time::Instant>,
    pub restart_count: u32,
    pub last_error: Option<String>,
    pub health_status: HealthStatus,
}

#[derive(Debug, Clone, Serialize)]
pub enum HealthStatus {
    Healthy,
    Warning,
    Critical,
    Unknown,
}

#[derive(Debug)]
pub struct DaemonState {
    pub config: DaemonConfig,
    pub components: RwLock<HashMap<String, ComponentStatus>>,
    pub processes: RwLock<HashMap<String, Child>>,
    pub start_time: std::time::Instant,
    pub connected_miners: RwLock<HashMap<String, MinerInfo>>,
}

impl DaemonState {
    pub fn new(config: DaemonConfig) -> Self {
        Self {
            config,
            components: RwLock::new(HashMap::new()),
            processes: RwLock::new(HashMap::new()),
            start_time: std::time::Instant::now(),
            connected_miners: RwLock::new(HashMap::new()),
        }
    }

    pub async fn update_component_status(&self, name: &str, running: bool, pid: Option<u32>) {
        let mut components = self.components.write().await;
        let now = std::time::Instant::now();
        
        // Update or create component status
        let status = components.entry(name.to_string()).or_insert_with(|| ComponentStatus {
            running: false,
            pid: None,
            last_check: now,
            start_time: None,
            restart_count: 0,
            last_error: None,
            health_status: HealthStatus::Unknown,
        });
        
        // Track restarts
        if !status.running && running {
            status.restart_count += 1;
            status.start_time = Some(now);
        }
        
        status.running = running;
        status.pid = pid;
        status.last_check = now;
        status.health_status = if running { HealthStatus::Healthy } else { HealthStatus::Critical };
    }
    
    pub async fn set_component_error(&self, name: &str, error: String) {
        let mut components = self.components.write().await;
        if let Some(status) = components.get_mut(name) {
            status.last_error = Some(error);
            status.health_status = HealthStatus::Critical;
        }
    }
    
    pub async fn add_connected_miner(&self, ip: String, miner: MinerInfo) {
        let mut miners = self.connected_miners.write().await;
        miners.insert(ip, miner);
    }
    
    pub async fn remove_connected_miner(&self, ip: &str) {
        let mut miners = self.connected_miners.write().await;
        miners.remove(ip);
    }
}

#[derive(Debug, Serialize)]
pub struct StatusResponse {
    pub running: bool,
    pub components: HashMap<String, ComponentStatusInfo>,
    pub miners: ConnectedMinersInfo,
    pub system_info: SystemInfo,
    pub uptime_seconds: u64,
}

#[derive(Debug, Serialize)]
pub struct ComponentStatusInfo {
    pub running: bool,
    pub pid: Option<u32>,
    pub uptime_seconds: Option<u64>,
    pub restart_count: u32,
    pub health_status: HealthStatus,
    pub last_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConnectedMinersInfo {
    pub total_count: u32,
    pub active_count: u32,
    pub miners: Vec<MinerInfo>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MinerInfo {
    pub ip: String,
    pub connected_at: String,
    pub hashrate: Option<f64>,
    pub shares_submitted: u32,
    pub last_activity: String,
}

#[derive(Debug, Serialize)]
pub struct SystemInfo {
    pub bitcoin_network: String,
    pub bitcoin_blocks: Option<u64>,
    pub bitcoin_synced: Option<bool>,
    pub sv2_version: String,
    pub daemon_version: String,
}

#[derive(Debug, Deserialize)]
pub struct JsonRpcRequest {
    pub method: String,
    pub params: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct JsonRpcResponse {
    pub result: serde_json::Value,
}

async fn start_bitcoin_core(state: Arc<DaemonState>) -> Result<()> {
    info!("🟡 Starting Bitcoin Core with smart detection...");
    
    // Parse network from config
    let network = Network::from_str(&state.config.daemon.network)
        .context("Invalid network in config")?;
    
    // Use smart Bitcoin detection/startup
    let bitcoin_connection = ensure_bitcoin_running(network).await
        .context("Failed to ensure Bitcoin Core is running")?;
    
    info!("✅ Bitcoin Core ready:");
    info!("   Network: {}", bitcoin_connection.network);
    info!("   Port: {}", bitcoin_connection.port);
    info!("   Blocks: {}", bitcoin_connection.block_count);
    info!("   Synced: {}", bitcoin_connection.synced);
    info!("   Auto-detected: {}", bitcoin_connection.detected_existing);
    
    // Update component status
    state.update_component_status("bitcoin", true, None).await;
    
    Ok(())
}

async fn start_sv2_tp(state: Arc<DaemonState>) -> Result<()> {
    info!("🟡 Starting sv2-tp...");
    
    let network = &state.config.daemon.network;
    let datadir = format!("/tmp/bitcoin_{}", network);
    
    // Determine correct sv2-tp port based on network
    let sv2_port = match network.as_str() {
        "regtest" => 18447,
        "signet" => 38336, 
        "mainnet" => 8336,
        _ => 38336, // default to signet port
    };
    
    // Open log files
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/sv2d-sv2-tp.log")
        .context("Failed to open sv2-tp log file")?;

    let child = TokioCommand::new("./sv2-tp-1.0.2/bin/sv2-tp")
        .arg(format!("-chain={}", network))
        .arg(format!("-datadir={}", datadir))
        .arg(format!("-sv2port={}", sv2_port))
        .arg("-debug=sv2")
        .arg("-loglevel=sv2:trace")
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .context("Failed to start sv2-tp")?;
    
    let pid = child.id();
    info!("Started sv2-tp with PID: {:?} on port {}", pid, sv2_port);
    
    // Wait for it to be ready (check for listening port)
    // sv2-tp needs to connect to Bitcoin IPC first, which can take 30-60 seconds
    for i in 0..30 {
        sleep(Duration::from_secs(2)).await;
        if test_tcp_port(sv2_port).await {
            info!("✅ sv2-tp ready on port {}", sv2_port);
            state.update_component_status("sv2-tp", true, pid).await;

            let mut processes = state.processes.write().await;
            processes.insert("sv2-tp".to_string(), child);

            return Ok(());
        }
        if i % 5 == 0 {
            info!("Waiting for sv2-tp (connecting to Bitcoin IPC)... ({}/30)", i + 1);
        }
    }

    Err(anyhow::anyhow!("sv2-tp failed to start within 60 seconds - check that Bitcoin Core IPC is ready"))
}

async fn start_pool(state: Arc<DaemonState>) -> Result<()> {
    info!("🟡 Starting SRI Pool...");
    
    // Generate pool config
    let network = &state.config.daemon.network;
    let tp_port = match network.as_str() {
        "regtest" => 18447,
        "signet" => 38336, 
        "mainnet" => 8336,
        _ => 38336, // default to signet port
    };
    
    let pool_config = format!(
        r#"# SRI Pool config for {}
authority_public_key = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
authority_secret_key = "mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n"
cert_validity_sec = 3600
test_only_listen_adress_plain = "0.0.0.0:34250"
listen_address = "0.0.0.0:34254"

# Mining address
coinbase_reward_script = "addr({})"

# Server Id
server_id = 1

# Pool signature
pool_signature = "{}"

# Template Provider config
tp_address = "127.0.0.1:{}"
shares_per_minute = 1.0
share_batch_size = 10
"#,
        network, state.config.pool.coinbase_address, state.config.pool.signature, tp_port
    );
    
    let config_path = format!("/tmp/pool_{}.toml", network);
    fs::write(&config_path, pool_config)?;

    // Open log files
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/sv2d-pool.log")
        .context("Failed to open pool log file")?;

    let child = TokioCommand::new("./stratum-reference/roles/target/debug/pool_sv2")
        .arg("--config")
        .arg("./config/sri_pool_regtest.WORKING.toml")
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .context("Failed to start SRI Pool")?;
    
    let pid = child.id();
    info!("Started SRI Pool with PID: {:?}", pid);
    
    // Wait for it to be ready with improved timing
    for i in 0..15 {
        sleep(Duration::from_secs(2)).await;
        if test_tcp_port(34254).await {
            info!("✅ SRI Pool ready on port 34254");
            state.update_component_status("pool", true, pid).await;
            
            let mut processes = state.processes.write().await;
            processes.insert("pool".to_string(), child);
            
            return Ok(());
        }
        if i % 3 == 0 {
            info!("Waiting for SRI Pool to start... ({}/15) - checking port 34254", i + 1);
        }
    }
    
    Err(anyhow::anyhow!("SRI Pool failed to start within 30 seconds"))
}

async fn start_translator(state: Arc<DaemonState>) -> Result<()> {
    info!("🟡 Starting SRI Translator...");
    
    // Generate translator config based on our working config
    let translator_config = format!(
        r#"# SRI Translator Configuration for Multi-miner Support
downstream_address = "0.0.0.0"
downstream_port = 3333

# Version support
max_supported_version = 2
min_supported_version = 2

# Extranonce2 size for multi-miner compatibility
downstream_extranonce2_size = {}

# User identity for pool connection
user_identity = "sv2d_miner"

# Aggregate channels recommended for small miners
aggregate_channels = true

# Difficulty params optimized for multi-miner
[downstream_difficulty_config]
min_individual_miner_hashrate = 500000000000.0  # 0.5 TH/s - proper for Bitaxe
shares_per_minute = 5.0  # Optimal feedback frequency for small ASIC miners
enable_vardiff = true  # Enable for proper difficulty adjustment

# Upstream pool connection
[[upstreams]]
address = "127.0.0.1"
port = 34254
authority_pubkey = "9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72"
"#,
        state.config.translator.min_extranonce2_size
    );
    
    let config_path = "/tmp/translator_sv2d.toml";
    fs::write(config_path, translator_config)?;

    // Open log files
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("/tmp/sv2d-translator.log")
        .context("Failed to open translator log file")?;

    let child = TokioCommand::new("./stratum-reference/roles/target/debug/translator_sv2")
        .arg("--config")
        .arg("./config/translator_config.WORKING.toml")
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .context("Failed to start SRI Translator")?;
    
    let pid = child.id();
    info!("Started SRI Translator with PID: {:?}", pid);
    
    // Wait for it to be ready
    for i in 0..15 {
        sleep(Duration::from_secs(2)).await;
        if test_tcp_port(3333).await {
            info!("✅ SRI Translator ready");
            state.update_component_status("translator", true, pid).await;
            
            let mut processes = state.processes.write().await;
            processes.insert("translator".to_string(), child);
            
            return Ok(());
        }
        if i % 3 == 0 {
            info!("Waiting for SRI Translator to start... ({}/15)", i + 1);
        }
    }
    
    Err(anyhow::anyhow!("SRI Translator failed to start within 30 seconds"))
}

async fn test_bitcoin_rpc(rpc_url: &str) -> Result<()> {
    let client = reqwest::Client::new();
    let response = client
        .post(rpc_url)
        .json(&serde_json::json!({
            "method": "getblockchaininfo",
            "params": [],
            "id": 1
        }))
        .timeout(Duration::from_secs(5))
        .send()
        .await?;
    
    if response.status().is_success() {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Bitcoin RPC not ready"))
    }
}

async fn test_tcp_port(port: u16) -> bool {
    // Try to connect to the port rather than bind to it
    // This is more reliable than trying to bind
    match tokio::net::TcpStream::connect(format!("127.0.0.1:{}", port)).await {
        Ok(_) => true,  // Connection successful - service is running
        Err(_) => false, // Connection failed - service not ready
    }
}

async fn detect_connected_miners(state: Arc<DaemonState>) -> Result<()> {
    // Use netstat/lsof to detect active connections to port 3333
    let output = TokioCommand::new("lsof")
        .args(&["-i", ":3333", "-n"])
        .output()
        .await?;
    
    let output_str = String::from_utf8_lossy(&output.stdout);
    let mut current_miners = HashMap::new();
    
    for line in output_str.lines() {
        if line.contains("ESTABLISHED") && line.contains("->") {
            // Parse line like: "translato 13380 munje   11u  IPv4 0xfddf66c2d589e155      0t0  TCP 10.0.0.3:dec-notes->10.0.0.2:65105 (ESTABLISHED)"
            if let Some(connection_part) = line.split("->").nth(1) {
                if let Some(ip) = connection_part.split(":").next() {
                    let miner_info = MinerInfo {
                        ip: ip.to_string(),
                        connected_at: std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap_or_default()
                            .as_secs()
                            .to_string(),
                        hashrate: None, // We can't detect hashrate from network connections alone
                        shares_submitted: 0,
                        last_activity: "Active".to_string(),
                    };
                    current_miners.insert(ip.to_string(), miner_info);
                }
            }
        }
    }
    
    // Update the connected miners
    let mut miners = state.connected_miners.write().await;
    *miners = current_miners;
    
    Ok(())
}

async fn generate_enhanced_status(state: Arc<DaemonState>) -> Result<StatusResponse> {
    // First, detect any connected miners
    let _ = detect_connected_miners(Arc::clone(&state)).await;
    
    let components = state.components.read().await;
    let miners = state.connected_miners.read().await;
    let now = std::time::Instant::now();
    
    // Build component status info
    let mut component_info = HashMap::new();
    for (name, status) in components.iter() {
        let uptime_seconds = status.start_time.map(|start| now.duration_since(start).as_secs());
        
        component_info.insert(name.clone(), ComponentStatusInfo {
            running: status.running,
            pid: status.pid,
            uptime_seconds,
            restart_count: status.restart_count,
            health_status: status.health_status.clone(),
            last_error: status.last_error.clone(),
        });
    }
    
    // Check if all components are running
    let running = components.values().all(|c| c.running);
    
    // Build miners info
    let active_miners: Vec<_> = miners.values().cloned().collect();
    let miners_info = ConnectedMinersInfo {
        total_count: active_miners.len() as u32,
        active_count: active_miners.iter().filter(|m| m.hashrate.is_some()).count() as u32,
        miners: active_miners,
    };
    
    // Get Bitcoin network info
    let system_info = get_system_info(Arc::clone(&state)).await;
    
    // Calculate daemon uptime
    let uptime_seconds = now.duration_since(state.start_time).as_secs();
    
    Ok(StatusResponse {
        running,
        components: component_info,
        miners: miners_info,
        system_info,
        uptime_seconds,
    })
}

async fn get_system_info(state: Arc<DaemonState>) -> SystemInfo {
    let mut bitcoin_blocks = None;
    let mut bitcoin_synced = None;
    
    // Try to get Bitcoin info if Bitcoin is running
    if let Ok(_response) = test_bitcoin_rpc(&state.config.bitcoin.rpc_url).await {
        // Try to get blockchain info for more details
        if let Ok(client) = reqwest::Client::new()
            .post(&state.config.bitcoin.rpc_url)
            .json(&serde_json::json!({
                "method": "getblockchaininfo",
                "params": [],
                "id": 1
            }))
            .timeout(Duration::from_secs(5))
            .send()
            .await
        {
            if let Ok(json) = client.json::<serde_json::Value>().await {
                if let Some(result) = json.get("result") {
                    bitcoin_blocks = result.get("blocks").and_then(|v| v.as_u64());
                    bitcoin_synced = result.get("initialblockdownload")
                        .and_then(|v| v.as_bool())
                        .map(|ibd| !ibd);
                }
            }
        }
    }
    
    SystemInfo {
        bitcoin_network: state.config.daemon.network.clone(),
        bitcoin_blocks,
        bitcoin_synced,
        sv2_version: "SRI v1.0.2".to_string(),
        daemon_version: "sv2d v0.1.0".to_string(),
    }
}

async fn start_all_components(state: Arc<DaemonState>) -> Result<()> {
    info!("🚀 Starting all components...");
    
    // Start in order: Bitcoin -> sv2-tp -> Pool -> Translator
    start_bitcoin_core(Arc::clone(&state)).await?;
    start_sv2_tp(Arc::clone(&state)).await?;
    start_pool(Arc::clone(&state)).await?;
    start_translator(Arc::clone(&state)).await?;
    
    info!("✅ All components started successfully!");
    Ok(())
}

async fn stop_all_components(state: Arc<DaemonState>) -> Result<()> {
    info!("🛑 Stopping all components...");
    
    let mut processes = state.processes.write().await;
    
    // Stop in reverse order
    for component in ["translator", "pool", "sv2-tp", "bitcoin"] {
        if let Some(mut child) = processes.remove(component) {
            info!("Stopping {}...", component);
            if let Err(e) = child.kill().await {
                warn!("Failed to stop {}: {}", component, e);
            }
            state.update_component_status(component, false, None).await;
        }
    }
    
    info!("✅ All components stopped");
    Ok(())
}

async fn handle_json_rpc(
    request: JsonRpcRequest,
    state: Arc<DaemonState>,
) -> Result<JsonRpcResponse> {
    match request.method.as_str() {
        "start" => {
            start_all_components(state).await?;
            Ok(JsonRpcResponse {
                result: serde_json::json!({
                    "status": "started",
                    "components": ["bitcoin", "sv2-tp", "pool", "translator"]
                }),
            })
        }
        "stop" => {
            stop_all_components(state).await?;
            Ok(JsonRpcResponse {
                result: serde_json::json!({"status": "stopped"}),
            })
        }
        "status" => {
            let status_response = generate_enhanced_status(state).await?;
            Ok(JsonRpcResponse {
                result: serde_json::json!(status_response),
            })
        }
        _ => Err(anyhow::anyhow!("Unknown method: {}", request.method)),
    }
}

async fn run_json_rpc_server(state: Arc<DaemonState>) -> Result<()> {
    use hyper::service::{make_service_fn, service_fn};
    use hyper::{Body, Request, Response, Server};
    use std::convert::Infallible;
    
    let make_svc = make_service_fn(move |_conn| {
        let state = Arc::clone(&state);
        async move {
            Ok::<_, Infallible>(service_fn(move |req: Request<Body>| {
                let state = Arc::clone(&state);
                async move {
                    let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
                    let request: JsonRpcRequest = serde_json::from_slice(&body_bytes)?;
                    
                    let response = handle_json_rpc(request, state).await
                        .unwrap_or_else(|e| JsonRpcResponse {
                            result: serde_json::json!({"error": e.to_string()}),
                        });
                    
                    let response_json = serde_json::to_string(&response)?;
                    
                    Ok::<_, anyhow::Error>(Response::new(Body::from(response_json)))
                }
            }))
        }
    });
    
    let addr = ([127, 0, 0, 1], 8333).into();
    let server = Server::bind(&addr).serve(make_svc);
    
    info!("JSON-RPC server listening on http://127.0.0.1:8333");
    
    if let Err(e) = server.await {
        error!("Server error: {}", e);
    }
    
    Ok(())
}

fn load_config() -> Result<DaemonConfig> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let config_dir = PathBuf::from(home).join(".sv2d");
    let config_path = config_dir.join("config.toml");
    
    if !config_path.exists() {
        return Err(anyhow::anyhow!(
            "Config file not found at {}. Run 'sv2-cli setup' first.",
            config_path.display()
        ));
    }
    
    let config_content = fs::read_to_string(&config_path)
        .context("Failed to read config file")?;
    
    let config: DaemonConfig = toml::from_str(&config_content)
        .context("Failed to parse config file")?;
    
    Ok(config)
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    
    let _matches = Command::new("sv2d")
        .version("0.1.0")
        .about("Stratum V2 daemon")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file path")
        )
        .get_matches();
    
    // Load configuration
    let config = load_config()?;
    info!("Loaded config for network: {}", config.daemon.network);
    
    // Create daemon state
    let state = Arc::new(DaemonState::new(config));

    // Start all components automatically
    let component_state = Arc::clone(&state);
    tokio::spawn(async move {
        if let Err(e) = start_all_components(component_state).await {
            error!("Failed to start components: {}", e);
            std::process::exit(1);
        }
    });

    // Start JSON-RPC server
    let server_state = Arc::clone(&state);
    let server_handle = tokio::spawn(async move {
        if let Err(e) = run_json_rpc_server(server_state).await {
            error!("JSON-RPC server error: {}", e);
        }
    });

    // Handle shutdown signals
    let shutdown_state = Arc::clone(&state);
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
        info!("Received shutdown signal");
        let _ = stop_all_components(shutdown_state).await;
        std::process::exit(0);
    });
    
    // Wait for server
    server_handle.await?;
    
    Ok(())
}