use clap::{Arg, Command};
use sv2_core::{Result, DaemonConfig, Daemon};
use tracing::{info, error};

pub mod daemon;

use daemon::Sv2Daemon;

#[tokio::main]
async fn main() -> Result<()> {
    // Logging will be initialized by daemon based on config

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
        .arg(
            Arg::new("mode")
                .short('m')
                .long("mode")
                .value_name("MODE")
                .help("Operation mode (solo, pool, proxy, client)")
        )
        .get_matches();

    let config_path = matches.get_one::<String>("config").unwrap();
    
    // Load configuration
    let mut config = if std::path::Path::new(config_path).exists() {
        info!("Loading configuration from {}", config_path);
        DaemonConfig::from_file(std::path::Path::new(config_path))?
    } else {
        info!("Using default configuration");
        DaemonConfig::default()
    };

    // Override mode if specified
    if let Some(mode) = matches.get_one::<String>("mode") {
        config.mode = mode.parse()?;
    }

    // Merge environment variables
    config.merge_env()?;

    info!("Starting sv2d in {} mode", config.mode);

    // Create and start daemon
    let mut daemon = Sv2Daemon::new();
    
    if let Err(e) = daemon.start(config).await {
        error!("Failed to start daemon: {}", e);
        std::process::exit(1);
    }

    // Run until shutdown signal is received
    if let Err(e) = daemon.run_until_shutdown().await {
        error!("Daemon error: {}", e);
        std::process::exit(1);
    }

    Ok(())
}