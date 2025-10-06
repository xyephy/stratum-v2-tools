use clap::{Arg, Command};
use serde::{Deserialize, Serialize};
use std::fs;
use tracing::{info, error};

#[derive(Debug, Deserialize, Serialize)]
struct Config {
    mode: ModeConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct ModeConfig {
    #[serde(rename = "type")]
    mode_type: String,
    config: ProxyConfig,
}

#[derive(Debug, Deserialize, Serialize)]
struct ProxyConfig {
    bind_port: u16,
    upstream_address: String,
    upstream_port: u16,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    let matches = Command::new("sv2d")
        .version("0.1.0")
        .about("Stratum V2 daemon")
        .arg(
            Arg::new("config")
                .short('c')
                .long("config")
                .value_name("FILE")
                .help("Configuration file path")
                .default_value("sv2d.toml")
        )
        .get_matches();

    let config_path = matches.get_one::<String>("config").unwrap();
    
    // Load configuration
    let config_content = fs::read_to_string(config_path)
        .map_err(|e| format!("Failed to read config file: {}", e))?;
    
    let config: Config = toml::from_str(&config_content)
        .map_err(|e| format!("Failed to parse config: {}", e))?;

    // Only support proxy mode for now
    if config.mode.mode_type != "Proxy" {
        error!("Only proxy mode is currently supported");
        std::process::exit(1);
    }

    let proxy_config = &config.mode.config;
    
    info!("Starting sv2d in proxy mode");
    
    // Run SRI Translator directly
    let translator_path = "/Users/munje/dawn/stratum-v2-tools/stratum-reference/roles/target/debug/translator_sv2";
    
    // Create translator config
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
        proxy_config.bind_port,
        proxy_config.upstream_address,
        proxy_config.upstream_port
    );

    fs::write("/tmp/translator_config.toml", config_content)
        .map_err(|e| format!("Failed to write translator config: {}", e))?;
    
    info!("Starting SRI Translator on port {}", proxy_config.bind_port);
    info!("Connecting to upstream pool at {}:{}", proxy_config.upstream_address, proxy_config.upstream_port);

    let mut child = tokio::process::Command::new(translator_path)
        .arg("-c")
        .arg("/tmp/translator_config.toml")
        .spawn()
        .map_err(|e| format!("Failed to start SRI Translator: {}", e))?;

    // Wait for the translator to finish
    let status = child.wait().await
        .map_err(|e| format!("SRI Translator error: {}", e))?;

    if !status.success() {
        error!("SRI Translator exited with error");
        std::process::exit(1);
    }

    Ok(())
}