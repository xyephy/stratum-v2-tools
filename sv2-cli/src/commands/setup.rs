use anyhow::{Context, Result};
use std::io::{self, Write};
use std::collections::HashMap;
use serde::{Deserialize, Serialize};

use super::{print_success, print_error, print_info, print_warning};

/// Configuration preset for common mining setups
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConfigPreset {
    pub name: String,
    pub description: String,
    pub mode: String,
    pub listen_address: String,
    pub difficulty: Option<f64>,
    pub max_connections: Option<u32>,
    pub bitcoin_rpc_url: Option<String>,
    pub upstream_url: Option<String>,
    pub pool_url: Option<String>,
    pub additional_settings: HashMap<String, String>,
}

/// Detected hardware information
#[derive(Debug, Clone)]
pub struct HardwareInfo {
    pub device_type: String,
    pub model: Option<String>,
    pub hashrate: Option<f64>,
    pub power_consumption: Option<u32>,
    pub recommended_difficulty: Option<f64>,
    pub connection_info: Option<String>,
}

/// Setup wizard configuration
#[derive(Debug, Clone)]
pub struct SetupConfig {
    pub mode: String,
    pub listen_address: String,
    pub bitcoin_node: Option<String>,
    pub upstream_pool: Option<String>,
    pub difficulty: Option<f64>,
    pub max_connections: Option<u32>,
    pub enable_monitoring: bool,
    pub log_level: String,
    pub detected_hardware: Vec<HardwareInfo>,
}

/// Handle the setup command
pub async fn handle_setup(
    skip_detection: bool,
    output_path: Option<String>,
    preset: Option<String>,
) -> Result<()> {
    print_info("Starting sv2-cli setup wizard...");
    
    println!("\n{}", "🚀 Welcome to sv2-cli Setup Wizard".to_uppercase());
    println!("This wizard will help you configure your Stratum V2 mining setup.\n");

    let mut setup_config = SetupConfig {
        mode: String::new(),
        listen_address: "0.0.0.0:3333".to_string(),
        bitcoin_node: None,
        upstream_pool: None,
        difficulty: None,
        max_connections: None,
        enable_monitoring: true,
        log_level: "info".to_string(),
        detected_hardware: Vec::new(),
    };

    // Load preset configuration if specified
    if let Some(preset_name) = preset {
        if let Some(preset_config) = load_preset(&preset_name)? {
            print_success(&format!("Loaded preset configuration: {}", preset_config.name));
            println!("Description: {}", preset_config.description);
            
            setup_config.mode = preset_config.mode;
            setup_config.listen_address = preset_config.listen_address;
            setup_config.difficulty = preset_config.difficulty;
            setup_config.max_connections = preset_config.max_connections;
            
            if let Some(bitcoin_url) = preset_config.bitcoin_rpc_url {
                setup_config.bitcoin_node = Some(bitcoin_url);
            }
            if let Some(upstream_url) = preset_config.upstream_url {
                setup_config.upstream_pool = Some(upstream_url);
            }
            
            println!("\nPreset configuration loaded. You can modify these settings in the following steps.\n");
        } else {
            print_warning(&format!("Preset '{}' not found. Available presets:", preset_name));
            list_available_presets();
            println!();
        }
    }

    // Hardware detection
    if !skip_detection {
        print_info("🔍 Detecting mining hardware...");
        setup_config.detected_hardware = detect_mining_hardware().await?;
        
        if setup_config.detected_hardware.is_empty() {
            print_warning("No mining hardware detected automatically");
            println!("You can still configure sv2d manually for your specific setup.");
        } else {
            print_success(&format!("Detected {} mining device(s)", setup_config.detected_hardware.len()));
            display_detected_hardware(&setup_config.detected_hardware);
        }
        println!();
    }

    // Interactive configuration
    println!("📝 Please answer the following questions to configure your setup:\n");

    // Mode selection (skip if preset loaded)
    if setup_config.mode.is_empty() {
        setup_config.mode = prompt_for_mode()?;
    } else {
        println!("Operation mode: {} (from preset)", setup_config.mode);
        if !prompt_yes_no("Keep this mode?")? {
            setup_config.mode = prompt_for_mode()?;
        }
    }

    // Mode-specific configuration
    match setup_config.mode.as_str() {
        "solo" => {
            setup_config.bitcoin_node = Some(prompt_for_bitcoin_node(setup_config.bitcoin_node.as_deref())?);
            setup_config.difficulty = Some(prompt_for_difficulty(setup_config.difficulty, &setup_config.detected_hardware)?);
        }
        "pool" => {
            setup_config.difficulty = Some(prompt_for_difficulty(setup_config.difficulty, &setup_config.detected_hardware)?);
            setup_config.max_connections = Some(prompt_for_max_connections(setup_config.max_connections)?);
        }
        "proxy" => {
            setup_config.upstream_pool = Some(prompt_for_upstream_pool(setup_config.upstream_pool.as_deref())?);
        }
        "client" => {
            setup_config.upstream_pool = Some(prompt_for_pool_connection(setup_config.upstream_pool.as_deref())?);
        }
        _ => {}
    }

    // Network configuration
    setup_config.listen_address = prompt_for_listen_address(Some(&setup_config.listen_address))?;

    // Advanced settings
    if prompt_yes_no("Configure advanced settings?")? {
        setup_config.enable_monitoring = prompt_for_monitoring(setup_config.enable_monitoring)?;
        setup_config.log_level = prompt_for_log_level(&setup_config.log_level)?;
    }

    // Configuration validation
    print_info("🔍 Validating configuration...");
    if let Err(e) = validate_setup_config(&setup_config) {
        print_error(&format!("Configuration validation failed: {}", e));
        if !prompt_yes_no("Continue anyway?")? {
            return Ok(());
        }
    } else {
        print_success("Configuration validation passed");
    }

    // Generate configuration file
    let config_content = generate_advanced_config(&setup_config)?;

    // Display configuration summary
    display_configuration_summary(&setup_config);

    // Save configuration
    let output_file = output_path.unwrap_or_else(|| "sv2d.toml".to_string());
    
    if prompt_yes_no(&format!("💾 Save configuration to '{}'?", output_file))? {
        std::fs::write(&output_file, config_content)
            .with_context(|| format!("Failed to write configuration to {}", output_file))?;
        
        print_success(&format!("Configuration saved to {}", output_file));
        
        // Display next steps
        display_next_steps(&output_file, &setup_config);
    } else {
        print_info("Configuration not saved");
        println!("\n📄 Generated configuration:\n{}", config_content);
    }

    Ok(())
}

/// Prompt user for operation mode
fn prompt_for_mode() -> Result<String> {
    println!("Select operation mode:");
    println!("1. Solo mining (mine directly to Bitcoin network)");
    println!("2. Pool mining (host a private pool)");
    println!("3. Proxy mode (translate SV1 to SV2)");
    println!("4. Client mode (connect to SV2 pool)");

    loop {
        print!("Enter choice (1-4): ");
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        match input.trim() {
            "1" => return Ok("solo".to_string()),
            "2" => return Ok("pool".to_string()),
            "3" => return Ok("proxy".to_string()),
            "4" => return Ok("client".to_string()),
            _ => println!("Invalid choice. Please enter 1, 2, 3, or 4."),
        }
    }
}

/// Prompt user for Bitcoin node connection
fn prompt_for_bitcoin_node(default: Option<&str>) -> Result<String> {
    let default_url = default.unwrap_or("http://localhost:8332");
    print!("Bitcoin node RPC URL (default: {}): ", default_url);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let url = input.trim();
    if url.is_empty() {
        Ok(default_url.to_string())
    } else {
        // Validate URL format
        if !url.starts_with("http://") && !url.starts_with("https://") {
            print_warning("URL should start with http:// or https://");
        }
        Ok(url.to_string())
    }
}

/// Prompt user for upstream pool connection
fn prompt_for_upstream_pool(default: Option<&str>) -> Result<String> {
    let default_url = default.unwrap_or("stratum+tcp://pool.example.com:4444");
    print!("Upstream pool URL (default: {}): ", default_url);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let url = input.trim();
    if url.is_empty() {
        Ok(default_url.to_string())
    } else {
        Ok(url.to_string())
    }
}

/// Prompt user for pool connection (client mode)
fn prompt_for_pool_connection(default: Option<&str>) -> Result<String> {
    let default_url = default.unwrap_or("stratum+tcp://pool.example.com:4444");
    print!("Pool URL to connect to (default: {}): ", default_url);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let url = input.trim();
    if url.is_empty() {
        Ok(default_url.to_string())
    } else {
        Ok(url.to_string())
    }
}

/// Prompt user for difficulty setting
fn prompt_for_difficulty(default: Option<f64>, hardware: &[HardwareInfo]) -> Result<f64> {
    // Calculate recommended difficulty based on detected hardware
    let recommended = if !hardware.is_empty() {
        let total_hashrate: f64 = hardware.iter()
            .filter_map(|h| h.hashrate)
            .sum();
        
        // Simple difficulty calculation based on total hashrate
        if total_hashrate > 50_000_000_000_000.0 { // > 50 TH/s
            65536.0
        } else if total_hashrate > 10_000_000_000_000.0 { // > 10 TH/s
            16384.0
        } else if total_hashrate > 1_000_000_000_000.0 { // > 1 TH/s
            4096.0
        } else {
            1.0
        }
    } else {
        default.unwrap_or(1.0)
    };

    print!("Mining difficulty (recommended: {}, default: {}): ", 
        recommended, default.unwrap_or(recommended));
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let difficulty_str = input.trim();
    if difficulty_str.is_empty() {
        Ok(default.unwrap_or(recommended))
    } else {
        match difficulty_str.parse::<f64>() {
            Ok(diff) if diff > 0.0 => Ok(diff),
            _ => {
                print_warning("Invalid difficulty, using recommended value");
                Ok(recommended)
            }
        }
    }
}

/// Prompt user for maximum connections
fn prompt_for_max_connections(default: Option<u32>) -> Result<u32> {
    let default_value = default.unwrap_or(1000);
    print!("Maximum concurrent connections (default: {}): ", default_value);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let connections_str = input.trim();
    if connections_str.is_empty() {
        Ok(default_value)
    } else {
        match connections_str.parse::<u32>() {
            Ok(conn) if conn > 0 => Ok(conn),
            _ => {
                print_warning("Invalid number, using default value");
                Ok(default_value)
            }
        }
    }
}

/// Prompt user for listen address
fn prompt_for_listen_address(default: Option<&str>) -> Result<String> {
    let default_addr = default.unwrap_or("0.0.0.0:3333");
    print!("Listen address (default: {}): ", default_addr);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let address = input.trim();
    if address.is_empty() {
        Ok(default_addr.to_string())
    } else {
        // Validate address format
        if address.parse::<std::net::SocketAddr>().is_err() {
            print_warning("Invalid address format, using default");
            Ok(default_addr.to_string())
        } else {
            Ok(address.to_string())
        }
    }
}

/// Prompt user for monitoring settings
fn prompt_for_monitoring(default: bool) -> Result<bool> {
    let default_str = if default { "Y" } else { "n" };
    print!("Enable monitoring and web dashboard? ({}/n): ", default_str);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    match input.trim().to_lowercase().as_str() {
        "" => Ok(default),
        "y" | "yes" => Ok(true),
        "n" | "no" => Ok(false),
        _ => {
            print_warning("Invalid input, using default");
            Ok(default)
        }
    }
}

/// Prompt user for log level
fn prompt_for_log_level(default: &str) -> Result<String> {
    println!("Select log level:");
    println!("1. error   - Only errors");
    println!("2. warn    - Warnings and errors");
    println!("3. info    - General information (recommended)");
    println!("4. debug   - Detailed debugging information");
    println!("5. trace   - Very verbose tracing");
    
    print!("Log level (default: {}): ", default);
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    
    let level_str = input.trim();
    if level_str.is_empty() {
        Ok(default.to_string())
    } else {
        match level_str {
            "1" => Ok("error".to_string()),
            "2" => Ok("warn".to_string()),
            "3" => Ok("info".to_string()),
            "4" => Ok("debug".to_string()),
            "5" => Ok("trace".to_string()),
            "error" | "warn" | "info" | "debug" | "trace" => Ok(level_str.to_string()),
            _ => {
                print_warning("Invalid log level, using default");
                Ok(default.to_string())
            }
        }
    }
}

/// Prompt for yes/no answer
fn prompt_yes_no(question: &str) -> Result<bool> {
    loop {
        print!("{} (y/n): ", question);
        io::stdout().flush()?;

        let mut input = String::new();
        io::stdin().read_line(&mut input)?;
        
        match input.trim().to_lowercase().as_str() {
            "y" | "yes" => return Ok(true),
            "n" | "no" => return Ok(false),
            _ => println!("Please enter 'y' or 'n'."),
        }
    }
}

/// Load configuration preset by name
fn load_preset(name: &str) -> Result<Option<ConfigPreset>> {
    let presets = get_builtin_presets();
    Ok(presets.into_iter().find(|p| p.name == name))
}

/// Get list of built-in configuration presets
fn get_builtin_presets() -> Vec<ConfigPreset> {
    vec![
        ConfigPreset {
            name: "bitaxe-solo".to_string(),
            description: "Bitaxe device solo mining configuration".to_string(),
            mode: "solo".to_string(),
            listen_address: "0.0.0.0:3333".to_string(),
            difficulty: Some(1.0),
            max_connections: Some(10),
            bitcoin_rpc_url: Some("http://localhost:8332".to_string()),
            upstream_url: None,
            pool_url: None,
            additional_settings: HashMap::new(),
        },
        ConfigPreset {
            name: "antminer-pool".to_string(),
            description: "Antminer S9 pool mining configuration".to_string(),
            mode: "pool".to_string(),
            listen_address: "0.0.0.0:3333".to_string(),
            difficulty: Some(16384.0),
            max_connections: Some(100),
            bitcoin_rpc_url: None,
            upstream_url: None,
            pool_url: None,
            additional_settings: HashMap::new(),
        },
        ConfigPreset {
            name: "proxy-sv1-to-sv2".to_string(),
            description: "SV1 to SV2 proxy for legacy miners".to_string(),
            mode: "proxy".to_string(),
            listen_address: "0.0.0.0:3333".to_string(),
            difficulty: None,
            max_connections: Some(1000),
            bitcoin_rpc_url: None,
            upstream_url: Some("stratum+tcp://pool.example.com:4444".to_string()),
            pool_url: None,
            additional_settings: HashMap::new(),
        },
        ConfigPreset {
            name: "sv2-client".to_string(),
            description: "SV2 client connecting to upstream pool".to_string(),
            mode: "client".to_string(),
            listen_address: "127.0.0.1:3334".to_string(),
            difficulty: None,
            max_connections: None,
            bitcoin_rpc_url: None,
            upstream_url: None,
            pool_url: Some("stratum+tcp://pool.example.com:4444".to_string()),
            additional_settings: HashMap::new(),
        },
    ]
}

/// List available configuration presets
fn list_available_presets() {
    let presets = get_builtin_presets();
    for preset in presets {
        println!("  • {}: {}", preset.name, preset.description);
    }
}

/// Detect mining hardware on the network
async fn detect_mining_hardware() -> Result<Vec<HardwareInfo>> {
    let mut detected = Vec::new();
    
    // Simulate hardware detection (in a real implementation, this would scan the network)
    // Common mining device detection patterns:
    
    // Check for Bitaxe devices (typically on port 80 with specific endpoints)
    if let Some(bitaxe) = detect_bitaxe_devices().await {
        detected.extend(bitaxe);
    }
    
    // Check for Antminer devices (typically on port 80 with /cgi-bin/minerStatus.cgi)
    if let Some(antminers) = detect_antminer_devices().await {
        detected.extend(antminers);
    }
    
    // Check for Whatsminer devices
    if let Some(whatsminers) = detect_whatsminer_devices().await {
        detected.extend(whatsminers);
    }
    
    Ok(detected)
}

/// Detect Bitaxe devices on the network
async fn detect_bitaxe_devices() -> Option<Vec<HardwareInfo>> {
    // In a real implementation, this would scan common IP ranges
    // and check for Bitaxe-specific HTTP endpoints
    
    // For now, return mock data if we detect common patterns
    if std::env::var("BITAXE_IP").is_ok() {
        Some(vec![HardwareInfo {
            device_type: "Bitaxe".to_string(),
            model: Some("Bitaxe Ultra".to_string()),
            hashrate: Some(500_000_000_000.0), // 500 GH/s
            power_consumption: Some(15), // 15W
            recommended_difficulty: Some(1.0),
            connection_info: Some("192.168.1.100:80".to_string()),
        }])
    } else {
        None
    }
}

/// Detect Antminer devices on the network
async fn detect_antminer_devices() -> Option<Vec<HardwareInfo>> {
    // Mock detection for Antminer S9
    if std::env::var("ANTMINER_IP").is_ok() {
        Some(vec![HardwareInfo {
            device_type: "Antminer".to_string(),
            model: Some("Antminer S9".to_string()),
            hashrate: Some(14_000_000_000_000.0), // 14 TH/s
            power_consumption: Some(1400), // 1400W
            recommended_difficulty: Some(16384.0),
            connection_info: Some("192.168.1.101:80".to_string()),
        }])
    } else {
        None
    }
}

/// Detect Whatsminer devices on the network
async fn detect_whatsminer_devices() -> Option<Vec<HardwareInfo>> {
    // Mock detection for Whatsminer
    if std::env::var("WHATSMINER_IP").is_ok() {
        Some(vec![HardwareInfo {
            device_type: "Whatsminer".to_string(),
            model: Some("Whatsminer M30S".to_string()),
            hashrate: Some(88_000_000_000_000.0), // 88 TH/s
            power_consumption: Some(3400), // 3400W
            recommended_difficulty: Some(65536.0),
            connection_info: Some("192.168.1.102:80".to_string()),
        }])
    } else {
        None
    }
}

/// Display detected hardware information
fn display_detected_hardware(hardware: &[HardwareInfo]) {
    println!("🔧 Detected Mining Hardware:");
    for (i, device) in hardware.iter().enumerate() {
        println!("  {}. {} {}", 
            i + 1, 
            device.device_type,
            device.model.as_deref().unwrap_or("Unknown Model")
        );
        
        if let Some(hashrate) = device.hashrate {
            println!("     Hashrate: {:.2} TH/s", hashrate / 1_000_000_000_000.0);
        }
        
        if let Some(power) = device.power_consumption {
            println!("     Power: {} W", power);
        }
        
        if let Some(difficulty) = device.recommended_difficulty {
            println!("     Recommended Difficulty: {}", difficulty);
        }
        
        if let Some(connection) = &device.connection_info {
            println!("     Connection: {}", connection);
        }
        println!();
    }
}

/// Validate the setup configuration
fn validate_setup_config(config: &SetupConfig) -> Result<()> {
    // Validate mode
    if !["solo", "pool", "proxy", "client"].contains(&config.mode.as_str()) {
        return Err(anyhow::anyhow!("Invalid mode: {}", config.mode));
    }
    
    // Validate listen address
    if config.listen_address.parse::<std::net::SocketAddr>().is_err() {
        return Err(anyhow::anyhow!("Invalid listen address: {}", config.listen_address));
    }
    
    // Mode-specific validation
    match config.mode.as_str() {
        "solo" => {
            if config.bitcoin_node.is_none() {
                return Err(anyhow::anyhow!("Bitcoin node URL required for solo mode"));
            }
        }
        "proxy" | "client" => {
            if config.upstream_pool.is_none() {
                return Err(anyhow::anyhow!("Upstream pool URL required for {} mode", config.mode));
            }
        }
        _ => {}
    }
    
    // Validate difficulty
    if let Some(difficulty) = config.difficulty {
        if difficulty <= 0.0 {
            return Err(anyhow::anyhow!("Difficulty must be positive"));
        }
    }
    
    // Validate max connections
    if let Some(max_conn) = config.max_connections {
        if max_conn == 0 {
            return Err(anyhow::anyhow!("Max connections must be greater than 0"));
        }
    }
    
    Ok(())
}

/// Display configuration summary
fn display_configuration_summary(config: &SetupConfig) {
    println!("\n📋 Configuration Summary:");
    println!("┌─────────────────────────────────────────┐");
    println!("│ Operation Mode: {:23} │", config.mode);
    println!("│ Listen Address: {:23} │", config.listen_address);
    
    if let Some(bitcoin_node) = &config.bitcoin_node {
        println!("│ Bitcoin Node: {:25} │", bitcoin_node);
    }
    
    if let Some(upstream_pool) = &config.upstream_pool {
        println!("│ Upstream Pool: {:24} │", upstream_pool);
    }
    
    if let Some(difficulty) = config.difficulty {
        println!("│ Difficulty: {:27} │", difficulty);
    }
    
    if let Some(max_conn) = config.max_connections {
        println!("│ Max Connections: {:22} │", max_conn);
    }
    
    println!("│ Monitoring: {:27} │", if config.enable_monitoring { "Enabled" } else { "Disabled" });
    println!("│ Log Level: {:28} │", config.log_level);
    println!("└─────────────────────────────────────────┘");
    
    if !config.detected_hardware.is_empty() {
        println!("\n🔧 Hardware Configuration:");
        for device in &config.detected_hardware {
            println!("  • {} will use recommended difficulty: {:.0}", 
                device.device_type,
                device.recommended_difficulty.unwrap_or(1.0)
            );
        }
    }
}

/// Display next steps after configuration
fn display_next_steps(config_file: &str, config: &SetupConfig) {
    println!("\n🎉 Setup Complete! Next steps:");
    println!("┌─────────────────────────────────────────────────────────────┐");
    println!("│ 1. Review configuration: cat {}                    │", config_file);
    println!("│ 2. Start the daemon: sv2-cli start -c {}           │", config_file);
    println!("│ 3. Check status: sv2-cli status                            │");
    println!("│ 4. View dashboard: http://localhost:8080                   │");
    println!("└─────────────────────────────────────────────────────────────┘");
    
    // Mode-specific tips
    match config.mode.as_str() {
        "solo" => {
            println!("\n💡 Solo Mining Tips:");
            println!("  • Ensure your Bitcoin node is fully synced");
            println!("  • Configure your payout address in the config file");
            println!("  • Monitor your hashrate and block finding progress");
        }
        "pool" => {
            println!("\n💡 Pool Mining Tips:");
            println!("  • Point your miners to: {}", config.listen_address);
            println!("  • Monitor connection count and share acceptance");
            println!("  • Adjust difficulty based on your miners' performance");
        }
        "proxy" => {
            println!("\n💡 Proxy Mode Tips:");
            println!("  • Point legacy SV1 miners to: {}", config.listen_address);
            println!("  • Verify upstream pool connectivity");
            println!("  • Monitor protocol translation performance");
        }
        "client" => {
            println!("\n💡 Client Mode Tips:");
            println!("  • Verify connection to upstream pool");
            println!("  • Monitor share acceptance rate");
            println!("  • Consider enabling Job Negotiation Protocol");
        }
        _ => {}
    }
    
    if !config.detected_hardware.is_empty() {
        println!("\n🔧 Hardware Setup:");
        for device in &config.detected_hardware {
            if let Some(connection) = &device.connection_info {
                println!("  • Configure {} at {} to point to {}", 
                    device.device_type, connection, config.listen_address);
            }
        }
    }
}

/// Generate advanced configuration file content
fn generate_advanced_config(config: &SetupConfig) -> Result<String> {
    let mut content = String::new();
    
    content.push_str("# sv2d Configuration File\n");
    content.push_str("# Generated by sv2-cli setup wizard\n");
    content.push_str(&format!("# Created: {}\n\n", chrono::Utc::now().format("%Y-%m-%d %H:%M:%S UTC")));
    
    // Basic configuration
    content.push_str(&format!("mode = \"{}\"\n", config.mode));
    content.push_str(&format!("listen_address = \"{}\"\n\n", config.listen_address));
    
    // Database configuration
    content.push_str("[database]\n");
    content.push_str("type = \"sqlite\"\n");
    content.push_str("path = \"sv2d.db\"\n");
    content.push_str("# Uncomment for PostgreSQL:\n");
    content.push_str("# type = \"postgres\"\n");
    content.push_str("# url = \"postgresql://user:password@localhost/sv2d\"\n\n");
    
    // Mode-specific configuration
    match config.mode.as_str() {
        "solo" => {
            content.push_str("[solo]\n");
            if let Some(bitcoin_node) = &config.bitcoin_node {
                content.push_str(&format!("bitcoin_rpc_url = \"{}\"\n", bitcoin_node));
            }
            content.push_str("bitcoin_rpc_user = \"user\"\n");
            content.push_str("bitcoin_rpc_password = \"password\"\n");
            content.push_str("payout_address = \"bc1qexampleaddress\"\n");
            if let Some(difficulty) = config.difficulty {
                content.push_str(&format!("default_difficulty = {}\n", difficulty));
            }
            content.push_str("\n");
        }
        "pool" => {
            content.push_str("[pool]\n");
            if let Some(difficulty) = config.difficulty {
                content.push_str(&format!("default_difficulty = {}\n", difficulty));
            }
            if let Some(max_conn) = config.max_connections {
                content.push_str(&format!("max_connections = {}\n", max_conn));
            }
            content.push_str("share_target_time = 30  # seconds\n");
            content.push_str("difficulty_adjustment_interval = 120  # seconds\n\n");
        }
        "proxy" => {
            content.push_str("[proxy]\n");
            if let Some(upstream_pool) = &config.upstream_pool {
                content.push_str(&format!("upstream_url = \"{}\"\n", upstream_pool));
            }
            content.push_str("upstream_user = \"username\"\n");
            content.push_str("upstream_password = \"password\"\n");
            if let Some(max_conn) = config.max_connections {
                content.push_str(&format!("max_downstream_connections = {}\n", max_conn));
            }
            content.push_str("enable_sv1_fallback = true\n\n");
        }
        "client" => {
            content.push_str("[client]\n");
            if let Some(pool_url) = &config.upstream_pool {
                content.push_str(&format!("pool_url = \"{}\"\n", pool_url));
            }
            content.push_str("worker_name = \"worker1\"\n");
            content.push_str("password = \"x\"\n");
            content.push_str("enable_job_negotiation = true\n\n");
        }
        _ => {}
    }
    
    // Monitoring configuration
    if config.enable_monitoring {
        content.push_str("[monitoring]\n");
        content.push_str("enable_metrics = true\n");
        content.push_str("metrics_port = 9090\n");
        content.push_str("enable_web_dashboard = true\n");
        content.push_str("web_port = 8080\n");
        content.push_str("enable_alerts = true\n");
        content.push_str("alert_thresholds = { hashrate_drop = 0.1, connection_loss = 0.05 }\n\n");
    }
    
    // Logging configuration
    content.push_str("[logging]\n");
    content.push_str(&format!("level = \"{}\"\n", config.log_level));
    content.push_str("format = \"json\"\n");
    content.push_str("file = \"sv2d.log\"\n");
    content.push_str("max_file_size = \"100MB\"\n");
    content.push_str("max_files = 5\n\n");
    
    // Hardware-specific optimizations
    if !config.detected_hardware.is_empty() {
        content.push_str("# Hardware-specific optimizations\n");
        content.push_str("[hardware]\n");
        
        for device in &config.detected_hardware {
            if let Some(difficulty) = device.recommended_difficulty {
                content.push_str(&format!("# {} recommended difficulty: {}\n", 
                    device.device_type, difficulty));
            }
        }
        content.push_str("\n");
    }
    
    // Security configuration
    content.push_str("[security]\n");
    content.push_str("enable_tls = false  # Set to true for production\n");
    content.push_str("# tls_cert_path = \"/path/to/cert.pem\"\n");
    content.push_str("# tls_key_path = \"/path/to/key.pem\"\n");
    content.push_str("rate_limit_connections = 100  # per minute\n");
    content.push_str("max_message_size = 1048576  # 1MB\n");
    
    Ok(content)
}