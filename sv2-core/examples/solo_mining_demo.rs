use sv2_core::{
    config::{DaemonConfig, SoloConfig, BitcoinConfig, BitcoinNetwork},
    database::DatabasePool,
    bitcoin_rpc::BitcoinRpcClient,
    modes::ModeHandlerFactory,
    mode::ModeHandler,
    types::{Connection, Share, Protocol},
};
use std::sync::Arc;
use std::net::SocketAddr;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing (simple console output)
    // tracing_subscriber::init(); // Commented out for now

    println!("Solo Mining Mode Demo");
    println!("====================");

    // Create configuration for solo mining
    let solo_config = SoloConfig {
        coinbase_address: "bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string(),
        block_template_refresh_interval: 30,
        enable_custom_templates: false,
        max_template_age: 300,
    };

    let bitcoin_config = BitcoinConfig {
        rpc_url: "http://127.0.0.1:18443".to_string(),
        rpc_user: "test".to_string(),
        rpc_password: "test".to_string(),
        network: BitcoinNetwork::Regtest,
        coinbase_address: Some("bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string()),
        block_template_timeout: 30,
    };

    let config = DaemonConfig {
        mode: sv2_core::config::OperationModeConfig::Solo(solo_config),
        bitcoin: bitcoin_config.clone(),
        ..Default::default()
    };

    println!("Configuration created for solo mining mode");

    // Create database connection (in-memory SQLite for demo)
    let database = Arc::new(DatabasePool::new("sqlite::memory:", 5).await?);
    database.migrate().await?;
    println!("Database initialized");

    // Create Bitcoin RPC client
    let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
    println!("Bitcoin RPC client created");

    // Create mode handler
    let handler = ModeHandlerFactory::create_handler(&config, bitcoin_client, database)?;
    println!("Solo mode handler created");

    // Simulate a miner connection
    let addr: SocketAddr = "127.0.0.1:3333".parse()?;
    let connection = Connection::new(addr, Protocol::Sv2);
    let connection_id = connection.id;
    
    println!("Simulating miner connection from {}", addr);
    
    // Handle the connection
    handler.handle_connection(connection).await?;
    println!("Connection handled successfully");

    // Try to get a work template
    match handler.get_work_template().await {
        Ok(template) => {
            println!("Work template generated:");
            println!("  Template ID: {}", template.id);
            println!("  Previous Hash: {}", template.previous_hash);
            println!("  Difficulty: {}", template.difficulty);
            println!("  Timestamp: {}", template.timestamp);
            println!("  Expires At: {}", template.expires_at);
            println!("  Transactions: {}", template.transactions.len());
        }
        Err(e) => {
            println!("Failed to generate work template (expected without Bitcoin node): {}", e);
        }
    }

    // Simulate share submission
    let share = Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0);
    println!("Simulating share submission:");
    println!("  Connection ID: {}", share.connection_id);
    println!("  Nonce: {}", share.nonce);
    println!("  Difficulty: {}", share.difficulty);

    match handler.process_share(share).await {
        Ok(result) => {
            println!("Share processed: {:?}", result);
        }
        Err(e) => {
            println!("Share processing failed (expected without work template): {}", e);
        }
    }

    // Get statistics
    match handler.get_statistics().await {
        Ok(stats) => {
            println!("Mining statistics:");
            println!("  Hashrate: {:.2} H/s", stats.hashrate);
            println!("  Shares per minute: {:.2}", stats.shares_per_minute);
            println!("  Acceptance rate: {:.2}%", stats.acceptance_rate);
            println!("  Efficiency: {:.2}%", stats.efficiency);
            println!("  Uptime: {:?}", stats.uptime);
        }
        Err(e) => {
            println!("Failed to get statistics: {}", e);
        }
    }

    // Handle disconnection
    handler.handle_disconnection(connection_id).await?;
    println!("Connection disconnected");

    println!("\nDemo completed successfully!");
    println!("Note: This demo runs without a real Bitcoin node, so some operations fail as expected.");
    println!("To run with a real Bitcoin node, ensure bitcoind is running on regtest with RPC enabled.");

    Ok(())
}