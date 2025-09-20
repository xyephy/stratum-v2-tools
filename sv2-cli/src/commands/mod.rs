pub mod auth;
pub mod status;
pub mod daemon_control;
pub mod setup;
pub mod monitor;

use anyhow::Result;
use colored::*;
use std::fmt;

/// Output format options
#[derive(Debug, Clone, Copy)]
pub enum OutputFormat {
    Table,
    Json,
    Yaml,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Table => write!(f, "table"),
            OutputFormat::Json => write!(f, "json"),
            OutputFormat::Yaml => write!(f, "yaml"),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "table" => Ok(OutputFormat::Table),
            "json" => Ok(OutputFormat::Json),
            "yaml" => Ok(OutputFormat::Yaml),
            _ => Err(anyhow::anyhow!("Invalid output format: {}. Valid formats: table, json, yaml", s)),
        }
    }
}

/// Print success message
pub fn print_success(message: &str) {
    println!("{} {}", "✓".green().bold(), message);
}

/// Print error message
pub fn print_error(message: &str) {
    eprintln!("{} {}", "✗".red().bold(), message);
}

/// Print warning message
pub fn print_warning(message: &str) {
    println!("{} {}", "⚠".yellow().bold(), message);
}

/// Print info message
pub fn print_info(message: &str) {
    println!("{} {}", "ℹ".blue().bold(), message);
}

/// Format duration for display
pub fn format_duration(duration: std::time::Duration) -> String {
    let total_seconds = duration.as_secs();
    let days = total_seconds / 86400;
    let hours = (total_seconds % 86400) / 3600;
    let minutes = (total_seconds % 3600) / 60;
    let seconds = total_seconds % 60;

    if days > 0 {
        format!("{}d {}h {}m {}s", days, hours, minutes, seconds)
    } else if hours > 0 {
        format!("{}h {}m {}s", hours, minutes, seconds)
    } else if minutes > 0 {
        format!("{}m {}s", minutes, seconds)
    } else {
        format!("{}s", seconds)
    }
}

/// Format bytes for display
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KB", "MB", "GB", "TB"];
    let mut size = bytes as f64;
    let mut unit_index = 0;

    while size >= 1024.0 && unit_index < UNITS.len() - 1 {
        size /= 1024.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{} {}", bytes, UNITS[unit_index])
    } else {
        format!("{:.2} {}", size, UNITS[unit_index])
    }
}

/// Format hashrate for display
pub fn format_hashrate(hashrate: f64) -> String {
    const UNITS: &[&str] = &["H/s", "KH/s", "MH/s", "GH/s", "TH/s", "PH/s"];
    let mut rate = hashrate;
    let mut unit_index = 0;

    while rate >= 1000.0 && unit_index < UNITS.len() - 1 {
        rate /= 1000.0;
        unit_index += 1;
    }

    if unit_index == 0 {
        format!("{:.0} {}", rate, UNITS[unit_index])
    } else {
        format!("{:.2} {}", rate, UNITS[unit_index])
    }
}

/// Format percentage for display
pub fn format_percentage(value: f64) -> String {
    format!("{:.2}%", value)
}

/// Check if daemon is reachable and print connection status
pub async fn check_daemon_connection(client: &crate::client::ApiClient) -> Result<bool> {
    print_info("Checking daemon connection...");
    
    match client.ping().await {
        Ok(true) => {
            print_success("Daemon is reachable");
            Ok(true)
        }
        Ok(false) => {
            print_error("Daemon is not responding");
            Ok(false)
        }
        Err(e) => {
            print_error(&format!("Failed to connect to daemon: {}", e));
            Err(e)
        }
    }
}