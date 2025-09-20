use anyhow::{Context, Result};
use std::time::Duration;
use tokio::time::sleep;

use crate::client::ApiClient;
use super::{print_success, print_error, print_info, print_warning, check_daemon_connection};

/// Handle the start command
pub async fn handle_start(
    client: &ApiClient,
    config_path: Option<String>,
    foreground: bool,
) -> Result<()> {
    print_info("Starting sv2d daemon...");

    // Check if daemon is already running
    if check_daemon_connection(client).await.unwrap_or(false) {
        print_warning("Daemon is already running");
        return Ok(());
    }

    // In a real implementation, this would start the daemon process
    // For now, we'll simulate the API call
    match client.start_daemon().await {
        Ok(response) => {
            if response.success {
                print_success(&response.message);
                if let Some(pid) = response.pid {
                    print_info(&format!("Daemon started with PID: {}", pid));
                }

                if !foreground {
                    // Wait a moment and verify the daemon started
                    sleep(Duration::from_secs(2)).await;
                    if check_daemon_connection(client).await.unwrap_or(false) {
                        print_success("Daemon startup verified");
                    } else {
                        print_error("Daemon may have failed to start properly");
                    }
                }
            } else {
                print_error(&response.message);
            }
        }
        Err(e) => {
            print_error(&format!("Failed to start daemon: {}", e));
            
            // Provide helpful suggestions
            println!("\nTroubleshooting tips:");
            println!("• Check if sv2d binary is in PATH");
            println!("• Verify configuration file is valid");
            println!("• Check system logs for error details");
            if let Some(config) = config_path {
                println!("• Validate config file: {}", config);
            }
        }
    }

    Ok(())
}

/// Handle the stop command
pub async fn handle_stop(
    client: &ApiClient,
    force: bool,
    timeout: Duration,
) -> Result<()> {
    print_info("Stopping sv2d daemon...");

    // Check if daemon is running
    if !check_daemon_connection(client).await.unwrap_or(false) {
        print_warning("Daemon is not running");
        return Ok(());
    }

    match client.stop_daemon().await {
        Ok(response) => {
            if response.success {
                print_success(&response.message);

                // Wait for graceful shutdown
                if !force {
                    print_info("Waiting for graceful shutdown...");
                    let start_time = std::time::Instant::now();
                    
                    while start_time.elapsed() < timeout {
                        sleep(Duration::from_millis(500)).await;
                        
                        if !check_daemon_connection(client).await.unwrap_or(false) {
                            print_success("Daemon stopped successfully");
                            return Ok(());
                        }
                    }
                    
                    print_warning("Graceful shutdown timeout reached");
                } else {
                    print_info("Force stop requested");
                }
            } else {
                print_error(&response.message);
            }
        }
        Err(e) => {
            print_error(&format!("Failed to stop daemon: {}", e));
            
            if force {
                print_info("Attempting force stop...");
                // In a real implementation, this would send SIGKILL
                print_warning("Force stop not implemented in API client");
            }
        }
    }

    Ok(())
}

/// Handle the restart command
pub async fn handle_restart(
    client: &ApiClient,
    config_path: Option<String>,
    timeout: Duration,
) -> Result<()> {
    print_info("Restarting sv2d daemon...");

    // Check current status
    let was_running = check_daemon_connection(client).await.unwrap_or(false);

    if was_running {
        // Stop the daemon first
        print_info("Stopping daemon...");
        handle_stop(client, false, timeout).await?;
        
        // Wait a moment before starting
        sleep(Duration::from_secs(1)).await;
    }

    // Start the daemon
    print_info("Starting daemon...");
    handle_start(client, config_path, false).await?;

    Ok(())
}

/// Handle the reload command
pub async fn handle_reload(
    client: &ApiClient,
    config_path: Option<String>,
    validate_only: bool,
) -> Result<()> {
    if validate_only {
        print_info("Validating configuration...");
    } else {
        print_info("Reloading daemon configuration...");
    }

    // Check if daemon is running
    if !check_daemon_connection(client).await.unwrap_or(false) {
        print_error("Daemon is not running");
        return Ok(());
    }

    // If a config path is provided, we would load and validate it first
    if let Some(config_file) = config_path {
        print_info(&format!("Loading configuration from: {}", config_file));
        
        // In a real implementation, we would:
        // 1. Load the config file
        // 2. Parse and validate it
        // 3. Send it to the daemon via API
        
        // For now, we'll just get the current config and validate it
        match client.get_config().await {
            Ok(config) => {
                print_success("Configuration loaded successfully");
                
                // Validate the configuration
                match client.validate_config(&config).await {
                    Ok(response) => {
                        if response.success {
                            print_success("Configuration validation passed");
                            
                            if !validate_only {
                                // Apply the configuration
                                match client.update_config(&config, false).await {
                                    Ok(update_response) => {
                                        if update_response.success {
                                            print_success("Configuration reloaded successfully");
                                        } else {
                                            print_error(&update_response.message);
                                            if let Some(errors) = update_response.validation_errors {
                                                for error in errors {
                                                    print_error(&format!("  • {}", error));
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        print_error(&format!("Failed to update configuration: {}", e));
                                    }
                                }
                            }
                        } else {
                            print_error("Configuration validation failed");
                            if let Some(errors) = response.validation_errors {
                                for error in errors {
                                    print_error(&format!("  • {}", error));
                                }
                            }
                        }
                    }
                    Err(e) => {
                        print_error(&format!("Failed to validate configuration: {}", e));
                    }
                }
            }
            Err(e) => {
                print_error(&format!("Failed to load current configuration: {}", e));
            }
        }
    } else {
        // Reload current configuration
        match client.reload_daemon().await {
            Ok(response) => {
                if response.success {
                    print_success(&response.message);
                } else {
                    print_error(&response.message);
                }
            }
            Err(e) => {
                print_error(&format!("Failed to reload configuration: {}", e));
            }
        }
    }

    Ok(())
}