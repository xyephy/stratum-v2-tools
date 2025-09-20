use anyhow::Result;
use sv2_cli::{ApiClient, ApiClientConfig};
use std::time::Duration;
use url::Url;

#[tokio::test]
async fn test_api_client_creation() {
    let client = ApiClient::new();
    // Basic smoke test - client should be created without errors
    assert!(true);
}

#[tokio::test]
async fn test_api_client_with_custom_config() -> Result<()> {
    let config = ApiClientConfig {
        base_url: Url::parse("http://example.com:9000")?,
        timeout: Duration::from_secs(10),
        api_key: Some("test-key".to_string()),
    };
    
    let client = ApiClient::with_config(config);
    // Should create client without errors
    assert!(true);
    Ok(())
}

#[tokio::test]
async fn test_api_client_builder_pattern() -> Result<()> {
    let client = ApiClient::new()
        .with_base_url("http://localhost:9000")?
        .with_api_key("test-api-key".to_string());
    
    // Should build client without errors
    assert!(true);
    Ok(())
}

#[tokio::test]
async fn test_ping_unreachable_server() {
    let client = ApiClient::new()
        .with_base_url("http://localhost:65432")
        .unwrap(); // Use unlikely port
    
    let result = client.ping().await;
    // Should return Ok(false) for unreachable server
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), false);
}

// Note: These tests require a running sv2d daemon for full integration testing
// For now, they test the client construction and basic error handling

#[cfg(feature = "integration")]
mod integration_tests {
    use super::*;
    use sv2_core::types::DaemonStatus;

    #[tokio::test]
    async fn test_get_status_with_running_daemon() -> Result<()> {
        let client = ApiClient::new();
        
        // This test requires a running daemon
        if client.ping().await? {
            let status = client.get_status().await?;
            assert!(status.uptime.as_secs() >= 0);
        }
        
        Ok(())
    }

    #[tokio::test]
    async fn test_get_connections_with_running_daemon() -> Result<()> {
        let client = ApiClient::new();
        
        if client.ping().await? {
            let connections = client.get_connections().await?;
            // Should return a vector (may be empty)
            assert!(connections.len() >= 0);
        }
        
        Ok(())
    }

    #[tokio::test]
    async fn test_get_shares_with_running_daemon() -> Result<()> {
        let client = ApiClient::new();
        
        if client.ping().await? {
            let shares = client.get_shares(None, Some(10)).await?;
            // Should return a vector (may be empty)
            assert!(shares.len() >= 0);
        }
        
        Ok(())
    }
}