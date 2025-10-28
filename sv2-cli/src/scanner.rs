use anyhow::{Context, Result};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fmt;
use std::net::{IpAddr, Ipv4Addr};
use std::time::{Duration, Instant};
use tokio::net::TcpStream;
use tokio::time::timeout;
use tracing::{info, warn, debug};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetectedMiner {
    pub ip: IpAddr,
    pub miner_type: MinerType,
    pub api_port: Option<u16>,
    pub response_time_ms: u64,
    #[serde(skip, default = "std::time::Instant::now")]
    pub last_seen: Instant,
    pub details: MinerDetails,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MinerType {
    Bitaxe,
    Apollo,
    AntminerS19,
    AntminerS21,
    Whatsminer,
    Unknown,
}

impl fmt::Display for MinerType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MinerType::Bitaxe => write!(f, "Bitaxe"),
            MinerType::Apollo => write!(f, "Apollo BTC"),
            MinerType::AntminerS19 => write!(f, "Antminer S19"),
            MinerType::AntminerS21 => write!(f, "Antminer S21"),
            MinerType::Whatsminer => write!(f, "Whatsminer"),
            MinerType::Unknown => write!(f, "Unknown"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinerDetails {
    pub hostname: Option<String>,
    pub mac_address: Option<String>,
    pub firmware_version: Option<String>,
    pub hashrate: Option<f64>,
    pub temperature: Option<f64>,
    pub power_consumption: Option<f64>,
    pub pool_url: Option<String>,
    pub worker_name: Option<String>,
}

impl Default for MinerDetails {
    fn default() -> Self {
        Self {
            hostname: None,
            mac_address: None,
            firmware_version: None,
            hashrate: None,
            temperature: None,
            power_consumption: None,
            pool_url: None,
            worker_name: None,
        }
    }
}

pub struct NetworkScanner {
    client: Client,
    timeout_duration: Duration,
}

impl NetworkScanner {
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(Duration::from_secs(5))
                .build()
                .expect("Failed to create HTTP client"),
            timeout_duration: Duration::from_secs(3),
        }
    }

    /// Scan the local network for miners
    pub async fn scan_network(&self, subnets: Option<Vec<String>>) -> Result<Vec<DetectedMiner>> {
        let subnets = subnets.unwrap_or_else(|| vec![
            "192.168.1.0/24".to_string(),
            "192.168.0.0/24".to_string(), 
            "10.0.0.0/24".to_string(),
        ]);

        info!("🔍 Scanning network for miners...");
        info!("   Subnets: {:?}", subnets);

        let mut detected_miners = Vec::new();

        for subnet in subnets {
            let subnet_miners = self.scan_subnet(&subnet).await?;
            detected_miners.extend(subnet_miners);
        }

        info!("✅ Network scan complete. Found {} miners", detected_miners.len());
        Ok(detected_miners)
    }

    /// Scan a specific subnet for miners
    async fn scan_subnet(&self, subnet: &str) -> Result<Vec<DetectedMiner>> {
        let (base_ip, _mask) = subnet.split_once('/').unwrap_or((subnet, "24"));
        let base: Vec<u8> = base_ip.split('.').map(|s| s.parse().unwrap_or(0)).collect();
        
        if base.len() != 4 {
            return Err(anyhow::anyhow!("Invalid subnet format: {}", subnet));
        }

        let mut miners = Vec::new();
        let mut scan_tasks = Vec::new();

        // Scan 192.168.x.1-254 range (skip .0 and .255)
        for i in 1..255 {
            let ip = Ipv4Addr::new(base[0], base[1], base[2], i);
            let scanner = self.clone();
            
            let task = tokio::spawn(async move {
                scanner.probe_host(IpAddr::V4(ip)).await
            });
            scan_tasks.push(task);
        }

        // Wait for all scans to complete
        for task in scan_tasks {
            if let Ok(Some(miner)) = task.await? {
                miners.push(miner);
            }
        }

        debug!("Subnet {} scan complete: {} miners found", subnet, miners.len());
        Ok(miners)
    }

    /// Probe a specific host for miner services
    async fn probe_host(&self, ip: IpAddr) -> Result<Option<DetectedMiner>> {
        // Common miner API ports to check
        let ports = [
            4028, // CGMiner API (many ASICs)
            80,   // Web interface
            8080, // Alternative web interface
            4029, // Backup API port
            3333, // Stratum port (sometimes has web interface)
        ];

        for &port in &ports {
            if let Ok(Some(miner)) = self.check_miner_api(ip, port).await {
                return Ok(Some(miner));
            }
        }

        Ok(None)
    }

    /// Check if a specific IP:port responds to miner API calls
    async fn check_miner_api(&self, ip: IpAddr, port: u16) -> Result<Option<DetectedMiner>> {
        let start_time = Instant::now();

        // First check if port is open
        if !self.is_port_open(ip, port).await {
            return Ok(None);
        }

        let response_time = start_time.elapsed().as_millis() as u64;
        debug!("Port {} open on {}", port, ip);

        // Try different miner API endpoints
        let miner_type = if let Some(miner_type) = self.detect_bitaxe(ip, port).await? {
            miner_type
        } else if let Some(miner_type) = self.detect_apollo(ip, port).await? {
            miner_type
        } else if let Some(miner_type) = self.detect_antminer(ip, port).await? {
            miner_type
        } else if let Some(miner_type) = self.detect_whatsminer(ip, port).await? {
            miner_type
        } else {
            return Ok(None);
        };

        // Get detailed information
        let details = self.get_miner_details(ip, port, &miner_type).await?;

        Ok(Some(DetectedMiner {
            ip,
            miner_type,
            api_port: Some(port),
            response_time_ms: response_time,
            last_seen: Instant::now(),
            details,
        }))
    }

    /// Check if a TCP port is open
    async fn is_port_open(&self, ip: IpAddr, port: u16) -> bool {
        timeout(
            self.timeout_duration,
            TcpStream::connect((ip, port))
        )
        .await
        .is_ok()
    }

    /// Detect Bitaxe miner
    async fn detect_bitaxe(&self, ip: IpAddr, port: u16) -> Result<Option<MinerType>> {
        let url = format!("http://{}:{}/api/system/info", ip, port);
        
        if let Ok(response) = self.client.get(&url).send().await {
            if let Ok(text) = response.text().await {
                if text.contains("bitaxe") || text.contains("\"ASICModel\"") {
                    debug!("Detected Bitaxe at {}:{}", ip, port);
                    return Ok(Some(MinerType::Bitaxe));
                }
            }
        }

        Ok(None)
    }

    /// Detect Apollo BTC miner
    async fn detect_apollo(&self, ip: IpAddr, port: u16) -> Result<Option<MinerType>> {
        // Apollo typically uses CGMiner API on port 4028
        if port == 4028 {
            // Try CGMiner version command
            // Note: This would require implementing CGMiner JSON API over TCP
            // For now, we'll check HTTP endpoints
        }

        let url = format!("http://{}:{}/", ip, port);
        if let Ok(response) = self.client.get(&url).send().await {
            if let Ok(text) = response.text().await {
                if text.contains("Apollo") || text.contains("FutureBit") {
                    debug!("Detected Apollo at {}:{}", ip, port);
                    return Ok(Some(MinerType::Apollo));
                }
            }
        }

        Ok(None)
    }

    /// Detect Antminer
    async fn detect_antminer(&self, ip: IpAddr, port: u16) -> Result<Option<MinerType>> {
        let url = format!("http://{}:{}/", ip, port);
        
        if let Ok(response) = self.client.get(&url).send().await {
            if let Ok(text) = response.text().await {
                if text.contains("Antminer") {
                    if text.contains("S19") {
                        debug!("Detected Antminer S19 at {}:{}", ip, port);
                        return Ok(Some(MinerType::AntminerS19));
                    } else if text.contains("S21") {
                        debug!("Detected Antminer S21 at {}:{}", ip, port);
                        return Ok(Some(MinerType::AntminerS21));
                    } else {
                        debug!("Detected unknown Antminer at {}:{}", ip, port);
                        return Ok(Some(MinerType::Unknown));
                    }
                }
            }
        }

        Ok(None)
    }

    /// Detect Whatsminer
    async fn detect_whatsminer(&self, ip: IpAddr, port: u16) -> Result<Option<MinerType>> {
        let url = format!("http://{}:{}/", ip, port);
        
        if let Ok(response) = self.client.get(&url).send().await {
            if let Ok(text) = response.text().await {
                if text.contains("Whatsminer") || text.contains("MicroBT") {
                    debug!("Detected Whatsminer at {}:{}", ip, port);
                    return Ok(Some(MinerType::Whatsminer));
                }
            }
        }

        Ok(None)
    }

    /// Get detailed information about a detected miner
    async fn get_miner_details(&self, ip: IpAddr, port: u16, miner_type: &MinerType) -> Result<MinerDetails> {
        let mut details = MinerDetails::default();

        match miner_type {
            MinerType::Bitaxe => {
                details = self.get_bitaxe_details(ip, port).await.unwrap_or_default();
            }
            MinerType::Apollo => {
                details = self.get_apollo_details(ip, port).await.unwrap_or_default();
            }
            _ => {
                // Try generic approaches for other miners
                details = self.get_generic_details(ip, port).await.unwrap_or_default();
            }
        }

        Ok(details)
    }

    /// Get Bitaxe-specific details
    async fn get_bitaxe_details(&self, ip: IpAddr, port: u16) -> Result<MinerDetails> {
        let mut details = MinerDetails::default();

        // Get system info
        let system_url = format!("http://{}:{}/api/system/info", ip, port);
        if let Ok(response) = self.client.get(&system_url).send().await {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                details.hostname = json.get("hostname").and_then(|v| v.as_str()).map(String::from);
                details.firmware_version = json.get("version").and_then(|v| v.as_str()).map(String::from);
                details.mac_address = json.get("macAddr").and_then(|v| v.as_str()).map(String::from);
            }
        }

        // Get mining stats
        let stats_url = format!("http://{}:{}/api/system/stats", ip, port);
        if let Ok(response) = self.client.get(&stats_url).send().await {
            if let Ok(json) = response.json::<serde_json::Value>().await {
                details.hashrate = json.get("hashRate").and_then(|v| v.as_f64());
                details.temperature = json.get("temp").and_then(|v| v.as_f64());
                details.power_consumption = json.get("power").and_then(|v| v.as_f64());
                details.pool_url = json.get("stratumURL").and_then(|v| v.as_str()).map(String::from);
                details.worker_name = json.get("stratumUser").and_then(|v| v.as_str()).map(String::from);
            }
        }

        Ok(details)
    }

    /// Get Apollo-specific details  
    async fn get_apollo_details(&self, ip: IpAddr, port: u16) -> Result<MinerDetails> {
        let mut details = MinerDetails::default();

        // Apollo uses CGMiner API - would need TCP JSON-RPC implementation
        // For now, try HTTP endpoints
        let url = format!("http://{}:{}/", ip, port);
        if let Ok(response) = self.client.get(&url).send().await {
            if let Ok(text) = response.text().await {
                // Parse HTML or JSON for Apollo details
                // This is a simplified implementation
                if text.contains("Apollo") {
                    details.hostname = Some("Apollo BTC".to_string());
                }
            }
        }

        Ok(details)
    }

    /// Get generic miner details
    async fn get_generic_details(&self, ip: IpAddr, port: u16) -> Result<MinerDetails> {
        let mut details = MinerDetails::default();

        let url = format!("http://{}:{}/", ip, port);
        if let Ok(response) = self.client.get(&url).send().await {
            if let Ok(text) = response.text().await {
                // Try to extract basic info from HTML
                details.hostname = Some(format!("Miner-{}", ip));
            }
        }

        Ok(details)
    }
}

impl Clone for NetworkScanner {
    fn clone(&self) -> Self {
        Self {
            client: self.client.clone(),
            timeout_duration: self.timeout_duration,
        }
    }
}

/// Generate configuration recommendations based on detected miners
pub fn generate_config_recommendations(miners: &[DetectedMiner]) -> HashMap<String, serde_json::Value> {
    let mut recommendations = HashMap::new();

    if miners.is_empty() {
        recommendations.insert(
            "message".to_string(), 
            serde_json::json!("No miners detected. Manual configuration required.")
        );
        return recommendations;
    }

    // Count miner types
    let mut miner_counts = HashMap::new();
    for miner in miners {
        *miner_counts.entry(miner.miner_type.to_string()).or_insert(0) += 1;
    }

    recommendations.insert("detected_miners".to_string(), serde_json::json!(miner_counts));

    // Determine optimal extranonce2_size
    let has_bitaxe = miners.iter().any(|m| matches!(m.miner_type, MinerType::Bitaxe));
    let has_apollo = miners.iter().any(|m| matches!(m.miner_type, MinerType::Apollo));

    let extranonce2_size = if has_bitaxe && !has_apollo {
        4 // Bitaxe optimal
    } else if has_apollo && !has_bitaxe {
        6 // Apollo optimal  
    } else if has_bitaxe && has_apollo {
        6 // Compatible with both
    } else {
        8 // Conservative default
    };

    recommendations.insert("extranonce2_size".to_string(), serde_json::json!(extranonce2_size));

    // Network-specific recommendations
    let total_estimated_hashrate: f64 = miners.iter()
        .filter_map(|m| m.details.hashrate)
        .sum();

    if total_estimated_hashrate > 0.0 {
        recommendations.insert("total_hashrate".to_string(), serde_json::json!(total_estimated_hashrate));
        
        let shares_per_minute = if total_estimated_hashrate < 1e12 { // < 1 TH/s
            3.0
        } else if total_estimated_hashrate < 10e12 { // < 10 TH/s  
            5.0
        } else {
            10.0
        };
        
        recommendations.insert("shares_per_minute".to_string(), serde_json::json!(shares_per_minute));
    }

    recommendations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_miner_type_display() {
        assert_eq!(MinerType::Bitaxe.to_string(), "Bitaxe");
        assert_eq!(MinerType::Apollo.to_string(), "Apollo BTC");
    }

    #[test]
    fn test_generate_config_recommendations() {
        let miners = vec![
            DetectedMiner {
                ip: "192.168.1.100".parse().unwrap(),
                miner_type: MinerType::Bitaxe,
                api_port: Some(80),
                response_time_ms: 50,
                last_seen: Instant::now(),
                details: MinerDetails {
                    hashrate: Some(700e9), // 700 GH/s
                    ..Default::default()
                },
            }
        ];

        let recommendations = generate_config_recommendations(&miners);
        assert_eq!(recommendations.get("extranonce2_size").unwrap(), &serde_json::json!(4));
    }
}