use anyhow::{Context, Result};
use colored::*;
use serde_json;
use serde_yaml;
use tabled::{Table, Tabled};

use crate::client::ApiClient;
use super::{OutputFormat, print_success, print_error, print_info, format_duration, format_hashrate, format_percentage, check_daemon_connection};

/// Handle the status command
pub async fn handle_status(client: &ApiClient, detailed: bool, format: &str) -> Result<()> {
    let output_format: OutputFormat = format.parse()
        .context("Invalid output format")?;

    // Check daemon connection first
    if !check_daemon_connection(client).await.unwrap_or(false) {
        return Ok(());
    }

    // Get daemon status
    print_info("Fetching daemon status...");
    let status = client.get_status().await
        .context("Failed to get daemon status")?;

    match output_format {
        OutputFormat::Json => {
            let json = serde_json::to_string_pretty(&status)
                .context("Failed to serialize status to JSON")?;
            println!("{}", json);
        }
        OutputFormat::Yaml => {
            let yaml = serde_yaml::to_string(&status)
                .context("Failed to serialize status to YAML")?;
            println!("{}", yaml);
        }
        OutputFormat::Table => {
            print_status_table(&status, detailed).await?;
        }
    }

    if detailed {
        print_detailed_status(client).await?;
    }

    Ok(())
}

/// Print status information in table format
async fn print_status_table(status: &sv2_core::types::DaemonStatus, detailed: bool) -> Result<()> {
    println!("\n{}", "Daemon Status".bold().underline());
    
    #[derive(Tabled)]
    struct StatusRow {
        #[tabled(rename = "Metric")]
        metric: String,
        #[tabled(rename = "Value")]
        value: String,
    }

    let mut rows = vec![
        StatusRow {
            metric: "Uptime".to_string(),
            value: format_duration(status.uptime),
        },
        StatusRow {
            metric: "Active Connections".to_string(),
            value: status.active_connections.to_string(),
        },
        StatusRow {
            metric: "Total Shares".to_string(),
            value: status.total_shares.to_string(),
        },
        StatusRow {
            metric: "Valid Shares".to_string(),
            value: status.valid_shares.to_string(),
        },
        StatusRow {
            metric: "Blocks Found".to_string(),
            value: status.blocks_found.to_string(),
        },
        StatusRow {
            metric: "Current Difficulty".to_string(),
            value: format!("{:.2}", status.current_difficulty),
        },
        StatusRow {
            metric: "Hashrate".to_string(),
            value: format_hashrate(status.hashrate),
        },
    ];

    // Add acceptance rate if we have shares
    if status.total_shares > 0 {
        let acceptance_rate = (status.valid_shares as f64 / status.total_shares as f64) * 100.0;
        rows.push(StatusRow {
            metric: "Acceptance Rate".to_string(),
            value: format_percentage(acceptance_rate),
        });
    }

    let table = Table::new(rows);
    println!("{}", table);

    // Print status indicators
    println!("\n{}", "Status Indicators".bold().underline());
    
    // Connection status
    if status.active_connections > 0 {
        print_success(&format!("{} active connections", status.active_connections));
    } else {
        print_error("No active connections");
    }

    // Mining status
    if status.hashrate > 0.0 {
        print_success(&format!("Mining at {}", format_hashrate(status.hashrate)));
    } else {
        print_error("No mining activity detected");
    }

    // Block finding status
    if status.blocks_found > 0 {
        print_success(&format!("{} blocks found", status.blocks_found));
    }

    Ok(())
}

/// Print detailed status information
async fn print_detailed_status(client: &ApiClient) -> Result<()> {
    println!("\n{}", "Detailed Information".bold().underline());

    // Get connections
    print_info("Fetching connection details...");
    match client.get_connections().await {
        Ok(connections) => {
            if connections.is_empty() {
                println!("No active connections");
            } else {
                println!("\n{}", "Active Connections".bold());
                
                #[derive(Tabled)]
                struct ConnectionRow {
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
                    #[tabled(rename = "Acceptance Rate")]
                    acceptance_rate: String,
                }

                let connection_rows: Vec<ConnectionRow> = connections.iter().map(|conn| {
                    ConnectionRow {
                        id: conn.id.to_string()[..8].to_string(),
                        address: conn.address.to_string(),
                        protocol: format!("{:?}", conn.protocol),
                        state: format!("{:?}", conn.state),
                        shares: format!("{}/{}", conn.valid_shares, conn.total_shares),
                        acceptance_rate: format_percentage(conn.acceptance_rate()),
                    }
                }).collect();

                let table = Table::new(connection_rows);
                println!("{}", table);
            }
        }
        Err(e) => {
            print_error(&format!("Failed to get connections: {}", e));
        }
    }

    // Get recent shares
    print_info("Fetching recent shares...");
    match client.get_shares(None, Some(10)).await {
        Ok(shares) => {
            if shares.is_empty() {
                println!("No recent shares");
            } else {
                println!("\n{}", "Recent Shares".bold());
                
                #[derive(Tabled)]
                struct ShareRow {
                    #[tabled(rename = "Connection")]
                    connection: String,
                    #[tabled(rename = "Difficulty")]
                    difficulty: String,
                    #[tabled(rename = "Valid")]
                    valid: String,
                    #[tabled(rename = "Block")]
                    block: String,
                    #[tabled(rename = "Submitted")]
                    submitted: String,
                }

                let share_rows: Vec<ShareRow> = shares.iter().take(10).map(|share| {
                    ShareRow {
                        connection: share.connection_id.to_string()[..8].to_string(),
                        difficulty: format!("{:.2}", share.difficulty),
                        valid: if share.is_valid { "✓".green().to_string() } else { "✗".red().to_string() },
                        block: if share.block_hash.is_some() { "✓".green().to_string() } else { "-".to_string() },
                        submitted: share.submitted_at.format("%H:%M:%S").to_string(),
                    }
                }).collect();

                let table = Table::new(share_rows);
                println!("{}", table);
            }
        }
        Err(e) => {
            print_error(&format!("Failed to get shares: {}", e));
        }
    }

    // Get alerts
    print_info("Fetching system alerts...");
    match client.get_alerts(Some(5)).await {
        Ok(alerts) => {
            if alerts.is_empty() {
                print_success("No active alerts");
            } else {
                println!("\n{}", "Recent Alerts".bold());
                
                #[derive(Tabled)]
                struct AlertRow {
                    #[tabled(rename = "Level")]
                    level: String,
                    #[tabled(rename = "Component")]
                    component: String,
                    #[tabled(rename = "Title")]
                    title: String,
                    #[tabled(rename = "Time")]
                    time: String,
                }

                let alert_rows: Vec<AlertRow> = alerts.iter().map(|alert| {
                    let level_str = match alert.level {
                        sv2_core::types::AlertLevel::Info => "INFO".blue().to_string(),
                        sv2_core::types::AlertLevel::Warning => "WARN".yellow().to_string(),
                        sv2_core::types::AlertLevel::Error => "ERROR".red().to_string(),
                        sv2_core::types::AlertLevel::Critical => "CRIT".red().bold().to_string(),
                    };

                    AlertRow {
                        level: level_str,
                        component: alert.component.clone(),
                        title: alert.title.clone(),
                        time: alert.created_at.format("%H:%M:%S").to_string(),
                    }
                }).collect();

                let table = Table::new(alert_rows);
                println!("{}", table);
            }
        }
        Err(e) => {
            print_error(&format!("Failed to get alerts: {}", e));
        }
    }

    // Get performance metrics
    print_info("Fetching performance metrics...");
    match client.get_metrics(Some(1)).await {
        Ok(metrics) => {
            if let Some(latest) = metrics.first() {
                println!("\n{}", "Performance Metrics".bold());
                
                #[derive(Tabled)]
                struct MetricRow {
                    #[tabled(rename = "Metric")]
                    metric: String,
                    #[tabled(rename = "Value")]
                    value: String,
                }

                let metric_rows = vec![
                    MetricRow {
                        metric: "CPU Usage".to_string(),
                        value: format_percentage(latest.cpu_usage),
                    },
                    MetricRow {
                        metric: "Memory Usage".to_string(),
                        value: format_percentage(latest.memory_usage_percent()),
                    },
                    MetricRow {
                        metric: "Open Connections".to_string(),
                        value: latest.open_connections.to_string(),
                    },
                    MetricRow {
                        metric: "Database Connections".to_string(),
                        value: latest.database_connections.to_string(),
                    },
                ];

                let table = Table::new(metric_rows);
                println!("{}", table);
            }
        }
        Err(e) => {
            print_error(&format!("Failed to get metrics: {}", e));
        }
    }

    Ok(())
}