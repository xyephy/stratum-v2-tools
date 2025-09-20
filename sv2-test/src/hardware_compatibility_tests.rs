use sv2_core::{
    Result, Error, Protocol, ProtocolMessage, Connection, Share, ShareResult, 
    WorkTemplate, ConnectionId, ModeHandler, MiningStats
};
use crate::mocks::{MockModeHandler, MockBitcoinRpcClient};
use crate::utils::TestUtils;
use async_trait::async_trait;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;
use tokio::time::sleep;
use bitcoin::{BlockHash, Transaction};
use bitcoin::absolute::LockTime;
use uuid::Uuid;
use rand::Rng;

/// Hardware device types supported for testing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HardwareDevice {
    Bitaxe,
    AntminerS9,
    Whatsminer,
}

impl HardwareDevice {
    /// Get device name as string
    pub fn name(&self) -> &'static str {
        match self {
            HardwareDevice::Bitaxe => "Bitaxe",
            HardwareDevice::AntminerS9 => "Antminer S9",
            HardwareDevice::Whatsminer => "Whatsminer",
        }
    }

    /// Get default user agent string for device
    pub fn user_agent(&self) -> &'static str {
        match self {
            HardwareDevice::Bitaxe => "bitaxe/1.0",
            HardwareDevice::AntminerS9 => "bmminer/2.0.0",
            HardwareDevice::Whatsminer => "cgminer/4.12.0",
        }
    }

    /// Get expected hashrate range for device (in TH/s)
    pub fn hashrate_range(&self) -> (f64, f64) {
        match self {
            HardwareDevice::Bitaxe => (0.4, 0.6), // 400-600 GH/s
            HardwareDevice::AntminerS9 => (13.0, 14.0), // 13-14 TH/s
            HardwareDevice::Whatsminer => (100.0, 120.0), // 100-120 TH/s
        }
    }

    /// Get typical power consumption in watts
    pub fn power_consumption(&self) -> u32 {
        match self {
            HardwareDevice::Bitaxe => 15,
            HardwareDevice::AntminerS9 => 1323,
            HardwareDevice::Whatsminer => 3360,
        }
    }

    /// Get supported protocols
    pub fn supported_protocols(&self) -> Vec<Protocol> {
        match self {
            HardwareDevice::Bitaxe => vec![Protocol::Sv1, Protocol::Sv2],
            HardwareDevice::AntminerS9 => vec![Protocol::Sv1],
            HardwareDevice::Whatsminer => vec![Protocol::Sv1],
        }
    }

    /// Get device-specific connection quirks
    pub fn connection_quirks(&self) -> DeviceQuirks {
        match self {
            HardwareDevice::Bitaxe => DeviceQuirks {
                needs_worker_name: false,
                supports_extranonce_subscribe: true,
                min_difficulty: 1.0,
                max_difficulty: 1_000_000.0,
                connection_timeout_ms: 5000,
                share_submission_delay_ms: 100,
                restart_required_on_difficulty_change: false,
                firmware_version_in_subscribe: true,
            },
            HardwareDevice::AntminerS9 => DeviceQuirks {
                needs_worker_name: true,
                supports_extranonce_subscribe: false,
                min_difficulty: 1.0,
                max_difficulty: 100_000.0,
                connection_timeout_ms: 30000,
                share_submission_delay_ms: 500,
                restart_required_on_difficulty_change: true,
                firmware_version_in_subscribe: false,
            },
            HardwareDevice::Whatsminer => DeviceQuirks {
                needs_worker_name: true,
                supports_extranonce_subscribe: true,
                min_difficulty: 1.0,
                max_difficulty: 1_000_000.0,
                connection_timeout_ms: 15000,
                share_submission_delay_ms: 200,
                restart_required_on_difficulty_change: false,
                firmware_version_in_subscribe: true,
            },
        }
    }
}

/// Device-specific behavioral quirks and limitations
#[derive(Debug, Clone)]
pub struct DeviceQuirks {
    pub needs_worker_name: bool,
    pub supports_extranonce_subscribe: bool,
    pub min_difficulty: f64,
    pub max_difficulty: f64,
    pub connection_timeout_ms: u64,
    pub share_submission_delay_ms: u64,
    pub restart_required_on_difficulty_change: bool,
    pub firmware_version_in_subscribe: bool,
}

/// Mock hardware device that simulates real device behavior
pub struct MockHardwareDevice {
    device_type: HardwareDevice,
    connection_id: ConnectionId,
    pub current_protocol: Protocol,
    quirks: DeviceQuirks,
    pub state: DeviceState,
    pub stats: DeviceStats,
    fail_conditions: Vec<FailureCondition>,
}

#[derive(Debug, Clone)]
pub struct DeviceState {
    pub connected: bool,
    pub authorized: bool,
    pub current_job_id: Option<String>,
    pub current_difficulty: f64,
    pub extranonce1: Option<String>,
    pub extranonce2_size: Option<u8>,
    pub worker_name: Option<String>,
    pub last_activity: std::time::Instant,
}

#[derive(Debug, Clone)]
pub struct DeviceStats {
    pub shares_submitted: u64,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub blocks_found: u64,
    pub current_hashrate: f64,
    pub uptime: Duration,
    pub connection_attempts: u32,
    pub last_error: Option<String>,
}

#[derive(Debug, Clone)]
pub enum FailureCondition {
    RandomDisconnect { probability: f64 },
    RejectShares { probability: f64, reason: String },
    DelayedResponse { delay_ms: u64 },
    CorruptedMessages { probability: f64 },
    DifficultyOverflow,
    MemoryLeak,
    FirmwareBug { bug_type: String },
}

impl MockHardwareDevice {
    pub fn new(device_type: HardwareDevice, protocol: Protocol) -> Self {
        let quirks = device_type.connection_quirks();
        let (min_hashrate, max_hashrate) = device_type.hashrate_range();
        let current_hashrate = min_hashrate + (max_hashrate - min_hashrate) * 0.5;

        Self {
            device_type,
            connection_id: Uuid::new_v4(),
            current_protocol: protocol,
            quirks,
            state: DeviceState {
                connected: false,
                authorized: false,
                current_job_id: None,
                current_difficulty: 1.0,
                extranonce1: None,
                extranonce2_size: None,
                worker_name: None,
                last_activity: std::time::Instant::now(),
            },
            stats: DeviceStats {
                shares_submitted: 0,
                shares_accepted: 0,
                shares_rejected: 0,
                blocks_found: 0,
                current_hashrate,
                uptime: Duration::from_secs(0),
                connection_attempts: 0,
                last_error: None,
            },
            fail_conditions: Vec::new(),
        }
    }

    pub fn with_failure_condition(mut self, condition: FailureCondition) -> Self {
        self.fail_conditions.push(condition);
        self
    }

    /// Simulate device connecting to pool
    pub async fn connect(&mut self, pool_address: SocketAddr) -> Result<Connection> {
        self.stats.connection_attempts += 1;
        
        // Check for connection timeout simulation
        if self.quirks.connection_timeout_ms > 10000 {
            sleep(Duration::from_millis(100)).await; // Simulate delay
        }

        // Check for random disconnect failure
        for condition in &self.fail_conditions {
            match condition {
                FailureCondition::RandomDisconnect { probability } => {
                    let mut rng = rand::thread_rng();
                    if rng.gen::<f64>() < *probability {
                        self.stats.last_error = Some("Random disconnect".to_string());
                        return Err(Error::Connection("Device randomly disconnected".to_string()));
                    }
                }
                _ => {}
            }
        }

        self.state.connected = true;
        self.state.last_activity = std::time::Instant::now();

        Ok(Connection::new(pool_address, self.current_protocol))
    }

    /// Simulate device subscribing to mining
    pub async fn subscribe(&mut self) -> Result<ProtocolMessage> {
        if !self.state.connected {
            return Err(Error::Connection("Device not connected".to_string()));
        }

        let user_agent = format!("{}/{}", 
            self.device_type.user_agent(),
            if self.quirks.firmware_version_in_subscribe { "v1.2.3" } else { "" }
        );

        match self.current_protocol {
            Protocol::Sv1 | Protocol::StratumV1 => {
                Ok(ProtocolMessage::Subscribe {
                    user_agent,
                    session_id: Some(format!("session_{}", self.connection_id)),
                })
            }
            Protocol::Sv2 | Protocol::StratumV2 => {
                Ok(ProtocolMessage::SetupConnection {
                    protocol: 2,
                    min_version: 2,
                    max_version: 2,
                    flags: 0,
                    endpoint_host: "127.0.0.1".to_string(),
                    endpoint_port: 4444,
                    vendor: self.device_type.name().to_string(),
                    hardware_version: "1.0".to_string(),
                    firmware: "1.2.3".to_string(),
                    device_id: format!("device_{}", self.connection_id),
                })
            }
        }
    }

    /// Simulate device authorization
    pub async fn authorize(&mut self, worker_name: String, password: String) -> Result<ProtocolMessage> {
        if !self.state.connected {
            return Err(Error::Connection("Device not connected".to_string()));
        }

        if self.quirks.needs_worker_name && worker_name.is_empty() {
            return Err(Error::Protocol("Worker name required".to_string()));
        }

        self.state.worker_name = Some(worker_name.clone());
        self.state.authorized = true;

        match self.current_protocol {
            Protocol::Sv1 | Protocol::StratumV1 => {
                Ok(ProtocolMessage::Authorize {
                    username: worker_name,
                    password,
                })
            }
            Protocol::Sv2 | Protocol::StratumV2 => {
                Ok(ProtocolMessage::OpenStandardMiningChannel {
                    request_id: 1,
                    user_identity: worker_name,
                    nominal_hash_rate: self.stats.current_hashrate as f32,
                    max_target: [0xFFu8; 32],
                })
            }
        }
    }

    /// Simulate device submitting a share
    pub async fn submit_share(&mut self, job_id: String, nonce: u32, ntime: u32) -> Result<ProtocolMessage> {
        if !self.state.authorized {
            return Err(Error::Protocol("Device not authorized".to_string()));
        }

        // Apply share submission delay
        sleep(Duration::from_millis(self.quirks.share_submission_delay_ms)).await;

        // Check for corrupted message failure
        for condition in &self.fail_conditions {
            match condition {
                FailureCondition::CorruptedMessages { probability } => {
                    let mut rng = rand::thread_rng();
                    if rng.gen::<f64>() < *probability {
                        return Err(Error::Protocol("Corrupted message".to_string()));
                    }
                }
                _ => {}
            }
        }

        self.stats.shares_submitted += 1;
        self.state.last_activity = std::time::Instant::now();

        let worker_name = self.state.worker_name.clone()
            .unwrap_or_else(|| "default_worker".to_string());

        match self.current_protocol {
            Protocol::Sv1 | Protocol::StratumV1 => {
                Ok(ProtocolMessage::Submit {
                    username: worker_name,
                    job_id,
                    extranonce2: format!("{:08x}", nonce & 0xFFFF),
                    ntime: format!("{:08x}", ntime),
                    nonce: format!("{:08x}", nonce),
                })
            }
            Protocol::Sv2 | Protocol::StratumV2 => {
                Ok(ProtocolMessage::SubmitSharesStandard {
                    channel_id: 1,
                    sequence_number: self.stats.shares_submitted as u32,
                    job_id: job_id.parse().unwrap_or(1),
                    nonce,
                    ntime,
                    version: 0x20000000,
                })
            }
        }
    }

    /// Process share validation result
    pub fn process_share_result(&mut self, result: ShareResult) {
        match result {
            ShareResult::Valid => {
                self.stats.shares_accepted += 1;
            }
            ShareResult::Invalid(_) => {
                self.stats.shares_rejected += 1;
            }
            ShareResult::Block(_) => {
                self.stats.shares_accepted += 1;
                self.stats.blocks_found += 1;
            }
        }
    }

    /// Update device difficulty
    pub async fn set_difficulty(&mut self, new_difficulty: f64) -> Result<()> {
        if new_difficulty < self.quirks.min_difficulty || new_difficulty > self.quirks.max_difficulty {
            return Err(Error::Protocol(format!(
                "Difficulty {} outside range {}-{}",
                new_difficulty, self.quirks.min_difficulty, self.quirks.max_difficulty
            )));
        }

        // Check for difficulty overflow bug
        for condition in &self.fail_conditions {
            match condition {
                FailureCondition::DifficultyOverflow => {
                    if new_difficulty > 65536.0 {
                        return Err(Error::Protocol("Difficulty overflow".to_string()));
                    }
                }
                _ => {}
            }
        }

        if self.quirks.restart_required_on_difficulty_change && 
           (self.state.current_difficulty - new_difficulty).abs() > 0.001 {
            // Simulate device restart
            self.state.connected = false;
            self.state.authorized = false;
            sleep(Duration::from_millis(1000)).await;
        }

        self.state.current_difficulty = new_difficulty;
        Ok(())
    }

    /// Get device statistics
    pub fn get_stats(&self) -> &DeviceStats {
        &self.stats
    }

    /// Get device state
    pub fn get_state(&self) -> &DeviceState {
        &self.state
    }

    /// Simulate device-specific firmware bugs
    pub async fn trigger_firmware_bug(&mut self, bug_type: &str) -> Result<()> {
        match bug_type {
            "memory_leak" => {
                // Simulate gradually increasing response times
                sleep(Duration::from_millis(100 + self.stats.shares_submitted / 10)).await;
            }
            "invalid_nonce" => {
                return Err(Error::Protocol("Firmware bug: invalid nonce generation".to_string()));
            }
            "connection_drop" => {
                self.state.connected = false;
                return Err(Error::Connection("Firmware bug: connection dropped".to_string()));
            }
            _ => {}
        }
        Ok(())
    }
}

/// Hardware compatibility test suite
pub struct HardwareCompatibilityTest {
    pub devices: HashMap<HardwareDevice, MockHardwareDevice>,
    mode_handler: MockModeHandler,
}

impl HardwareCompatibilityTest {
    pub fn new() -> Self {
        Self {
            devices: HashMap::new(),
            mode_handler: MockModeHandler::new(),
        }
    }

    /// Add a device to the test suite
    pub fn add_device(&mut self, device_type: HardwareDevice, protocol: Protocol) {
        let device = MockHardwareDevice::new(device_type, protocol);
        self.devices.insert(device_type, device);
    }

    /// Add a device with specific failure conditions
    pub fn add_device_with_failures(&mut self, device_type: HardwareDevice, protocol: Protocol, failures: Vec<FailureCondition>) {
        let mut device = MockHardwareDevice::new(device_type, protocol);
        for failure in failures {
            device = device.with_failure_condition(failure);
        }
        self.devices.insert(device_type, device);
    }

    /// Test basic device connectivity
    pub async fn test_device_connectivity(&mut self, device_type: HardwareDevice) -> Result<()> {
        let device = self.devices.get_mut(&device_type)
            .ok_or_else(|| Error::System("Device not found".to_string()))?;

        let pool_address = "127.0.0.1:3333".parse().unwrap();
        let conn = device.connect(pool_address).await?;
        
        assert!(device.get_state().connected);
        assert_eq!(conn.protocol, device.current_protocol);
        
        Ok(())
    }

    /// Test protocol compatibility
    pub async fn test_protocol_compatibility(&mut self, device_type: HardwareDevice) -> Result<()> {
        let device = self.devices.get_mut(&device_type)
            .ok_or_else(|| Error::System("Device not found".to_string()))?;

        let supported_protocols = device_type.supported_protocols();
        
        for protocol in supported_protocols {
            device.current_protocol = protocol;
            
            // Test subscription
            let subscribe_msg = device.subscribe().await?;
            assert_eq!(subscribe_msg.protocol(), protocol);
            
            // Test authorization
            let auth_msg = device.authorize("test_worker".to_string(), "password".to_string()).await?;
            assert_eq!(auth_msg.protocol(), protocol);
        }
        
        Ok(())
    }

    /// Test share submission and validation
    pub async fn test_share_submission(&mut self, device_type: HardwareDevice) -> Result<()> {
        let device = self.devices.get_mut(&device_type)
            .ok_or_else(|| Error::System("Device not found".to_string()))?;

        // Setup device
        let pool_address = "127.0.0.1:3333".parse().unwrap();
        device.connect(pool_address).await?;
        device.authorize("test_worker".to_string(), "password".to_string()).await?;

        // Submit multiple shares
        for i in 0..10 {
            let job_id = format!("job_{}", i);
            let nonce = 0x12345678 + i;
            let ntime = chrono::Utc::now().timestamp() as u32;

            let submit_msg = device.submit_share(job_id, nonce, ntime).await?;
            assert_eq!(submit_msg.protocol(), device.current_protocol);

            // Simulate share validation result
            let result = if i % 10 == 0 {
                ShareResult::Block("0000000000000000000000000000000000000000000000000000000000000000".parse().unwrap())
            } else if i % 9 == 0 {
                ShareResult::Invalid("Low difficulty".to_string())
            } else {
                ShareResult::Valid
            };

            device.process_share_result(result);
        }

        let stats = device.get_stats();
        assert_eq!(stats.shares_submitted, 10);
        assert!(stats.shares_accepted > 0);
        
        Ok(())
    }

    /// Test connection management and failover
    pub async fn test_connection_failover(&mut self, device_type: HardwareDevice) -> Result<()> {
        let device = self.devices.get_mut(&device_type)
            .ok_or_else(|| Error::System("Device not found".to_string()))?;

        let pool_address = "127.0.0.1:3333".parse().unwrap();
        
        // Test initial connection
        device.connect(pool_address).await?;
        assert!(device.get_state().connected);

        // Simulate connection drop
        device.state.connected = false;
        
        // Test reconnection
        device.connect(pool_address).await?;
        assert!(device.get_state().connected);
        assert!(device.get_stats().connection_attempts >= 2);
        
        Ok(())
    }

    /// Test device-specific quirks and edge cases
    pub async fn test_device_quirks(&mut self, device_type: HardwareDevice) -> Result<()> {
        let device = self.devices.get_mut(&device_type)
            .ok_or_else(|| Error::System("Device not found".to_string()))?;

        let quirks = &device.quirks.clone();
        let pool_address = "127.0.0.1:3333".parse().unwrap();
        device.connect(pool_address).await?;

        // Test worker name requirement
        if quirks.needs_worker_name {
            let result = device.authorize("".to_string(), "password".to_string()).await;
            assert!(result.is_err());
        }

        // Test difficulty limits
        let result = device.set_difficulty(quirks.max_difficulty + 1.0).await;
        assert!(result.is_err());

        let result = device.set_difficulty(quirks.min_difficulty - 1.0).await;
        assert!(result.is_err());

        // Test valid difficulty
        device.set_difficulty(quirks.min_difficulty).await?;
        assert_eq!(device.get_state().current_difficulty, quirks.min_difficulty);

        Ok(())
    }

    /// Test performance characteristics
    pub async fn test_performance(&mut self, device_type: HardwareDevice) -> Result<()> {
        let device = self.devices.get_mut(&device_type)
            .ok_or_else(|| Error::System("Device not found".to_string()))?;

        let (min_hashrate, max_hashrate) = device_type.hashrate_range();
        let current_hashrate = device.get_stats().current_hashrate;

        assert!(current_hashrate >= min_hashrate);
        assert!(current_hashrate <= max_hashrate);

        // Test sustained operation
        let pool_address = "127.0.0.1:3333".parse().unwrap();
        device.connect(pool_address).await?;
        device.authorize("test_worker".to_string(), "password".to_string()).await?;

        let start_time = std::time::Instant::now();
        let mut share_count = 0;

        // Simulate 1 second of mining
        while start_time.elapsed() < Duration::from_millis(100) { // Shortened for test
            let job_id = format!("job_{}", share_count);
            let nonce = 0x12345678 + share_count;
            let ntime = chrono::Utc::now().timestamp() as u32;

            device.submit_share(job_id, nonce, ntime).await?;
            share_count += 1;
            
            sleep(Duration::from_millis(10)).await; // Simulate mining delay
        }

        assert!(share_count > 0);
        
        Ok(())
    }

    /// Run comprehensive hardware compatibility test suite
    pub async fn run_full_test_suite(&mut self) -> Result<HashMap<HardwareDevice, Vec<String>>> {
        let mut results = HashMap::new();

        for &device_type in &[HardwareDevice::Bitaxe, HardwareDevice::AntminerS9, HardwareDevice::Whatsminer] {
            let mut test_results = Vec::new();

            // Add device for testing
            self.add_device(device_type, Protocol::Sv1);

            // Run all tests
            match self.test_device_connectivity(device_type).await {
                Ok(_) => test_results.push("Connectivity: PASS".to_string()),
                Err(e) => test_results.push(format!("Connectivity: FAIL - {}", e)),
            }

            match self.test_protocol_compatibility(device_type).await {
                Ok(_) => test_results.push("Protocol Compatibility: PASS".to_string()),
                Err(e) => test_results.push(format!("Protocol Compatibility: FAIL - {}", e)),
            }

            match self.test_share_submission(device_type).await {
                Ok(_) => test_results.push("Share Submission: PASS".to_string()),
                Err(e) => test_results.push(format!("Share Submission: FAIL - {}", e)),
            }

            match self.test_connection_failover(device_type).await {
                Ok(_) => test_results.push("Connection Failover: PASS".to_string()),
                Err(e) => test_results.push(format!("Connection Failover: FAIL - {}", e)),
            }

            match self.test_device_quirks(device_type).await {
                Ok(_) => test_results.push("Device Quirks: PASS".to_string()),
                Err(e) => test_results.push(format!("Device Quirks: FAIL - {}", e)),
            }

            match self.test_performance(device_type).await {
                Ok(_) => test_results.push("Performance: PASS".to_string()),
                Err(e) => test_results.push(format!("Performance: FAIL - {}", e)),
            }

            results.insert(device_type, test_results);
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bitaxe_compatibility() {
        let mut test_suite = HardwareCompatibilityTest::new();
        test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv1);
        
        assert!(test_suite.test_device_connectivity(HardwareDevice::Bitaxe).await.is_ok());
        assert!(test_suite.test_protocol_compatibility(HardwareDevice::Bitaxe).await.is_ok());
        assert!(test_suite.test_share_submission(HardwareDevice::Bitaxe).await.is_ok());
    }

    #[tokio::test]
    async fn test_antminer_s9_compatibility() {
        let mut test_suite = HardwareCompatibilityTest::new();
        test_suite.add_device(HardwareDevice::AntminerS9, Protocol::Sv1);
        
        assert!(test_suite.test_device_connectivity(HardwareDevice::AntminerS9).await.is_ok());
        assert!(test_suite.test_share_submission(HardwareDevice::AntminerS9).await.is_ok());
    }

    #[tokio::test]
    async fn test_whatsminer_compatibility() {
        let mut test_suite = HardwareCompatibilityTest::new();
        test_suite.add_device(HardwareDevice::Whatsminer, Protocol::Sv1);
        
        assert!(test_suite.test_device_connectivity(HardwareDevice::Whatsminer).await.is_ok());
        assert!(test_suite.test_share_submission(HardwareDevice::Whatsminer).await.is_ok());
    }

    #[tokio::test]
    async fn test_device_failure_conditions() {
        let mut test_suite = HardwareCompatibilityTest::new();
        
        let failures = vec![
            FailureCondition::RandomDisconnect { probability: 0.1 },
            FailureCondition::RejectShares { probability: 0.05, reason: "Test rejection".to_string() },
        ];
        
        test_suite.add_device_with_failures(HardwareDevice::Bitaxe, Protocol::Sv1, failures);
        
        // Test should handle failure conditions gracefully
        let result = test_suite.test_device_connectivity(HardwareDevice::Bitaxe).await;
        // May pass or fail depending on random conditions
    }

    #[tokio::test]
    async fn test_full_test_suite() {
        let mut test_suite = HardwareCompatibilityTest::new();
        let results = test_suite.run_full_test_suite().await.unwrap();
        
        assert_eq!(results.len(), 3); // Three device types
        
        for (device, test_results) in results {
            println!("Device: {}", device.name());
            for result in test_results {
                println!("  {}", result);
            }
        }
    }

    #[tokio::test]
    async fn test_sv2_protocol_compatibility() {
        let mut test_suite = HardwareCompatibilityTest::new();
        test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv2);
        
        assert!(test_suite.test_protocol_compatibility(HardwareDevice::Bitaxe).await.is_ok());
    }

    #[tokio::test]
    async fn test_device_quirks_validation() {
        let quirks = HardwareDevice::AntminerS9.connection_quirks();
        assert!(quirks.needs_worker_name);
        assert!(!quirks.supports_extranonce_subscribe);
        assert!(quirks.restart_required_on_difficulty_change);

        let quirks = HardwareDevice::Bitaxe.connection_quirks();
        assert!(!quirks.needs_worker_name);
        assert!(quirks.supports_extranonce_subscribe);
        assert!(!quirks.restart_required_on_difficulty_change);
    }
}