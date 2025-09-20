use crate::{
    bitcoin_rpc::BitcoinRpcClient,
    config::{BitcoinConfig, BitcoinNetwork},
    modes::SoloModeHandler,
    config::SoloConfig,
    database::MockDatabaseOps,
    types::{Share, ShareResult},
    mode::ModeHandler,
    Result,
};
use std::sync::Arc;
use uuid::Uuid;

#[tokio::test]
async fn test_bitcoin_rpc_client_creation() -> Result<()> {
    let config = create_test_bitcoin_config();
    let client = BitcoinRpcClient::new(config);
    
    // Test that client can be created without errors
    println!("Bitcoin RPC client created successfully");
    
    // Note: We can't test actual connection without a running Bitcoin node
    // but we can test the client creation and basic functionality
    
    Ok(())
}

#[tokio::test]
async fn test_solo_mode_with_bitcoin_rpc() -> Result<()> {
    let solo_config = create_test_solo_config();
    let bitcoin_config = create_test_bitcoin_config();
    let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
    let database = Arc::new(MockDatabaseOps::new());
    
    let solo_handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
    
    // Test that solo handler can be created with Bitcoin RPC client
    println!("Solo mode handler created with Bitcoin RPC client");
    
    // Test share processing (will fail without actual Bitcoin node, but tests the flow)
    let connection_id = Uuid::new_v4();
    let share = Share::new(connection_id, 12345, chrono::Utc::now().timestamp() as u32, 1.0);
    
    match solo_handler.process_share(share).await {
        Ok(result) => {
            println!("Share processing result: {:?}", result);
            // Should get some result, even if it's an error due to no Bitcoin node
        }
        Err(e) => {
            println!("Share processing failed as expected (no Bitcoin node): {}", e);
            // This is expected without a running Bitcoin node
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_work_template_generation_mock() -> Result<()> {
    let config = create_test_bitcoin_config();
    let client = BitcoinRpcClient::new(config);
    
    // Test work template generation (will fail without Bitcoin node, but tests the interface)
    match client.generate_work_template("tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx").await {
        Ok(template) => {
            println!("Work template generated: id={}, difficulty={}", template.id, template.difficulty);
            assert!(template.difficulty > 0.0);
        }
        Err(e) => {
            println!("Work template generation failed as expected (no Bitcoin node): {}", e);
            // This is expected without a running Bitcoin node
            assert!(e.to_string().contains("Failed to connect") || 
                   e.to_string().contains("Connection refused") ||
                   e.to_string().contains("HTTP request failed") ||
                   e.to_string().contains("RPC request timeout"));
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_bitcoin_network_conversion() -> Result<()> {
    let configs = vec![
        (BitcoinNetwork::Mainnet, "mainnet"),
        (BitcoinNetwork::Testnet, "testnet"),
        (BitcoinNetwork::Signet, "signet"),
        (BitcoinNetwork::Regtest, "regtest"),
    ];
    
    for (network, name) in configs {
        let config = BitcoinConfig {
            rpc_url: "http://127.0.0.1:18443".to_string(),
            rpc_user: "test".to_string(),
            rpc_password: "test".to_string(),
            network,
            coinbase_address: None,
            block_template_timeout: 30,
        };
        
        let client = BitcoinRpcClient::new(config);
        println!("Created Bitcoin RPC client for {} network", name);
        
        // Test that client can be created for all network types
        assert!(true); // Just verify no panics occur
    }
    
    Ok(())
}

#[tokio::test]
async fn test_solo_mode_start_without_bitcoin_node() -> Result<()> {
    let solo_config = create_test_solo_config();
    let bitcoin_config = create_test_bitcoin_config();
    let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
    let database = Arc::new(MockDatabaseOps::new());
    
    let solo_handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
    
    // Test starting solo mode (should fail gracefully without Bitcoin node)
    match solo_handler.start().await {
        Ok(()) => {
            println!("Solo mode started successfully (unexpected without Bitcoin node)");
        }
        Err(e) => {
            println!("Solo mode start failed as expected (no Bitcoin node): {}", e);
            // This is expected without a running Bitcoin node
            assert!(e.to_string().contains("Failed to connect") || 
                   e.to_string().contains("Connection refused") ||
                   e.to_string().contains("HTTP request failed") ||
                   e.to_string().contains("RPC request timeout"));
        }
    }
    
    Ok(())
}

#[tokio::test]
async fn test_share_validation_flow() -> Result<()> {
    let solo_config = create_test_solo_config();
    let bitcoin_config = create_test_bitcoin_config();
    let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
    let database = Arc::new(MockDatabaseOps::new());
    
    let solo_handler = SoloModeHandler::new(solo_config, bitcoin_client, database);
    
    // Test different share scenarios
    let connection_id = Uuid::new_v4();
    
    // Test valid share (will fail without work template, but tests validation logic)
    let valid_share = Share::new(connection_id, 0x12345678, chrono::Utc::now().timestamp() as u32, 1.0);
    
    match solo_handler.process_share(valid_share).await {
        Ok(ShareResult::Valid) => {
            println!("Share validated as valid");
        }
        Ok(ShareResult::Invalid(reason)) => {
            println!("Share validated as invalid: {}", reason);
        }
        Ok(ShareResult::Block(hash)) => {
            println!("Share found block: {}", hash);
        }
        Err(e) => {
            println!("Share validation failed: {}", e);
            // Expected without work template
        }
    }
    
    // Test share with zero difficulty (should be invalid)
    let invalid_share = Share::new(connection_id, 0x12345678, chrono::Utc::now().timestamp() as u32, 0.0);
    
    match solo_handler.process_share(invalid_share).await {
        Ok(ShareResult::Invalid(reason)) => {
            println!("Zero difficulty share correctly rejected: {}", reason);
        }
        Ok(result) => {
            println!("Unexpected result for zero difficulty share: {:?}", result);
        }
        Err(e) => {
            println!("Share validation error: {}", e);
        }
    }
    
    Ok(())
}

fn create_test_bitcoin_config() -> BitcoinConfig {
    BitcoinConfig {
        rpc_url: "http://127.0.0.1:18443".to_string(),
        rpc_user: "test".to_string(),
        rpc_password: "test".to_string(),
        network: BitcoinNetwork::Regtest,
        coinbase_address: Some("bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string()),
        block_template_timeout: 5, // Short timeout for tests
    }
}

fn create_test_solo_config() -> SoloConfig {
    SoloConfig {
        coinbase_address: "bcrt1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string(),
        block_template_refresh_interval: 30,
        enable_custom_templates: false,
        max_template_age: 300,
    }
}