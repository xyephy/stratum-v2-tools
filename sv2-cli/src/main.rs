use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::Duration;

mod scanner;
use scanner::{NetworkScanner, generate_config_recommendations};

#[derive(Parser)]
#[command(name = "sv2-cli")]
#[command(version = "0.1.0")]
#[command(about = "Stratum V2 command-line interface")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard
    Setup,
    
    /// Scan network for miners
    Scan {
        /// Custom subnet to scan (e.g., 192.168.1.0/24)
        #[arg(short, long)]
        subnet: Option<Vec<String>>,
        
        /// Save detected miners to file
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    
    /// Start the daemon
    Start,
    
    /// Stop the daemon
    Stop,
    
    /// Get daemon status
    Status,
    
    /// Show daemon logs
    Logs {
        /// Follow logs in real-time
        #[arg(short, long)]
        follow: bool,
    },
}

#[derive(Debug, Serialize)]
struct JsonRpcRequest {
    method: String,
    params: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct JsonRpcResponse {
    result: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct StatusResponse {
    running: bool,
    components: std::collections::HashMap<String, ComponentStatusInfo>,
    miners: ConnectedMinersInfo,
    system_info: SystemInfo,
    uptime_seconds: u64,
}

#[derive(Debug, Deserialize)]
struct ComponentStatusInfo {
    running: bool,
    pid: Option<u32>,
    uptime_seconds: Option<u64>,
    restart_count: u32,
    health_status: String,
    last_error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ConnectedMinersInfo {
    total_count: u32,
    active_count: u32,
    miners: Vec<MinerInfo>,
}

#[derive(Debug, Deserialize)]
struct MinerInfo {
    ip: String,
    connected_at: String,
    hashrate: Option<f64>,
    shares_submitted: u32,
    last_activity: String,
}

#[derive(Debug, Deserialize)]
struct SystemInfo {
    bitcoin_network: String,
    bitcoin_blocks: Option<u64>,
    bitcoin_synced: Option<bool>,
    sv2_version: String,
    daemon_version: String,
}

async fn send_rpc_request(method: &str, params: serde_json::Value) -> Result<serde_json::Value> {
    let client = Client::new();
    
    let request = JsonRpcRequest {
        method: method.to_string(),
        params,
    };
    
    let response = client
        .post("http://127.0.0.1:8333")
        .json(&request)
        .timeout(Duration::from_secs(30))
        .send()
        .await
        .context("Failed to connect to sv2d daemon. Is it running?")?;
    
    if response.status().is_success() {
        let rpc_response: JsonRpcResponse = response.json().await?;
        Ok(rpc_response.result)
    } else {
        Err(anyhow::anyhow!("RPC request failed: {}", response.status()))
    }
}

async fn check_daemon_running() -> bool {
    send_rpc_request("status", json!({})).await.is_ok()
}

async fn start_daemon() -> Result<()> {
    // Check if daemon is already running
    if check_daemon_running().await {
        println!("✅ sv2d daemon is already running");
        return Ok(());
    }
    
    println!("🚀 Starting sv2d daemon...");

    // Get config path
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let config_path = format!("{}/.sv2d/config.toml", home);

    // Start daemon in background - redirect to log file to avoid pipe blocking
    let log_file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(format!("{}/.sv2d/sv2d.log", home))
        .context("Failed to open sv2d log file")?;

    let mut child = Command::new("./target/release/sv2d")
        .arg("--config")
        .arg(&config_path)
        .stdout(Stdio::from(log_file.try_clone()?))
        .stderr(Stdio::from(log_file))
        .spawn()
        .context("Failed to start sv2d daemon. Run 'cargo build --release' first.")?;
    
    // Give it a moment to start
    std::thread::sleep(Duration::from_secs(2));
    
    // Check if it's still running
    match child.try_wait()? {
        Some(status) => {
            if !status.success() {
                return Err(anyhow::anyhow!("sv2d daemon failed to start"));
            }
        }
        None => {
            // Still running, good
            println!("✅ sv2d daemon started in background");
        }
    }
    
    Ok(())
}

async fn handle_start() -> Result<()> {
    // First start the daemon process if needed
    start_daemon().await?;

    // Wait a bit for daemon to be ready
    for i in 0..10 {
        if check_daemon_running().await {
            break;
        }
        if i == 9 {
            return Err(anyhow::anyhow!("Daemon failed to become ready"));
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }

    // sv2d now auto-starts components, so just confirm they're running
    println!("✅ sv2d daemon started - components will auto-start");
    println!("💡 Use 'sv2-cli status' to check component status");
    Ok(())
}

async fn handle_stop() -> Result<()> {
    if !check_daemon_running().await {
        println!("❌ sv2d daemon is not running");
        return Ok(());
    }
    
    println!("🛑 Stopping mining components...");
    let result = send_rpc_request("stop", json!({})).await?;
    println!("✅ {}", result);
    Ok(())
}

async fn handle_status() -> Result<()> {
    if !check_daemon_running().await {
        println!("❌ sv2d daemon is not running");
        return Ok(());
    }
    
    let result = send_rpc_request("status", json!({})).await?;
    let status: StatusResponse = serde_json::from_value(result)?;
    
    // Header
    println!("📊 SV2 Daemon Status");
    println!("{:=<80}", "");
    println!("Overall Status: {}", if status.running { "✅ Running" } else { "❌ Stopped" });
    println!("Daemon Uptime: {}", format_duration(status.uptime_seconds));
    println!();
    
    // System Information
    println!("🖥  System Information:");
    println!("   Network: {}", status.system_info.bitcoin_network);
    println!("   Daemon: {}", status.system_info.daemon_version);
    println!("   SV2 Implementation: {}", status.system_info.sv2_version);
    
    if let Some(blocks) = status.system_info.bitcoin_blocks {
        println!("   Bitcoin Blocks: {}", blocks);
    }
    if let Some(synced) = status.system_info.bitcoin_synced {
        println!("   Bitcoin Synced: {}", if synced { "✅ Yes" } else { "⏳ Syncing" });
    }
    println!();
    
    // Components
    println!("🔧 Components:");
    for (name, component) in &status.components {
        let status_icon = match component.health_status.as_str() {
            "Healthy" => "✅",
            "Warning" => "⚠️",
            "Critical" => "❌",
            _ => "❓",
        };
        
        print!("   {} {}", status_icon, name);
        
        if let Some(pid) = component.pid {
            print!(" (PID: {})", pid);
        }
        
        if let Some(uptime) = component.uptime_seconds {
            print!(" - Up: {}", format_duration(uptime));
        }
        
        if component.restart_count > 0 {
            print!(" - Restarts: {}", component.restart_count);
        }
        
        println!();
        
        if let Some(error) = &component.last_error {
            println!("      Last error: {}", error);
        }
    }
    println!();
    
    // Miners
    println!("⛏  Connected Miners:");
    if status.miners.total_count == 0 {
        println!("   No miners connected");
        println!("   💡 Point your miners to YOUR_IP:3333");
    } else {
        println!("   Total: {} | Active: {}", status.miners.total_count, status.miners.active_count);
        
        for (i, miner) in status.miners.miners.iter().enumerate() {
            print!("   {}. {}", i + 1, miner.ip);
            
            if let Some(hashrate) = miner.hashrate {
                print!(" - {:.2} GH/s", hashrate / 1e9);
            }
            
            print!(" - {} shares", miner.shares_submitted);
            println!(" - Connected: {}", miner.connected_at);
        }
    }
    
    Ok(())
}

fn format_duration(seconds: u64) -> String {
    let days = seconds / 86400;
    let hours = (seconds % 86400) / 3600;
    let minutes = (seconds % 3600) / 60;
    let secs = seconds % 60;
    
    if days > 0 {
        format!("{}d {}h {}m", days, hours, minutes)
    } else if hours > 0 {
        format!("{}h {}m", hours, minutes)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, secs)
    } else {
        format!("{}s", secs)
    }
}

async fn handle_logs(follow: bool) -> Result<()> {
    // For now, just show that logs would be here
    // In a full implementation, we'd tail the daemon log file
    println!("📝 Daemon logs:");
    
    if follow {
        println!("Following logs... (Ctrl+C to exit)");
        // TODO: Implement real log following
        loop {
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    } else {
        println!("Log file location: ~/.sv2d/sv2d.log");
        println!("Use 'sv2-cli logs --follow' to follow in real-time");
    }
    
    Ok(())
}

fn create_config_dir() -> Result<PathBuf> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let config_dir = PathBuf::from(home).join(".sv2d");
    
    if !config_dir.exists() {
        fs::create_dir_all(&config_dir)
            .context("Failed to create ~/.sv2d directory")?;
    }
    
    Ok(config_dir)
}

async fn handle_scan(subnets: Option<Vec<String>>, output: Option<PathBuf>) -> Result<()> {
    println!("🔍 Scanning network for miners...");
    
    let scanner = NetworkScanner::new();
    let miners = scanner.scan_network(subnets).await?;
    
    if miners.is_empty() {
        println!("❌ No miners detected on the network");
        println!("   • Make sure miners are powered on and connected");
        println!("   • Check that you're on the same network");
        println!("   • Try specifying different subnets with --subnet");
        return Ok(());
    }
    
    println!("\n✅ Found {} miner(s):", miners.len());
    println!("{:-<80}", "");
    
    for (i, miner) in miners.iter().enumerate() {
        println!("{}. {} at {}", i + 1, miner.miner_type, miner.ip);
        if let Some(port) = miner.api_port {
            println!("   API Port: {}", port);
        }
        println!("   Response time: {}ms", miner.response_time_ms);
        
        if let Some(hostname) = &miner.details.hostname {
            println!("   Hostname: {}", hostname);
        }
        if let Some(hashrate) = miner.details.hashrate {
            println!("   Hashrate: {:.2} GH/s", hashrate / 1e9);
        }
        if let Some(temp) = miner.details.temperature {
            println!("   Temperature: {:.1}°C", temp);
        }
        if let Some(pool) = &miner.details.pool_url {
            println!("   Current pool: {}", pool);
        }
        if let Some(worker) = &miner.details.worker_name {
            println!("   Worker name: {}", worker);
        }
        println!();
    }
    
    // Generate configuration recommendations
    let recommendations = generate_config_recommendations(&miners);
    
    println!("📋 Configuration Recommendations:");
    println!("{:-<80}", "");
    
    if let Some(extranonce2) = recommendations.get("extranonce2_size") {
        println!("• Recommended extranonce2_size: {}", extranonce2);
    }
    
    if let Some(hashrate) = recommendations.get("total_hashrate") {
        println!("• Total estimated hashrate: {:.2} GH/s", hashrate.as_f64().unwrap_or(0.0) / 1e9);
    }
    
    if let Some(shares) = recommendations.get("shares_per_minute") {
        println!("• Recommended shares_per_minute: {}", shares);
    }
    
    if let Some(detected) = recommendations.get("detected_miners") {
        println!("• Detected miner types: {}", detected);
    }
    
    // Save to file if requested
    if let Some(output_path) = output {
        let scan_results = serde_json::json!({
            "scan_time": chrono::Utc::now().to_rfc3339(),
            "miners": miners,
            "recommendations": recommendations
        });
        
        fs::write(&output_path, serde_json::to_string_pretty(&scan_results)?)?;
        println!("\n💾 Scan results saved to: {}", output_path.display());
    }
    
    println!("\n💡 Next steps:");
    println!("   1. Update your sv2-cli setup with these recommendations");
    println!("   2. Point miners to YOUR_IP:3333 when ready");
    println!("   3. Use 'sv2-cli status' to monitor connections");
    
    Ok(())
}

async fn handle_setup() -> Result<()> {
    println!("🎰 SV2 Solo Mining Setup Wizard\n");
    
    // Create config directory
    let config_dir = create_config_dir()?;
    let config_path = config_dir.join("config.toml");
    
    // Check if config already exists
    if config_path.exists() {
        println!("⚠️  Configuration already exists at {}", config_path.display());
        print!("Overwrite? (y/N): ");
        
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        
        if !input.trim().to_lowercase().starts_with('y') {
            println!("Setup cancelled.");
            return Ok(());
        }
    }
    
    // 1. Hardware selection
    println!("What hardware do you have?");
    println!("1) Bitaxe (~700 GH/s)");
    println!("2) FutureBit Apollo (~4.8 TH/s)");
    println!("3) Mixed or unknown");
    print!("Choice (1-3): ");
    
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    let hardware_choice = input.trim().parse::<u32>().unwrap_or(3);
    
    let min_extranonce2_size = match hardware_choice {
        1 => 4,  // Bitaxe can work with smaller
        2 => 16, // Apollo needs 16
        _ => 16, // Universal safe default
    };
    
    // 2. Network selection
    println!("\nWhich network?");
    println!("1) Regtest (testing, instant blocks)");
    println!("2) Signet (practice with free coins)");
    print!("Choice (1-2): ");
    
    input.clear();
    std::io::stdin().read_line(&mut input)?;
    let network_choice = input.trim().parse::<u32>().unwrap_or(2);
    
    let network = match network_choice {
        1 => "regtest",
        _ => "signet",
    };
    
    let rpc_url = match network {
        "regtest" => "http://127.0.0.1:18443",
        _ => "http://127.0.0.1:38332",
    };
    
    // 3. Mining address
    println!("\nWhat's your Bitcoin address?");
    println!("(Where block rewards go if you find a block)");
    
    let address_prefix = match network {
        "regtest" => "bcrt1",
        _ => "tb1", // signet uses testnet addresses
    };
    
    loop {
        print!("Address: ");
        input.clear();
        std::io::stdin().read_line(&mut input)?;
        let address = input.trim();
        
        if address.starts_with(address_prefix) {
            break;
        }
        
        println!("❌ Invalid address. Must start with {} for {}", address_prefix, network);
    }
    
    let mining_address = input.trim().to_string();
    
    // 4. Generate config
    let config = format!(
        r#"[daemon]
mode = "proxy"
network = "{}"

[bitcoin]
rpc_url = "{}"
rpc_user = "test"
rpc_password = "test"

[pool]
signature = "SV2"
coinbase_address = "{}"

[translator]
bind_address = "0.0.0.0:3333"
min_extranonce2_size = {}
"#,
        network, rpc_url, mining_address, min_extranonce2_size
    );
    
    // 5. Write config
    fs::write(&config_path, config)
        .context("Failed to write config file")?;
    
    // 6. Success message
    println!("\n✅ Setup complete!");
    println!("Config saved to: {}", config_path.display());
    println!("\nNext steps:");
    println!("  1. sv2-cli start");
    println!("  2. Point your miner to: YOUR_IP:3333");
    println!("\nMiner configuration:");
    println!("  Pool: YOUR_IP:3333");
    println!("  Worker: (any name)");
    println!("  Password: (empty)");
    
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    
    match cli.command {
        Commands::Setup => handle_setup().await,
        Commands::Scan { subnet, output } => handle_scan(subnet, output).await,
        Commands::Start => handle_start().await,
        Commands::Stop => handle_stop().await,
        Commands::Status => handle_status().await,
        Commands::Logs { follow } => handle_logs(follow).await,
    }
}