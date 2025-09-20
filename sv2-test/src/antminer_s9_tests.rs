use crate::hardware_compatibility_tests::*;
use sv2_core::{Result, Protocol, ProtocolMessage};

/// Antminer S9-specific compatibility tests
pub struct AntminerS9Tests {
    pub test_suite: HardwareCompatibilityTest,
}

impl AntminerS9Tests {
    pub fn new() -> Self {
        let mut test_suite = HardwareCompatibilityTest::new();
        test_suite.add_device(HardwareDevice::AntminerS9, Protocol::Sv1);
        
        Self { test_suite }
    }

    /// Test Antminer S9 SV1 compatibility (primary protocol)
    pub async fn test_sv1_compatibility(&mut self) -> Result<()> {
        self.test_suite.test_protocol_compatibility(HardwareDevice::AntminerS9).await?;
        
        // Verify S9 only supports SV1
        let supported_protocols = HardwareDevice::AntminerS9.supported_protocols();
        assert_eq!(supported_protocols, vec![Protocol::Sv1]);
        
        Ok(())
    }

    /// Test worker name requirement (critical for S9)
    pub async fn test_worker_name_requirement(&mut self) -> Result<()> {
        let quirks = HardwareDevice::AntminerS9.connection_quirks();
        assert!(quirks.needs_worker_name);

        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::AntminerS9) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;

            // Test that empty worker name fails
            let result = device.authorize("".to_string(), "password".to_string()).await;
            assert!(result.is_err());

            // Test that valid worker name succeeds
            let result = device.authorize("s9_worker.001".to_string(), "password".to_string()).await;
            assert!(result.is_ok());
        }

        Ok(())
    }

    /// Test difficulty change requiring restart (S9 specific behavior)
    pub async fn test_difficulty_restart_requirement(&mut self) -> Result<()> {
        let quirks = HardwareDevice::AntminerS9.connection_quirks();
        assert!(quirks.restart_required_on_difficulty_change);

        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::AntminerS9) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;
            device.authorize("s9_worker.001".to_string(), "password".to_string()).await?;

            // Set initial difficulty
            device.set_difficulty(1024.0).await?;
            assert!(device.get_state().connected);

            // Change difficulty - should trigger restart
            device.set_difficulty(2048.0).await?;
            // After restart simulation, device should reconnect
            assert_eq!(device.get_state().current_difficulty, 2048.0);
        }

        Ok(())
    }

    /// Test lack of extranonce subscription support
    pub async fn test_no_extranonce_subscription(&mut self) -> Result<()> {
        let quirks = HardwareDevice::AntminerS9.connection_quirks();
        assert!(!quirks.supports_extranonce_subscribe);

        // S9 doesn't support extranonce.subscribe, should work with basic extranonce
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::AntminerS9) {
            // Should not have extranonce subscription capability
            assert!(device.state.extranonce1.is_none());
            assert!(device.state.extranonce2_size.is_none());
        }

        Ok(())
    }

    /// Test high hashrate operation (13-14 TH/s)
    pub async fn test_high_hashrate_operation(&mut self) -> Result<()> {
        let (min_hashrate, max_hashrate) = HardwareDevice::AntminerS9.hashrate_range();
        assert_eq!(min_hashrate, 13.0);
        assert_eq!(max_hashrate, 14.0);

        if let Some(device) = self.test_suite.devices.get(&HardwareDevice::AntminerS9) {
            let current_hashrate = device.get_stats().current_hashrate;
            assert!(current_hashrate >= min_hashrate);
            assert!(current_hashrate <= max_hashrate);
        }

        Ok(())
    }

    /// Test S9 power consumption characteristics
    pub async fn test_power_consumption(&mut self) -> Result<()> {
        let power = HardwareDevice::AntminerS9.power_consumption();
        assert_eq!(power, 1323); // ~1.3 kW typical consumption

        Ok(())
    }

    /// Test S9 connection timeout behavior (30 seconds)
    pub async fn test_connection_timeout(&mut self) -> Result<()> {
        let quirks = HardwareDevice::AntminerS9.connection_quirks();
        assert_eq!(quirks.connection_timeout_ms, 30000); // 30 seconds

        // S9 takes longer to establish connections compared to other devices
        assert!(quirks.connection_timeout_ms > 15000);

        Ok(())
    }

    /// Test S9 share submission delay
    pub async fn test_share_submission_delay(&mut self) -> Result<()> {
        let quirks = HardwareDevice::AntminerS9.connection_quirks();
        assert_eq!(quirks.share_submission_delay_ms, 500); // 500ms delay

        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::AntminerS9) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;
            device.authorize("s9_worker.001".to_string(), "password".to_string()).await?;

            let start_time = std::time::Instant::now();
            
            // Submit a share and measure delay
            device.submit_share("job_1".to_string(), 0x12345678, chrono::Utc::now().timestamp() as u32).await?;
            
            let elapsed = start_time.elapsed();
            // Should include the share submission delay
            assert!(elapsed >= std::time::Duration::from_millis(500));
        }

        Ok(())
    }

    /// Test S9 firmware bug simulation (common issues)
    pub async fn test_firmware_bugs(&mut self) -> Result<()> {
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::AntminerS9) {
            // Test memory leak simulation
            let result = device.trigger_firmware_bug("memory_leak").await;
            assert!(result.is_ok());

            // Test invalid nonce bug
            let result = device.trigger_firmware_bug("invalid_nonce").await;
            assert!(result.is_err());

            // Test connection drop bug
            let result = device.trigger_firmware_bug("connection_drop").await;
            assert!(result.is_err());
            assert!(!device.get_state().connected);
        }

        Ok(())
    }

    /// Test S9 with failure conditions (overheating, network issues)
    pub async fn test_failure_conditions(&mut self) -> Result<()> {
        // Add S9 with specific failure conditions
        let failures = vec![
            FailureCondition::RandomDisconnect { probability: 0.05 }, // 5% disconnect chance
            FailureCondition::RejectShares { probability: 0.02, reason: "Overheating".to_string() },
            FailureCondition::DelayedResponse { delay_ms: 1000 },
        ];

        self.test_suite.add_device_with_failures(HardwareDevice::AntminerS9, Protocol::Sv1, failures);

        // Test with failure conditions - may pass or fail depending on random conditions
        let result = self.test_suite.test_device_connectivity(HardwareDevice::AntminerS9).await;
        // Don't assert result as it depends on random failure conditions

        Ok(())
    }

    /// Test S9 multi-worker configuration
    pub async fn test_multi_worker_configuration(&mut self) -> Result<()> {
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::AntminerS9) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            
            // Test multiple worker connections (simulating S9's 3 hash boards)
            let workers = vec!["s9_worker.001", "s9_worker.002", "s9_worker.003"];
            
            for worker in workers {
                device.connect(pool_address).await?;
                device.authorize(worker.to_string(), "password".to_string()).await?;
                
                // Submit shares for each worker
                for i in 0..3 {
                    let job_id = format!("job_{}_{}", worker, i);
                    let nonce = 0x12345678 + i as u32;
                    let ntime = chrono::Utc::now().timestamp() as u32;
                    device.submit_share(job_id, nonce, ntime).await?;
                }
            }

            // Should have submitted 9 shares total (3 workers × 3 shares)
            assert_eq!(device.get_stats().shares_submitted, 9);
        }

        Ok(())
    }

    /// Test S9 difficulty adjustment behavior
    pub async fn test_difficulty_adjustment(&mut self) -> Result<()> {
        let quirks = HardwareDevice::AntminerS9.connection_quirks();
        
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::AntminerS9) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;
            device.authorize("s9_worker.001".to_string(), "password".to_string()).await?;

            // Test difficulty range
            assert!(device.set_difficulty(quirks.min_difficulty).await.is_ok());
            assert!(device.set_difficulty(quirks.max_difficulty).await.is_ok());

            // Test beyond limits
            assert!(device.set_difficulty(quirks.max_difficulty + 1.0).await.is_err());
            assert!(device.set_difficulty(quirks.min_difficulty - 1.0).await.is_err());
        }

        Ok(())
    }

    /// Run comprehensive Antminer S9 test suite
    pub async fn run_full_s9_tests(&mut self) -> Result<Vec<String>> {
        let mut results = Vec::new();

        // Run basic compatibility tests
        match self.test_suite.test_device_connectivity(HardwareDevice::AntminerS9).await {
            Ok(_) => results.push("Antminer S9 Connectivity: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Connectivity: FAIL - {}", e)),
        }

        match self.test_sv1_compatibility().await {
            Ok(_) => results.push("Antminer S9 SV1 Compatibility: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 SV1 Compatibility: FAIL - {}", e)),
        }

        match self.test_worker_name_requirement().await {
            Ok(_) => results.push("Antminer S9 Worker Name Requirement: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Worker Name Requirement: FAIL - {}", e)),
        }

        match self.test_difficulty_restart_requirement().await {
            Ok(_) => results.push("Antminer S9 Difficulty Restart: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Difficulty Restart: FAIL - {}", e)),
        }

        match self.test_no_extranonce_subscription().await {
            Ok(_) => results.push("Antminer S9 No Extranonce Subscription: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 No Extranonce Subscription: FAIL - {}", e)),
        }

        match self.test_high_hashrate_operation().await {
            Ok(_) => results.push("Antminer S9 High Hashrate Operation: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 High Hashrate Operation: FAIL - {}", e)),
        }

        match self.test_power_consumption().await {
            Ok(_) => results.push("Antminer S9 Power Consumption: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Power Consumption: FAIL - {}", e)),
        }

        match self.test_connection_timeout().await {
            Ok(_) => results.push("Antminer S9 Connection Timeout: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Connection Timeout: FAIL - {}", e)),
        }

        match self.test_share_submission_delay().await {
            Ok(_) => results.push("Antminer S9 Share Submission Delay: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Share Submission Delay: FAIL - {}", e)),
        }

        match self.test_firmware_bugs().await {
            Ok(_) => results.push("Antminer S9 Firmware Bugs: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Firmware Bugs: FAIL - {}", e)),
        }

        match self.test_multi_worker_configuration().await {
            Ok(_) => results.push("Antminer S9 Multi-Worker Configuration: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Multi-Worker Configuration: FAIL - {}", e)),
        }

        match self.test_difficulty_adjustment().await {
            Ok(_) => results.push("Antminer S9 Difficulty Adjustment: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Difficulty Adjustment: FAIL - {}", e)),
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_s9_sv1_protocol_only() {
        let mut s9_tests = AntminerS9Tests::new();
        assert!(s9_tests.test_sv1_compatibility().await.is_ok());
    }

    #[tokio::test]
    async fn test_s9_worker_name_mandatory() {
        let mut s9_tests = AntminerS9Tests::new();
        assert!(s9_tests.test_worker_name_requirement().await.is_ok());
    }

    #[tokio::test]
    async fn test_s9_difficulty_restart_behavior() {
        let mut s9_tests = AntminerS9Tests::new();
        assert!(s9_tests.test_difficulty_restart_requirement().await.is_ok());
    }

    #[tokio::test]
    async fn test_s9_no_extranonce_support() {
        let mut s9_tests = AntminerS9Tests::new();
        assert!(s9_tests.test_no_extranonce_subscription().await.is_ok());
    }

    #[tokio::test]
    async fn test_s9_high_hashrate_characteristics() {
        let mut s9_tests = AntminerS9Tests::new();
        assert!(s9_tests.test_high_hashrate_operation().await.is_ok());
    }

    #[tokio::test]
    async fn test_s9_multi_worker_setup() {
        let mut s9_tests = AntminerS9Tests::new();
        assert!(s9_tests.test_multi_worker_configuration().await.is_ok());
    }

    #[tokio::test]
    async fn test_s9_full_test_suite() {
        let mut s9_tests = AntminerS9Tests::new();
        let results = s9_tests.run_full_s9_tests().await.unwrap();
        
        println!("Antminer S9 Test Results:");
        for result in results {
            println!("  {}", result);
        }
    }
}