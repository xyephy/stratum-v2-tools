use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use sv2_core::{
    config::DaemonConfig, 
    types::{DaemonStatus, ConnectionInfo, Share, WorkTemplate, PerformanceMetrics, Alert, MiningStats},
    database::ShareStats,
};
use url::Url;
use uuid::Uuid;

/// Configuration for the API client
#[derive(Debug, Clone)]
pub struct ApiClientConfig {
    pub base_url: Url,
    pub timeout: Duration,
    pub api_key: Option<String>,
}

impl Default for ApiClientConfig {
    fn default() -> Self {
        Self {
            base_url: Url::parse("http://localhost:8080").unwrap(),
            timeout: Duration::from_secs(30),
            api_key: None,
        }
    }
}

/// API client for communicating with sv2d daemon
#[derive(Debug, Clone)]
pub struct ApiClient {
    client: Client,
    config: ApiClientConfig,
}

/// Response wrapper for API calls
#[derive(Debug, Deserialize)]
pub struct ApiResponse<T> {
    pub data: Option<T>,
    pub error: Option<String>,
    pub success: bool,
}

/// Configuration update request
#[derive(Debug, Serialize)]
pub struct ConfigUpdateRequest {
    pub config: serde_json::Value,
    pub validate_only: Option<bool>,
}

/// Configuration update response
#[derive(Debug, Deserialize)]
pub struct ConfigUpdateResponse {
    pub success: bool,
    pub message: String,
    pub validation_errors: Option<Vec<String>>,
}

/// Daemon control commands
#[derive(Debug, Clone, Copy)]
pub enum DaemonCommand {
    Start,
    Stop,
    Restart,
    Reload,
}

/// Daemon control response
#[derive(Debug, Deserialize)]
pub struct DaemonControlResponse {
    pub success: bool,
    pub message: String,
    pub pid: Option<u32>,
}

impl ApiClient {
    /// Create a new API client with default configuration
    pub fn new() -> Self {
        Self::with_config(ApiClientConfig::default())
    }

    /// Create a new API client with custom configuration
    pub fn with_config(config: ApiClientConfig) -> Self {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .expect("Failed to create HTTP client");

        Self { client, config }
    }

    /// Set the base URL for the API
    pub fn with_base_url(mut self, url: &str) -> Result<Self> {
        self.config.base_url = Url::parse(url)
            .with_context(|| format!("Invalid base URL: {}", url))?;
        Ok(self)
    }

    /// Set the API key for authentication
    pub fn with_api_key(mut self, api_key: String) -> Self {
        self.config.api_key = Some(api_key);
        self
    }

    /// Build a URL for an API endpoint
    fn build_url(&self, path: &str) -> Result<Url> {
        self.config.base_url.join(path)
            .with_context(|| format!("Failed to build URL for path: {}", path))
    }

    /// Build a request with common headers
    fn build_request(&self, method: reqwest::Method, url: Url) -> reqwest::RequestBuilder {
        let mut request = self.client.request(method, url);
        
        if let Some(api_key) = &self.config.api_key {
            request = request.header("Authorization", format!("Bearer {}", api_key));
        }
        
        request.header("Content-Type", "application/json")
    }

    /// Check if the daemon is reachable
    pub async fn ping(&self) -> Result<bool> {
        let url = self.build_url("/api/v1/health")?;
        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await;

        match response {
            Ok(resp) => Ok(resp.status().is_success()),
            Err(_) => Ok(false),
        }
    }

    /// Get daemon status
    pub async fn get_status(&self) -> Result<DaemonStatus> {
        let url = self.build_url("/api/v1/status")?;
        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send status request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Status request failed with status: {}",
                response.status()
            ));
        }

        let status: DaemonStatus = response.json().await
            .context("Failed to parse status response")?;

        Ok(status)
    }

    /// Get active connections
    pub async fn get_connections(&self) -> Result<Vec<ConnectionInfo>> {
        let url = self.build_url("/api/v1/connections")?;
        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send connections request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Connections request failed with status: {}",
                response.status()
            ));
        }

        let connections: Vec<ConnectionInfo> = response.json().await
            .context("Failed to parse connections response")?;

        Ok(connections)
    }

    /// Get connection by ID
    pub async fn get_connection(&self, id: Uuid) -> Result<ConnectionInfo> {
        let url = self.build_url(&format!("/api/v1/connections/{}", id))?;
        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send connection request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Connection request failed with status: {}",
                response.status()
            ));
        }

        let connection: ConnectionInfo = response.json().await
            .context("Failed to parse connection response")?;

        Ok(connection)
    }

    /// Get shares
    pub async fn get_shares(&self, connection_id: Option<Uuid>, limit: Option<u32>) -> Result<Vec<Share>> {
        let mut url = self.build_url("/api/v1/shares")?;
        
        // Add query parameters
        let mut query_pairs = url.query_pairs_mut();
        if let Some(conn_id) = connection_id {
            query_pairs.append_pair("connection_id", &conn_id.to_string());
        }
        if let Some(limit) = limit {
            query_pairs.append_pair("limit", &limit.to_string());
        }
        drop(query_pairs);

        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send shares request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Shares request failed with status: {}",
                response.status()
            ));
        }

        let shares: Vec<Share> = response.json().await
            .context("Failed to parse shares response")?;

        Ok(shares)
    }

    /// Get share statistics
    pub async fn get_share_stats(&self, connection_id: Option<Uuid>) -> Result<ShareStats> {
        let mut url = self.build_url("/api/v1/shares/stats")?;
        
        if let Some(conn_id) = connection_id {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("connection_id", &conn_id.to_string());
        }

        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send share stats request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Share stats request failed with status: {}",
                response.status()
            ));
        }

        let stats: ShareStats = response.json().await
            .context("Failed to parse share stats response")?;

        Ok(stats)
    }

    /// Get performance metrics
    pub async fn get_metrics(&self, limit: Option<u32>) -> Result<Vec<PerformanceMetrics>> {
        let mut url = self.build_url("/api/v1/metrics")?;
        
        if let Some(limit) = limit {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("limit", &limit.to_string());
        }

        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send metrics request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Metrics request failed with status: {}",
                response.status()
            ));
        }

        let metrics: Vec<PerformanceMetrics> = response.json().await
            .context("Failed to parse metrics response")?;

        Ok(metrics)
    }

    /// Get work templates
    pub async fn get_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>> {
        let mut url = self.build_url("/api/v1/templates")?;
        
        if let Some(limit) = limit {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("limit", &limit.to_string());
        }

        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send templates request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Templates request failed with status: {}",
                response.status()
            ));
        }

        let templates: Vec<WorkTemplate> = response.json().await
            .context("Failed to parse templates response")?;

        Ok(templates)
    }

    /// Get system alerts
    pub async fn get_alerts(&self, limit: Option<u32>) -> Result<Vec<Alert>> {
        let mut url = self.build_url("/api/v1/alerts")?;
        
        if let Some(limit) = limit {
            let mut query_pairs = url.query_pairs_mut();
            query_pairs.append_pair("limit", &limit.to_string());
        }

        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send alerts request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Alerts request failed with status: {}",
                response.status()
            ));
        }

        let alerts: Vec<Alert> = response.json().await
            .context("Failed to parse alerts response")?;

        Ok(alerts)
    }

    /// Get mining statistics
    pub async fn get_mining_stats(&self) -> Result<MiningStats> {
        let url = self.build_url("/api/v1/mining/stats")?;
        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send mining stats request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Mining stats request failed with status: {}",
                response.status()
            ));
        }

        let stats: MiningStats = response.json().await
            .context("Failed to parse mining stats response")?;

        Ok(stats)
    }

    /// Get current configuration
    pub async fn get_config(&self) -> Result<DaemonConfig> {
        let url = self.build_url("/api/v1/config")?;
        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .context("Failed to send config request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Config request failed with status: {}",
                response.status()
            ));
        }

        let config: DaemonConfig = response.json().await
            .context("Failed to parse config response")?;

        Ok(config)
    }

    /// Update configuration
    pub async fn update_config(&self, config: &DaemonConfig, validate_only: bool) -> Result<ConfigUpdateResponse> {
        let url = self.build_url("/api/v1/config")?;
        
        let config_value = serde_json::to_value(config)
            .context("Failed to serialize config")?;
        
        let request = ConfigUpdateRequest {
            config: config_value,
            validate_only: Some(validate_only),
        };

        let response = self.build_request(reqwest::Method::POST, url)
            .json(&request)
            .send()
            .await
            .context("Failed to send config update request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Config update request failed with status: {}",
                response.status()
            ));
        }

        let update_response: ConfigUpdateResponse = response.json().await
            .context("Failed to parse config update response")?;

        Ok(update_response)
    }

    /// Validate configuration without applying it
    pub async fn validate_config(&self, config: &DaemonConfig) -> Result<ConfigUpdateResponse> {
        self.update_config(config, true).await
    }

    /// Send daemon control command
    pub async fn control_daemon(&self, command: DaemonCommand) -> Result<DaemonControlResponse> {
        let endpoint = match command {
            DaemonCommand::Start => "/api/v1/daemon/start",
            DaemonCommand::Stop => "/api/v1/daemon/stop",
            DaemonCommand::Restart => "/api/v1/daemon/restart",
            DaemonCommand::Reload => "/api/v1/daemon/reload",
        };

        let url = self.build_url(endpoint)?;
        let response = self.build_request(reqwest::Method::POST, url)
            .send()
            .await
            .context("Failed to send daemon control request")?;

        if !response.status().is_success() {
            return Err(anyhow::anyhow!(
                "Daemon control request failed with status: {}",
                response.status()
            ));
        }

        let control_response: DaemonControlResponse = response.json().await
            .context("Failed to parse daemon control response")?;

        Ok(control_response)
    }

    /// Start the daemon
    pub async fn start_daemon(&self) -> Result<DaemonControlResponse> {
        self.control_daemon(DaemonCommand::Start).await
    }

    /// Stop the daemon
    pub async fn stop_daemon(&self) -> Result<DaemonControlResponse> {
        self.control_daemon(DaemonCommand::Stop).await
    }

    /// Restart the daemon
    pub async fn restart_daemon(&self) -> Result<DaemonControlResponse> {
        self.control_daemon(DaemonCommand::Restart).await
    }

    /// Reload daemon configuration
    pub async fn reload_daemon(&self) -> Result<DaemonControlResponse> {
        self.control_daemon(DaemonCommand::Reload).await
    }

    /// Generic GET request
    pub async fn get<T>(&self, path: &str) -> Result<T>
    where
        T: for<'de> Deserialize<'de>,
    {
        let url = self.build_url(path)?;
        let response = self.build_request(reqwest::Method::GET, url)
            .send()
            .await
            .with_context(|| format!("Failed to send GET request to {}", path))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "GET request to {} failed with status {}: {}",
                path, status, error_text
            ));
        }

        let result: T = response.json().await
            .with_context(|| format!("Failed to parse response from {}", path))?;

        Ok(result)
    }

    /// Generic POST request
    pub async fn post<T, R>(&self, path: &str, body: &T) -> Result<R>
    where
        T: Serialize,
        R: for<'de> Deserialize<'de>,
    {
        let url = self.build_url(path)?;
        let response = self.build_request(reqwest::Method::POST, url)
            .json(body)
            .send()
            .await
            .with_context(|| format!("Failed to send POST request to {}", path))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "POST request to {} failed with status {}: {}",
                path, status, error_text
            ));
        }

        let result: R = response.json().await
            .with_context(|| format!("Failed to parse response from {}", path))?;

        Ok(result)
    }

    /// Generic DELETE request
    pub async fn delete(&self, path: &str) -> Result<()> {
        let url = self.build_url(path)?;
        let response = self.build_request(reqwest::Method::DELETE, url)
            .send()
            .await
            .with_context(|| format!("Failed to send DELETE request to {}", path))?;

        if !response.status().is_success() {
            let status = response.status();
            let error_text = response.text().await.unwrap_or_default();
            return Err(anyhow::anyhow!(
                "DELETE request to {} failed with status {}: {}",
                path, status, error_text
            ));
        }

        Ok(())
    }

    /// Create a new API client with authentication
    pub fn new_with_auth(base_url: &str, api_key: Option<String>) -> Result<Self> {
        let mut config = ApiClientConfig::default();
        config.base_url = Url::parse(base_url)
            .with_context(|| format!("Invalid base URL: {}", base_url))?;
        config.api_key = api_key;

        Ok(Self::with_config(config))
    }

    /// Get the base URL
    pub fn base_url(&self) -> &str {
        self.config.base_url.as_str()
    }
}

impl Default for ApiClient {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_client_creation() {
        let client = ApiClient::new();
        assert_eq!(client.config.base_url.as_str(), "http://localhost:8080/");
        assert_eq!(client.config.timeout, Duration::from_secs(30));
        assert!(client.config.api_key.is_none());
    }

    #[test]
    fn test_api_client_with_custom_url() {
        let client = ApiClient::new()
            .with_base_url("http://example.com:9000")
            .unwrap();
        assert_eq!(client.config.base_url.as_str(), "http://example.com:9000/");
    }

    #[test]
    fn test_api_client_with_api_key() {
        let client = ApiClient::new()
            .with_api_key("test-key".to_string());
        assert_eq!(client.config.api_key, Some("test-key".to_string()));
    }

    #[test]
    fn test_build_url() {
        let client = ApiClient::new();
        let url = client.build_url("/api/v1/status").unwrap();
        assert_eq!(url.as_str(), "http://localhost:8080/api/v1/status");
    }

    #[test]
    fn test_invalid_base_url() {
        let result = ApiClient::new().with_base_url("invalid-url");
        assert!(result.is_err());
    }
}