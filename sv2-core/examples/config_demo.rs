use sv2_core::config::DaemonConfig;
use sv2_core::mode::OperationMode;
use std::collections::HashMap;

fn main() -> sv2_core::Result<()> {
    println!("SV2 Configuration Demo");
    println!("======================");

    // 1. Create a default configuration
    println!("\n1. Default configuration:");
    let default_config = DaemonConfig::default();
    println!("   Mode: {}", default_config.get_mode_type());
    println!("   Bind address: {}", default_config.network.bind_address);

    // 2. Create mode-specific templates
    println!("\n2. Mode-specific templates:");
    for mode in [OperationMode::Solo, OperationMode::Pool, OperationMode::Proxy, OperationMode::Client] {
        let config = DaemonConfig::template_for_mode(mode.clone());
        println!("   {} mode template created", mode);
    }

    // 3. Load configuration from file (if exists)
    println!("\n3. Loading from configuration files:");
    let config_files = [
        ("examples/solo_config.toml", OperationMode::Solo),
        ("examples/pool_config.toml", OperationMode::Pool),
        ("examples/proxy_config.toml", OperationMode::Proxy),
    ];

    for (file_path, expected_mode) in config_files {
        let path = std::path::Path::new(file_path);
        if path.exists() {
            match DaemonConfig::from_file(path) {
                Ok(config) => {
                    println!("   ✓ Loaded {} - Mode: {}", file_path, config.get_mode_type());
                    assert_eq!(config.get_mode_type(), expected_mode);
                }
                Err(e) => {
                    println!("   ✗ Failed to load {}: {}", file_path, e);
                }
            }
        } else {
            println!("   - {} not found", file_path);
        }
    }

    // 4. Environment variable override demo
    println!("\n4. Environment variable override:");
    std::env::set_var("SV2D_BITCOIN_RPC_URL", "http://demo:8332");
    std::env::set_var("SV2D_LOG_LEVEL", "debug");

    let mut config = DaemonConfig::default();
    config.merge_env()?;
    println!("   Bitcoin RPC URL: {}", config.bitcoin.rpc_url);
    println!("   Log level: {}", config.logging.level);

    // Clean up environment variables
    std::env::remove_var("SV2D_BITCOIN_RPC_URL");
    std::env::remove_var("SV2D_LOG_LEVEL");

    // 5. Configuration overrides demo
    println!("\n5. Configuration overrides:");
    let mut config = DaemonConfig::default();
    let mut overrides = HashMap::new();
    overrides.insert("network.bind_address".to_string(), "0.0.0.0:4444".to_string());
    overrides.insert("bitcoin.network".to_string(), "testnet".to_string());

    config.apply_overrides(overrides)?;
    println!("   Bind address: {}", config.network.bind_address);
    println!("   Bitcoin network: {:?}", config.bitcoin.network);

    // 6. Configuration validation demo
    println!("\n6. Configuration validation:");
    let mut invalid_config = DaemonConfig::default();
    // This should fail validation due to empty coinbase address in solo mode
    match invalid_config.validate() {
        Ok(_) => println!("   ✓ Configuration is valid"),
        Err(e) => println!("   ✗ Configuration validation failed: {}", e),
    }

    // Fix the configuration
    if let sv2_core::config::OperationModeConfig::Solo(ref mut solo_config) = invalid_config.mode {
        solo_config.coinbase_address = "bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh".to_string();
    }

    match invalid_config.validate() {
        Ok(_) => println!("   ✓ Configuration is now valid after fixing coinbase address"),
        Err(e) => println!("   ✗ Configuration still invalid: {}", e),
    }

    println!("\nDemo completed successfully!");
    Ok(())
}