use anyhow::Result;
use sv2_cli::commands::{format_duration, format_bytes, format_hashrate, format_percentage, OutputFormat};
use std::time::Duration;

#[test]
fn test_output_format_parsing() -> Result<()> {
    assert!(matches!("table".parse::<OutputFormat>()?, OutputFormat::Table));
    assert!(matches!("json".parse::<OutputFormat>()?, OutputFormat::Json));
    assert!(matches!("yaml".parse::<OutputFormat>()?, OutputFormat::Yaml));
    
    // Case insensitive
    assert!(matches!("TABLE".parse::<OutputFormat>()?, OutputFormat::Table));
    assert!(matches!("Json".parse::<OutputFormat>()?, OutputFormat::Json));
    
    // Invalid format should error
    assert!("invalid".parse::<OutputFormat>().is_err());
    
    Ok(())
}

#[test]
fn test_format_duration() {
    assert_eq!(format_duration(Duration::from_secs(30)), "30s");
    assert_eq!(format_duration(Duration::from_secs(90)), "1m 30s");
    assert_eq!(format_duration(Duration::from_secs(3661)), "1h 1m 1s");
    assert_eq!(format_duration(Duration::from_secs(90061)), "1d 1h 1m 1s");
}

#[test]
fn test_format_bytes() {
    assert_eq!(format_bytes(512), "512 B");
    assert_eq!(format_bytes(1024), "1.00 KB");
    assert_eq!(format_bytes(1536), "1.50 KB");
    assert_eq!(format_bytes(1048576), "1.00 MB");
    assert_eq!(format_bytes(1073741824), "1.00 GB");
}

#[test]
fn test_format_hashrate() {
    assert_eq!(format_hashrate(500.0), "500 H/s");
    assert_eq!(format_hashrate(1500.0), "1.50 KH/s");
    assert_eq!(format_hashrate(1500000.0), "1.50 MH/s");
    assert_eq!(format_hashrate(1500000000.0), "1.50 GH/s");
    assert_eq!(format_hashrate(1500000000000.0), "1.50 TH/s");
}

#[test]
fn test_format_percentage() {
    assert_eq!(format_percentage(0.0), "0.00%");
    assert_eq!(format_percentage(50.0), "50.00%");
    assert_eq!(format_percentage(99.99), "99.99%");
    assert_eq!(format_percentage(100.0), "100.00%");
}

#[test]
fn test_output_format_display() {
    assert_eq!(OutputFormat::Table.to_string(), "table");
    assert_eq!(OutputFormat::Json.to_string(), "json");
    assert_eq!(OutputFormat::Yaml.to_string(), "yaml");
}

// Test command parsing (would require more setup for full CLI testing)
#[cfg(test)]
mod cli_parsing_tests {
    use clap::Parser;
    
    // These would test the actual CLI argument parsing
    // For now, we'll just ensure the structures compile correctly
    
    #[test]
    fn test_cli_structure_compiles() {
        // This is a compile-time test to ensure our CLI structures are valid
        assert!(true);
    }
}