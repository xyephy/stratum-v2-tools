use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use std::time::Duration;

mod client;
mod commands;

use client::ApiClient;
use commands::{auth, status, daemon_control, setup, monitor};

#[derive(Parser)]
#[command(name = "sv2-cli")]
#[command(version = "0.1.0")]
#[command(about = "Stratum V2 command-line interface")]
struct Cli {
    /// API server URL
    #[arg(long, default_value = "http://localhost:8080")]
    url: String,

    /// API key for authentication
    #[arg(long)]
    api_key: Option<String>,

    /// Request timeout in seconds
    #[arg(long, default_value = "30")]
    timeout: u64,

    /// Enable verbose output
    #[arg(short, long)]
    verbose: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Get daemon status and health information
    Status {
        /// Show detailed status information
        #[arg(short, long)]
        detailed: bool,
        
        /// Output format (json, table, yaml)
        #[arg(short, long, default_value = "table")]
        format: String,
    },
    
    /// Start the daemon
    Start {
        /// Configuration file path
        #[arg(short, long)]
        config: Option<String>,
        
        /// Run in foreground (don't daemonize)
        #[arg(short, long)]
        foreground: bool,
    },
    
    /// Stop the daemon
    Stop {
        /// Force stop without graceful shutdown
        #[arg(short, long)]
        force: bool,
        
        /// Timeout for graceful shutdown in seconds
        #[arg(short, long, default_value = "30")]
        timeout: u64,
    },
    
    /// Restart the daemon
    Restart {
        /// Configuration file path
        #[arg(short, long)]
        config: Option<String>,
        
        /// Timeout for restart operation in seconds
        #[arg(short, long, default_value = "60")]
        timeout: u64,
    },
    
    /// Reload daemon configuration
    Reload {
        /// Configuration file path
        #[arg(short, long)]
        config: Option<String>,
        
        /// Validate configuration without applying
        #[arg(short, long)]
        validate_only: bool,
    },
    
    /// Interactive setup wizard
    Setup {
        /// Skip hardware detection
        #[arg(long)]
        skip_detection: bool,
        
        /// Configuration output file
        #[arg(short, long)]
        output: Option<String>,
        
        /// Use preset configuration template
        #[arg(short, long)]
        preset: Option<String>,
    },
    
    /// Real-time monitoring display
    Monitor {
        /// Refresh interval in seconds
        #[arg(short, long, default_value = "5")]
        interval: u64,
        
        /// Hide connection information
        #[arg(long)]
        no_connections: bool,
        
        /// Hide share information
        #[arg(long)]
        no_shares: bool,
        
        /// Hide performance metrics
        #[arg(long)]
        no_performance: bool,
        
        /// Hide alerts
        #[arg(long)]
        no_alerts: bool,
        
        /// Maximum entries to display
        #[arg(short, long, default_value = "10")]
        max_entries: usize,
    },
    
    /// Authentication and authorization management
    Auth {
        #[command(subcommand)]
        command: auth::AuthSubcommand,
    },
    
    /// Configuration management
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    
    /// System diagnostics
    Diagnostics,
    
    /// Troubleshooting analysis
    Troubleshoot,
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Get configuration value
    Get {
        /// Configuration key (e.g., "mode" or "database.type")
        key: Option<String>,
    },
    
    /// Validate configuration
    Validate {
        /// Configuration file to validate
        #[arg(short, long)]
        file: Option<String>,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    if cli.verbose {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();
    } else {
        env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    }

    // Create API client
    let mut client = ApiClient::new()
        .with_base_url(&cli.url)
        .context("Invalid API URL")?;

    if let Some(api_key) = cli.api_key {
        client = client.with_api_key(api_key);
    }

    // Execute command
    match cli.command {
        Commands::Status { detailed, format } => {
            status::handle_status(&client, detailed, &format).await
        }
        
        Commands::Start { config, foreground } => {
            daemon_control::handle_start(&client, config, foreground).await
        }
        
        Commands::Stop { force, timeout } => {
            daemon_control::handle_stop(&client, force, Duration::from_secs(timeout)).await
        }
        
        Commands::Restart { config, timeout } => {
            daemon_control::handle_restart(&client, config, Duration::from_secs(timeout)).await
        }
        
        Commands::Reload { config, validate_only } => {
            daemon_control::handle_reload(&client, config, validate_only).await
        }
        
        Commands::Setup { skip_detection, output, preset } => {
            setup::handle_setup(skip_detection, output, preset).await
        }
        
        Commands::Monitor { interval, no_connections, no_shares, no_performance, no_alerts, max_entries } => {
            let config = monitor::MonitorConfig {
                refresh_interval: Duration::from_secs(interval),
                show_connections: !no_connections,
                show_shares: !no_shares,
                show_performance: !no_performance,
                show_alerts: !no_alerts,
                max_entries,
            };
            monitor::handle_monitor(&client, config).await
        }
        
        Commands::Config { action } => {
            match action {
                ConfigAction::Get { key } => {
                    monitor::handle_config_get(&client, key).await
                }
                ConfigAction::Validate { file } => {
                    monitor::handle_config_validate(&client, file).await
                }
            }
        }
        
        Commands::Diagnostics => {
            monitor::handle_diagnostics(&client).await
        }
        
        Commands::Troubleshoot => {
            monitor::handle_troubleshoot(&client).await
        }
        
        Commands::Auth { command } => {
            let auth_command = auth::AuthCommand { command };
            auth_command.execute(&client).await
        }
    }
}