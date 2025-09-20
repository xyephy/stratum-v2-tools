use anyhow::{Context, Result};
use colored::*;
use std::time::Duration;
use tokio::time::{interval, sleep};
use tabled::{Table, Tabled};

use crate::client::ApiClient;
use super::{print_success, print_error, print_info, print_warning, format_hashrate, format_percentage, format_duration, check_daemon_connection};

/// Real-time monitoring display configuration
#[derive(Debug, Clone)]
pub struct MonitorConfig {
    pub refresh_interval: Duration,
    pub show_connections: bool,
    pub show_shares: bool,
    pub show_performance: bool,
    pub show_alerts: bool,
    pub max_entries: usize,
}

impl Default for MonitorConfig {
    fn default() -> Self {
        Self {
            refresh_interval: Duration::from_secs(5),
            show_connections: true,
            show_shares: true,
            show_performance: true,
            show_alerts: true,
            max_entries: 10,
        }
    }
}

/// Handle real-time monitoring display
pub async fn handle_monitor(
    client: &ApiClient,
    config: MonitorConfig,
) -> Result<()> {
    print_info("Starting real-time monitoring...");
    print_info("Press Ctrl+C to exit");
    
    // Check daemon connection
    if !check_daemon_connection(client).await.unwrap_or(false) {
        return Ok(());
    }

    let mut interval = interval(config.refresh_interval);
    let mut iteration = 0;

    loop {
        // Clear screen and move cursor to top
        print!("\x1B[2J\x1B[1;1H");
        
        // Display header
        println!("{}", "🔍 SV2D Real-time Monitor".bold().blue());
        println!("{}", "─".repeat(60));
        println!("Refresh: {}s | Iteration: {} | Time: {}", 
            config.refresh_interval.as_secs(),
            iteration,
            chrono::Utc::now().format("%H:%M:%S UTC")
        );
        println!();

        // Display daemon status
        match display_daemon_status(client).await {
            Ok(_) => {},
            Err(e) => {
                print_error(&format!("Failed to get daemon status: {}", e));
                sleep(Duration::from_secs(2)).await;
                continue;
            }
        }

        // Display connections if enabled
        if config.show_connections {
            if let Err(e) = display_connections_monitor(client, config.max_entries).await {
                print_warning(&format!("Failed to get connections: {}", e));
            }
        }

        // Display recent shares if enabled
        if config.show_shares {
            if let Err(e) = display_shares_monitor(client, config.max_entries).await {
                print_warning(&format!("Failed to get shares: {}", e));
            }
        }

        // Display performance metrics if enabled
        if config.show_performance {
            if let Err(e) = display_performance_monitor(client).await {
                print_warning(&format!("Failed to get performance metrics: {}", e));
            }
        }

        // Display alerts if enabled
        if config.show_alerts {
            if let Err(e) = display_alerts_monitor(client, config.max_entries).await {
                print_warning(&format!("Failed to get alerts: {}", e));
            }
        }

        iteration += 1;
        interval.tick().await;
    }
}

/// Display daemon status in monitor
async fn display_daemon_status(client: &ApiClient) -> Result<()> {
    let status = client.get_status().await?;
    
    println!("{}", "📊 Daemon Status".bold());
    
    #[derive(Tabled)]
    struct StatusDisplay {
        #[tabled(rename = "Metric")]
        metric: String,
        #[tabled(rename = "Value")]
        value: String,
        #[tabled(rename = "Status")]
        status: String,
    }

    let acceptance_rate = if status.total_shares > 0 {
        (status.valid_shares as f64 / status.total_shares as f64) * 100.0
    } else {
        0.0
    };

    let rows = vec![
        StatusDisplay {
            metric: "Uptime".to_string(),
            value: format_duration(status.uptime),
            status: "🟢".to_string(),
        },
        StatusDisplay {
            metric: "Connections".to_string(),
            value: status.active_connections.to_string(),
            status: if status.active_connections > 0 { "🟢" } else { "🔴" }.to_string(),
        },
        StatusDisplay {
            metric: "Hashrate".to_string(),
            value: format_hashrate(status.hashrate),
            status: if status.hashrate > 0.0 { "🟢" } else { "🔴" }.to_string(),
        },
        StatusDisplay {
            metric: "Shares (Valid/Total)".to_string(),
            value: format!("{}/{}", status.valid_shares, status.total_shares),
            status: if acceptance_rate >= 95.0 { "🟢" } else if acceptance_rate >= 90.0 { "🟡" } else { "🔴" }.to_string(),
        },
        StatusDisplay {
            metric: "Acceptance Rate".to_string(),
            value: format_percentage(acceptance_rate),
            status: if acceptance_rate >= 95.0 { "🟢" } else if acceptance_rate >= 90.0 { "🟡" } else { "🔴" }.to_string(),
        },
        StatusDisplay {
            metric: "Blocks Found".to_string(),
            value: status.blocks_found.to_string(),
            status: if status.blocks_found > 0 { "🎉" } else { "⏳" }.to_string(),
        },
    ];

    let table = Table::new(rows);
    println!("{}", table);
    println!();

    Ok(())
}

/// Display connections in monitor
async fn display_connections_monitor(client: &ApiClient, max_entries: usize) -> Result<()> {
    let connections = client.get_connections().await?;
    
    println!("{}", "🔗 Active Connections".bold());
    
    if connections.is_empty() {
        println!("No active connections");
    } else {
        #[derive(Tabled)]
        struct ConnectionDisplay {
            #[tabled(rename = "ID")]
            id: String,
            #[tabled(rename = "Address")]
            address: String,
            #[tabled(rename = "Protocol")]
            protocol: String,
            #[tabled(rename = "State")]
            state: String,
            #[tabled(rename = "Shares")]
            shares: String,
            #[tabled(rename = "Rate")]
            rate: String,
            #[tabled(rename = "Last Activity")]
            activity: String,
        }

        let connection_rows: Vec<ConnectionDisplay> = connections
            .iter()
            .take(max_entries)
            .map(|conn| {
                let state_icon = match conn.state {
                    sv2_core::types::ConnectionState::Connected => "🟢",
                    sv2_core::types::ConnectionState::Authenticated => "🔐",
                    sv2_core::types::ConnectionState::Connecting => "🟡",
                    sv2_core::types::ConnectionState::Disconnecting => "🟠",
                    sv2_core::types::ConnectionState::Disconnected => "🔴",
                    sv2_core::types::ConnectionState::Error(_) => "❌",
                };

                ConnectionDisplay {
                    id: conn.id.to_string()[..8].to_string(),
                    address: conn.address.to_string(),
                    protocol: format!("{:?}", conn.protocol),
                    state: format!("{} {:?}", state_icon, conn.state),
                    shares: format!("{}/{}", conn.valid_shares, conn.total_shares),
                    rate: format_percentage(conn.acceptance_rate()),
                    activity: conn.last_activity.format("%H:%M:%S").to_string(),
                }
            })
            .collect();

        let table = Table::new(connection_rows);
        println!("{}", table);
    }
    println!();

    Ok(())
}

/// Display recent shares in monitor
async fn display_shares_monitor(client: &ApiClient, max_entries: usize) -> Result<()> {
    let shares = client.get_shares(None, Some(max_entries as u32)).await?;
    
    println!("{}", "💎 Recent Shares".bold());
    
    if shares.is_empty() {
        println!("No recent shares");
    } else {
        #[derive(Tabled)]
        struct ShareDisplay {
            #[tabled(rename = "Connection")]
            connection: String,
            #[tabled(rename = "Difficulty")]
            difficulty: String,
            #[tabled(rename = "Valid")]
            valid: String,
            #[tabled(rename = "Block")]
            block: String,
            #[tabled(rename = "Time")]
            time: String,
        }

        let share_rows: Vec<ShareDisplay> = shares
            .iter()
            .take(max_entries)
            .map(|share| {
                ShareDisplay {
                    connection: share.connection_id.to_string()[..8].to_string(),
                    difficulty: format!("{:.2}", share.difficulty),
                    valid: if share.is_valid { "✅" } else { "❌" }.to_string(),
                    block: if share.block_hash.is_some() { "🎉" } else { "-" }.to_string(),
                    time: share.submitted_at.format("%H:%M:%S").to_string(),
                }
            })
            .collect();

        let table = Table::new(share_rows);
        println!("{}", table);
    }
    println!();

    Ok(())
}

/// Display performance metrics in monitor
async fn display_performance_monitor(client: &ApiClient) -> Result<()> {
    let metrics = client.get_metrics(Some(1)).await?;
    
    println!("{}", "⚡ Performance Metrics".bold());
    
    if let Some(latest) = metrics.first() {
        #[derive(Tabled)]
        struct MetricDisplay {
            #[tabled(rename = "Metric")]
            metric: String,
            #[tabled(rename = "Value")]
            value: String,
            #[tabled(rename = "Status")]
            status: String,
        }

        let cpu_status = if latest.cpu_usage < 70.0 { "🟢" } else if latest.cpu_usage < 90.0 { "🟡" } else { "🔴" };
        let memory_status = if latest.memory_usage_percent() < 70.0 { "🟢" } else if latest.memory_usage_percent() < 90.0 { "🟡" } else { "🔴" };
        let conn_status = if latest.open_connections < 900 { "🟢" } else if latest.open_connections < 950 { "🟡" } else { "🔴" };

        let metric_rows = vec![
            MetricDisplay {
                metric: "CPU Usage".to_string(),
                value: format_percentage(latest.cpu_usage),
                status: cpu_status.to_string(),
            },
            MetricDisplay {
                metric: "Memory Usage".to_string(),
                value: format_percentage(latest.memory_usage_percent()),
                status: memory_status.to_string(),
            },
            MetricDisplay {
                metric: "Open Connections".to_string(),
                value: latest.open_connections.to_string(),
                status: conn_status.to_string(),
            },
            MetricDisplay {
                metric: "DB Connections".to_string(),
                value: latest.database_connections.to_string(),
                status: "🟢".to_string(),
            },
        ];

        let table = Table::new(metric_rows);
        println!("{}", table);
    } else {
        println!("No performance metrics available");
    }
    println!();

    Ok(())
}

/// Display alerts in monitor
async fn display_alerts_monitor(client: &ApiClient, max_entries: usize) -> Result<()> {
    let alerts = client.get_alerts(Some(max_entries as u32)).await?;
    
    println!("{}", "🚨 System Alerts".bold());
    
    if alerts.is_empty() {
        println!("🟢 No active alerts");
    } else {
        #[derive(Tabled)]
        struct AlertDisplay {
            #[tabled(rename = "Level")]
            level: String,
            #[tabled(rename = "Component")]
            component: String,
            #[tabled(rename = "Title")]
            title: String,
            #[tabled(rename = "Time")]
            time: String,
            #[tabled(rename = "Status")]
            status: String,
        }

        let alert_rows: Vec<AlertDisplay> = alerts
            .iter()
            .take(max_entries)
            .map(|alert| {
                let (level_icon, level_str) = match alert.level {
                    sv2_core::types::AlertLevel::Info => ("ℹ️", "INFO"),
                    sv2_core::types::AlertLevel::Warning => ("⚠️", "WARN"),
                    sv2_core::types::AlertLevel::Error => ("❌", "ERROR"),
                    sv2_core::types::AlertLevel::Critical => ("🚨", "CRIT"),
                };

                let status = if alert.is_resolved() { "✅ Resolved" } else { "🔴 Active" };

                AlertDisplay {
                    level: format!("{} {}", level_icon, level_str),
                    component: alert.component.clone(),
                    title: alert.title.clone(),
                    time: alert.created_at.format("%H:%M:%S").to_string(),
                    status: status.to_string(),
                }
            })
            .collect();

        let table = Table::new(alert_rows);
        println!("{}", table);
    }
    println!();

    Ok(())
}

/// Handle configuration management commands
pub async fn handle_config_get(client: &ApiClient, key: Option<String>) -> Result<()> {
    print_info("Retrieving daemon configuration...");
    
    if !check_daemon_connection(client).await.unwrap_or(false) {
        return Ok(());
    }

    let config = client.get_config().await
        .context("Failed to get configuration")?;

    if let Some(key_path) = key {
        // Get specific configuration key
        let config_value = serde_json::to_value(&config)?;
        let keys: Vec<&str> = key_path.split('.').collect();
        
        let mut current = &config_value;
        for key in &keys {
            current = current.get(key).ok_or_else(|| {
                anyhow::anyhow!("Configuration key '{}' not found", key_path)
            })?;
        }
        
        println!("{}: {}", key_path, serde_json::to_string_pretty(current)?);
    } else {
        // Display full configuration
        let config_json = serde_json::to_string_pretty(&config)?;
        println!("{}", config_json);
    }

    Ok(())
}

/// Handle configuration validation
pub async fn handle_config_validate(client: &ApiClient, config_file: Option<String>) -> Result<()> {
    print_info("Validating configuration...");
    
    if !check_daemon_connection(client).await.unwrap_or(false) {
        return Ok(());
    }

    let config = if let Some(file_path) = config_file {
        // Load configuration from file
        print_info(&format!("Loading configuration from: {}", file_path));
        let config_content = std::fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read config file: {}", file_path))?;
        
        toml::from_str(&config_content)
            .with_context(|| format!("Failed to parse config file: {}", file_path))?
    } else {
        // Use current configuration
        client.get_config().await
            .context("Failed to get current configuration")?
    };

    // Validate configuration
    match client.validate_config(&config).await {
        Ok(response) => {
            if response.success {
                print_success("Configuration validation passed");
                println!("✅ {}", response.message);
            } else {
                print_error("Configuration validation failed");
                println!("❌ {}", response.message);
                
                if let Some(errors) = response.validation_errors {
                    println!("\nValidation errors:");
                    for error in errors {
                        println!("  • {}", error);
                    }
                }
            }
        }
        Err(e) => {
            print_error(&format!("Failed to validate configuration: {}", e));
        }
    }

    Ok(())
}

/// Handle diagnostic commands
pub async fn handle_diagnostics(client: &ApiClient) -> Result<()> {
    print_info("Running system diagnostics...");
    
    println!("\n{}", "🔧 System Diagnostics".bold().blue());
    println!("{}", "─".repeat(50));

    // Test daemon connectivity
    print!("1. Daemon connectivity... ");
    match client.ping().await {
        Ok(true) => println!("{}", "✅ OK".green()),
        Ok(false) => println!("{}", "❌ FAILED - Daemon not responding".red()),
        Err(e) => println!("{}", format!("❌ ERROR - {}", e).red()),
    }

    // Test API endpoints
    print!("2. API endpoints... ");
    let mut api_ok = true;
    
    if let Err(_) = client.get_status().await {
        api_ok = false;
    }
    
    if api_ok {
        println!("{}", "✅ OK".green());
    } else {
        println!("{}", "❌ FAILED - API endpoints not responding".red());
    }

    // Check system resources
    print!("3. System resources... ");
    match client.get_metrics(Some(1)).await {
        Ok(metrics) => {
            if let Some(latest) = metrics.first() {
                let cpu_ok = latest.cpu_usage < 90.0;
                let memory_ok = latest.memory_usage_percent() < 90.0;
                
                if cpu_ok && memory_ok {
                    println!("{}", "✅ OK".green());
                } else {
                    println!("{}", "⚠️ WARNING - High resource usage".yellow());
                    if !cpu_ok {
                        println!("   CPU usage: {:.1}%", latest.cpu_usage);
                    }
                    if !memory_ok {
                        println!("   Memory usage: {:.1}%", latest.memory_usage_percent());
                    }
                }
            } else {
                println!("{}", "❌ FAILED - No metrics available".red());
            }
        }
        Err(_) => {
            println!("{}", "❌ FAILED - Cannot retrieve metrics".red());
        }
    }

    // Check for active alerts
    print!("4. System alerts... ");
    match client.get_alerts(Some(10)).await {
        Ok(alerts) => {
            let active_alerts: Vec<_> = alerts.iter().filter(|a| !a.is_resolved()).collect();
            
            if active_alerts.is_empty() {
                println!("{}", "✅ OK".green());
            } else {
                println!("{}", format!("⚠️ {} active alerts", active_alerts.len()).yellow());
                for alert in active_alerts.iter().take(3) {
                    println!("   • {}: {}", alert.component, alert.title);
                }
                if active_alerts.len() > 3 {
                    println!("   ... and {} more", active_alerts.len() - 3);
                }
            }
        }
        Err(_) => {
            println!("{}", "❌ FAILED - Cannot retrieve alerts".red());
        }
    }

    // Check connections
    print!("5. Mining connections... ");
    match client.get_connections().await {
        Ok(connections) => {
            if connections.is_empty() {
                println!("{}", "⚠️ WARNING - No active connections".yellow());
            } else {
                let healthy_connections = connections.iter()
                    .filter(|c| matches!(c.state, sv2_core::types::ConnectionState::Connected | sv2_core::types::ConnectionState::Authenticated))
                    .count();
                
                if healthy_connections == connections.len() {
                    println!("{}", format!("✅ OK - {} healthy connections", healthy_connections).green());
                } else {
                    println!("{}", format!("⚠️ WARNING - {}/{} connections healthy", healthy_connections, connections.len()).yellow());
                }
            }
        }
        Err(_) => {
            println!("{}", "❌ FAILED - Cannot retrieve connections".red());
        }
    }

    println!("\n{}", "Diagnostics complete".bold());

    Ok(())
}

/// Handle troubleshooting command
pub async fn handle_troubleshoot(client: &ApiClient) -> Result<()> {
    print_info("Running troubleshooting analysis...");
    
    println!("\n{}", "🔍 Troubleshooting Analysis".bold().blue());
    println!("{}", "─".repeat(50));

    let mut issues_found = 0;

    // Check daemon status
    match client.get_status().await {
        Ok(status) => {
            // Check for low hashrate
            if status.hashrate == 0.0 && status.active_connections > 0 {
                issues_found += 1;
                println!("❌ Issue {}: No hashrate detected despite active connections", issues_found);
                println!("   Possible causes:");
                println!("   • Miners not submitting shares");
                println!("   • Incorrect difficulty settings");
                println!("   • Network connectivity issues");
                println!("   Solutions:");
                println!("   • Check miner configuration");
                println!("   • Verify pool/proxy settings");
                println!("   • Review logs for errors");
                println!();
            }

            // Check acceptance rate
            if status.total_shares > 0 {
                let acceptance_rate = (status.valid_shares as f64 / status.total_shares as f64) * 100.0;
                if acceptance_rate < 90.0 {
                    issues_found += 1;
                    println!("❌ Issue {}: Low share acceptance rate ({:.1}%)", issues_found, acceptance_rate);
                    println!("   Possible causes:");
                    println!("   • Difficulty too high for miners");
                    println!("   • Network latency issues");
                    println!("   • Stale work submissions");
                    println!("   Solutions:");
                    println!("   • Lower difficulty settings");
                    println!("   • Check network connectivity");
                    println!("   • Verify time synchronization");
                    println!();
                }
            }

            // Check for no connections
            if status.active_connections == 0 {
                issues_found += 1;
                println!("❌ Issue {}: No active mining connections", issues_found);
                println!("   Possible causes:");
                println!("   • Miners not configured correctly");
                println!("   • Network firewall blocking connections");
                println!("   • Daemon not listening on correct address");
                println!("   Solutions:");
                println!("   • Verify listen address configuration");
                println!("   • Check firewall settings");
                println!("   • Test connectivity from miners");
                println!();
            }
        }
        Err(e) => {
            issues_found += 1;
            println!("❌ Issue {}: Cannot retrieve daemon status", issues_found);
            println!("   Error: {}", e);
            println!("   Solutions:");
            println!("   • Check if daemon is running");
            println!("   • Verify API endpoint configuration");
            println!("   • Check daemon logs for errors");
            println!();
        }
    }

    // Check system resources
    if let Ok(metrics) = client.get_metrics(Some(1)).await {
        if let Some(latest) = metrics.first() {
            if latest.cpu_usage > 90.0 {
                issues_found += 1;
                println!("❌ Issue {}: High CPU usage ({:.1}%)", issues_found, latest.cpu_usage);
                println!("   Solutions:");
                println!("   • Reduce connection limits");
                println!("   • Optimize configuration");
                println!("   • Consider hardware upgrade");
                println!();
            }

            if latest.memory_usage_percent() > 90.0 {
                issues_found += 1;
                println!("❌ Issue {}: High memory usage ({:.1}%)", issues_found, latest.memory_usage_percent());
                println!("   Solutions:");
                println!("   • Reduce connection limits");
                println!("   • Clear old share data");
                println!("   • Restart daemon if needed");
                println!();
            }
        }
    }

    // Check for active alerts
    if let Ok(alerts) = client.get_alerts(Some(10)).await {
        let critical_alerts: Vec<_> = alerts.iter()
            .filter(|a| !a.is_resolved() && matches!(a.level, sv2_core::types::AlertLevel::Critical | sv2_core::types::AlertLevel::Error))
            .collect();

        for alert in critical_alerts {
            issues_found += 1;
            println!("❌ Issue {}: {} - {}", issues_found, alert.component, alert.title);
            println!("   Message: {}", alert.message);
            println!("   Time: {}", alert.created_at.format("%Y-%m-%d %H:%M:%S UTC"));
            println!();
        }
    }

    if issues_found == 0 {
        println!("✅ No issues detected - system appears to be running normally");
        println!("\nFor additional help:");
        println!("• Check logs: sv2-cli logs");
        println!("• Run diagnostics: sv2-cli diagnostics");
        println!("• Monitor real-time: sv2-cli monitor");
    } else {
        println!("Found {} potential issues. Review the suggestions above.", issues_found);
        println!("\nFor more detailed analysis:");
        println!("• Check daemon logs for specific errors");
        println!("• Monitor system in real-time: sv2-cli monitor");
        println!("• Verify configuration: sv2-cli config validate");
    }

    Ok(())
}