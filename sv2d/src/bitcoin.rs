use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fmt;
use std::str::FromStr;
use std::time::Duration;
use tokio::process::{Child, Command};
use tokio::time::sleep;
use tracing::{info, warn};

#[derive(Debug, Clone, PartialEq)]
pub enum Network {
    Regtest,
    Signet,
    Mainnet,
}

impl Network {
    pub fn rpc_port(&self) -> u16 {
        match self {
            Network::Regtest => 18443,
            Network::Signet => 38332,
            Network::Mainnet => 8332,
        }
    }

    pub fn name(&self) -> &'static str {
        match self {
            Network::Regtest => "regtest",
            Network::Signet => "signet",
            Network::Mainnet => "mainnet",
        }
    }
}

impl FromStr for Network {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "regtest" => Ok(Network::Regtest),
            "signet" => Ok(Network::Signet),
            "main" => Ok(Network::Mainnet),
            _ => Err(anyhow::anyhow!("Unknown network: {}", s)),
        }
    }
}

impl fmt::Display for Network {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[derive(Debug, Clone)]
pub struct BitcoinConnection {
    pub port: u16,
    pub network: Network,
    pub block_count: u64,
    pub synced: bool,
    pub detected_existing: bool,
}

#[derive(Debug, Deserialize)]
struct RpcResponse {
    result: Option<serde_json::Value>,
    error: Option<serde_json::Value>,
}

pub async fn ensure_bitcoin_running(network: Network) -> Result<BitcoinConnection> {
    let port = network.rpc_port();
    
    info!("🔍 Checking for Bitcoin Core on port {}...", port);
    
    // Try to connect to existing instance
    if let Ok(connection) = test_connection(port, true).await {
        info!("✅ Found existing Bitcoin Core");
        info!("   Network: {}", connection.network);
        info!("   Blocks: {}", connection.block_count);
        info!("   Synced: {}", connection.synced);
        
        // Verify it's the right network
        if connection.network == network {
            return Ok(connection);
        } else {
            return Err(anyhow::anyhow!(
                "Bitcoin Core is running on {} but config expects {}",
                connection.network, network
            ));
        }
    }
    
    // Not running, need to start it
    info!("⚙️  Starting Bitcoin Core...");
    start_bitcoin_core(network).await
}

async fn test_connection(port: u16, is_existing: bool) -> Result<BitcoinConnection> {
    let client = Client::new();
    
    let response = client
        .post(format!("http://127.0.0.1:{}", port))
        .basic_auth("test", Some("test"))
        .json(&json!({
            "jsonrpc": "1.0",
            "id": "test",
            "method": "getblockchaininfo",
            "params": []
        }))
        .timeout(Duration::from_secs(3))
        .send()
        .await?;
    
    let rpc_response: RpcResponse = response.json().await?;
    
    if let Some(error) = rpc_response.error {
        return Err(anyhow::anyhow!("RPC error: {}", error));
    }
    
    let result = rpc_response.result
        .ok_or_else(|| anyhow::anyhow!("No result in RPC response"))?;
    
    let result_obj = result.as_object()
        .ok_or_else(|| anyhow::anyhow!("Invalid response format"))?;
    
    let network_str = result_obj["chain"].as_str()
        .ok_or_else(|| anyhow::anyhow!("Missing chain field"))?;
    
    let network = Network::from_str(network_str)?;
    let block_count = result_obj["blocks"].as_u64().unwrap_or(0);
    let synced = !result_obj["initialblockdownload"].as_bool().unwrap_or(true);
    
    Ok(BitcoinConnection {
        port,
        network,
        block_count,
        synced,
        detected_existing: is_existing,
    })
}

async fn start_bitcoin_core(network: Network) -> Result<BitcoinConnection> {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    let bitcoin_path = format!("{}/Downloads/bitcoin-30.0/bin/bitcoin", home);
    let datadir = format!("/tmp/bitcoin_{}", network.name());
    
    // Create datadir if needed
    std::fs::create_dir_all(&datadir)
        .context("Failed to create Bitcoin datadir")?;
    
    // Write bitcoin.conf
    let bitcoin_conf = match network {
        Network::Regtest => format!(
            "regtest=1\nrpcuser=test\nrpcpassword=test\nzmqpubhashblock=tcp://127.0.0.1:28332\nzmqpubrawtx=tcp://127.0.0.1:28333\nfallbackfee=0.0002\n\n[regtest]\nrpcport={}\n",
            network.rpc_port()
        ),
        Network::Signet => format!(
            "signet=1\nrpcuser=test\nrpcpassword=test\nfallbackfee=0.0002\n\n[signet]\nrpcport={}\n",
            network.rpc_port()
        ),
        Network::Mainnet => format!(
            "rpcuser=test\nrpcpassword=test\nfallbackfee=0.0002\nrpcport={}\n",
            network.rpc_port()
        ),
    };
    
    let conf_path = format!("{}/bitcoin.conf", datadir);
    std::fs::write(&conf_path, bitcoin_conf)
        .context("Failed to write bitcoin.conf")?;
    
    let mut args = vec![
        "-m".to_string(),
        "node".to_string(),
        "-ipcbind=unix".to_string(),
        format!("-datadir={}", datadir),
        "-daemon".to_string(),
    ];
    
    info!("Starting Bitcoin Core with: {} {}", bitcoin_path, args.join(" "));
    
    let mut child = Command::new(&bitcoin_path)
        .args(&args)
        .spawn()
        .context("Failed to start Bitcoin Core")?;
    
    // Give it some time to start
    sleep(Duration::from_secs(5)).await;

    // Check if it's still running
    match child.try_wait()? {
        Some(status) if !status.success() => {
            return Err(anyhow::anyhow!("Bitcoin Core failed to start with status: {}", status));
        }
        _ => {} // Still running or no status yet
    }

    // Wait for IPC socket to be created (critical for sv2-tp)
    let ipc_socket_path = format!("{}/{}/node.sock", datadir, network.name());
    info!("Waiting for Bitcoin IPC socket at {}...", ipc_socket_path);
    for i in 0..30 {
        if std::path::Path::new(&ipc_socket_path).exists() {
            info!("✅ Bitcoin IPC socket ready");
            break;
        }
        if i % 5 == 0 {
            info!("Waiting for IPC socket... ({}/30)", i + 1);
        }
        sleep(Duration::from_secs(2)).await;
    }

    // Wait for it to be ready (with timeout)
    let port = network.rpc_port();
    for i in 0..30 {
        if let Ok(connection) = test_connection(port, false).await {
            // Generate initial block to exit IBD immediately
            info!("Generating initial block to exit IBD...");
            let client = Client::new();
            let rpc_url = format!("http://127.0.0.1:{}", port);
            let _ = client
                .post(&rpc_url)
                .basic_auth("test", Some("test"))
                .json(&serde_json::json!({
                    "method": "generatetoaddress",
                    "params": [1, "bcrt1qe8le5cgtujqrx9r85e8q4r6zjy4c227zhgtyea"],
                    "id": 1
                }))
                .send()
                .await;
            info!("✅ Bitcoin Core started successfully and exited IBD");
            return Ok(connection);
        }

        if i % 5 == 0 {
            info!("Waiting for Bitcoin Core RPC... ({}/30)", i + 1);
        }

        sleep(Duration::from_secs(2)).await;
    }
    
    Err(anyhow::anyhow!("Bitcoin Core failed to become ready within 60 seconds"))
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_network_from_str() {
        assert_eq!(Network::from_str("regtest").unwrap(), Network::Regtest);
        assert_eq!(Network::from_str("signet").unwrap(), Network::Signet);
        assert_eq!(Network::from_str("main").unwrap(), Network::Mainnet);
        assert!(Network::from_str("invalid").is_err());
    }
    
    #[test]
    fn test_network_rpc_port() {
        assert_eq!(Network::Regtest.rpc_port(), 18443);
        assert_eq!(Network::Signet.rpc_port(), 38332);
        assert_eq!(Network::Mainnet.rpc_port(), 8332);
    }
}