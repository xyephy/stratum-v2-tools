use crate::hardware_compatibility_tests::*;
use sv2_core::{Result, Protocol, ProtocolMessage, ShareResult};

/// Whatsminer-specific compatibility tests
pub struct WhatsminerTests {
    pub test_suite: HardwareCompatibilityTest,
}

impl WhatsminerTests {
    pub fn new() -> Self {
        let mut test_suite = HardwareCompatibilityTest::new();
        test_suite.add_device(HardwareDevice::Whatsminer, Protocol::Sv1);
        
        Self { test_suite }
    }

    /// Test Whatsminer SV1 compatibility
    pub async fn test_sv1_compatibility(&mut self) -> Result<()> {
        self.test_suite.test_protocol_compatibility(HardwareDevice::Whatsminer).await?;
        
        // Verify Whatsminer only supports SV1 (like S9)
        let supported_protocols = HardwareDevice::Whatsminer.supported_protocols();
        assert_eq!(supported_protocols, vec![Protocol::Sv1]);
        
        Ok(())
    }

    /// Test very high hashrate operation (100-120 TH/s)
    pub async fn test_very_high_hashrate_operation(&mut self) -> Result<()> {
        let (min_hashrate, max_hashrate) = HardwareDevice::Whatsminer.hashrate_range();
        assert_eq!(min_hashrate, 100.0);
        assert_eq!(max_hashrate, 120.0);

        if let Some(device) = self.test_suite.devices.get(&HardwareDevice::Whatsminer) {
            let current_hashrate = device.get_stats().current_hashrate;
            assert!(current_hashrate >= min_hashrate);
            assert!(current_hashrate <= max_hashrate);
            
            // Whatsminer should have significantly higher hashrate than S9
            assert!(current_hashrate > 50.0); // Much higher than S9's 13-14 TH/s
        }

        Ok(())
    }

    /// Test extremely high power consumption (3.3+ kW)
    pub async fn test_high_power_consumption(&mut self) -> Result<()> {
        let power = HardwareDevice::Whatsminer.power_consumption();
        assert_eq!(power, 3360); // ~3.3 kW typical consumption

        // Should be much higher than S9 (1323W) and Bitaxe (15W)
        assert!(power > 3000);

        Ok(())
    }

    /// Test worker name requirement (similar to S9)
    pub async fn test_worker_name_requirement(&mut self) -> Result<()> {
        let quirks = HardwareDevice::Whatsminer.connection_quirks();
        assert!(quirks.needs_worker_name);

        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;

            // Test that empty worker name fails
            let result = device.authorize("".to_string(), "password".to_string()).await;
            assert!(result.is_err());

            // Test that valid worker name succeeds
            let result = device.authorize("whatsminer_worker.01".to_string(), "password".to_string()).await;
            assert!(result.is_ok());
        }

        Ok(())
    }

    /// Test extranonce subscription support (unlike S9)
    pub async fn test_extranonce_subscription_support(&mut self) -> Result<()> {
        let quirks = HardwareDevice::Whatsminer.connection_quirks();
        assert!(quirks.supports_extranonce_subscribe);

        // Whatsminer supports extranonce subscription, unlike S9
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            device.state.extranonce1 = Some("87654321".to_string());
            device.state.extranonce2_size = Some(8);

            assert_eq!(device.state.extranonce1, Some("87654321".to_string()));
            assert_eq!(device.state.extranonce2_size, Some(8));
        }

        Ok(())
    }

    /// Test no restart required on difficulty change (unlike S9)
    pub async fn test_no_difficulty_restart(&mut self) -> Result<()> {
        let quirks = HardwareDevice::Whatsminer.connection_quirks();
        assert!(!quirks.restart_required_on_difficulty_change);

        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;
            device.authorize("whatsminer_worker.01".to_string(), "password".to_string()).await?;

            // Set initial difficulty
            device.set_difficulty(1024.0).await?;
            let initial_connection_state = device.get_state().connected;
            assert!(initial_connection_state);

            // Change difficulty - should NOT trigger restart
            device.set_difficulty(2048.0).await?;
            assert!(device.get_state().connected); // Should stay connected
            assert_eq!(device.get_state().current_difficulty, 2048.0);
        }

        Ok(())
    }

    /// Test connection timeout behavior (15 seconds - between Bitaxe and S9)
    pub async fn test_connection_timeout(&mut self) -> Result<()> {
        let quirks = HardwareDevice::Whatsminer.connection_quirks();
        assert_eq!(quirks.connection_timeout_ms, 15000); // 15 seconds

        // Should be between Bitaxe (5s) and S9 (30s)
        assert!(quirks.connection_timeout_ms > 5000);
        assert!(quirks.connection_timeout_ms < 30000);

        Ok(())
    }

    /// Test moderate share submission delay
    pub async fn test_share_submission_delay(&mut self) -> Result<()> {
        let quirks = HardwareDevice::Whatsminer.connection_quirks();
        assert_eq!(quirks.share_submission_delay_ms, 200); // 200ms delay

        // Should be between Bitaxe (100ms) and S9 (500ms)
        assert!(quirks.share_submission_delay_ms > 100);
        assert!(quirks.share_submission_delay_ms < 500);

        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;
            device.authorize("whatsminer_worker.01".to_string(), "password".to_string()).await?;

            let start_time = std::time::Instant::now();
            
            // Submit a share and measure delay
            device.submit_share("job_1".to_string(), 0x87654321, chrono::Utc::now().timestamp() as u32).await?;
            
            let elapsed = start_time.elapsed();
            // Should include the share submission delay
            assert!(elapsed >= std::time::Duration::from_millis(200));
        }

        Ok(())
    }

    /// Test firmware version in subscribe message
    pub async fn test_firmware_version_in_subscribe(&mut self) -> Result<()> {
        let quirks = HardwareDevice::Whatsminer.connection_quirks();
        assert!(quirks.firmware_version_in_subscribe);

        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;
            
            let subscribe_msg = device.subscribe().await?;
            match subscribe_msg {
                ProtocolMessage::Subscribe { user_agent, .. } => {
                    // Should contain firmware version
                    assert!(user_agent.contains("cgminer"));
                    assert!(user_agent.contains("v1.2.3"));
                }
                _ => panic!("Expected Subscribe message"),
            }
        }

        Ok(())
    }

    /// Test high-volume share submission (due to very high hashrate)
    pub async fn test_high_volume_share_submission(&mut self) -> Result<()> {
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;
            device.authorize("whatsminer_worker.01".to_string(), "password".to_string()).await?;

            // Submit many shares to simulate high hashrate
            for i in 0..50 {
                let job_id = format!("job_{}", i);
                let nonce = 0x87654321 + i;
                let ntime = chrono::Utc::now().timestamp() as u32;
                device.submit_share(job_id, nonce, ntime).await?;

                // Process share result
                let result = if i % 100 == 0 {
                    ShareResult::Block("0000000000000000000000000000000000000000000000000000000000000000".parse().unwrap())
                } else if i % 20 == 0 {
                    ShareResult::Invalid("Low difficulty".to_string())
                } else {
                    ShareResult::Valid
                };
                device.process_share_result(result);
            }

            assert_eq!(device.get_stats().shares_submitted, 50);
            assert!(device.get_stats().shares_accepted > 0);
        }

        Ok(())
    }

    /// Test advanced difficulty range (very high maximum)
    pub async fn test_advanced_difficulty_range(&mut self) -> Result<()> {
        let quirks = HardwareDevice::Whatsminer.connection_quirks();
        
        // Should support very high difficulties due to high hashrate
        assert_eq!(quirks.min_difficulty, 1.0);
        assert_eq!(quirks.max_difficulty, 1_000_000.0);

        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            let pool_address = "127.0.0.1:3333".parse().unwrap();
            device.connect(pool_address).await?;
            device.authorize("whatsminer_worker.01".to_string(), "password".to_string()).await?;

            // Test very high difficulty that would be impossible for lower hashrate devices
            assert!(device.set_difficulty(100_000.0).await.is_ok());
            assert_eq!(device.get_state().current_difficulty, 100_000.0);
        }

        Ok(())
    }

    /// Test thermal management simulation
    pub async fn test_thermal_management(&mut self) -> Result<()> {
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            let initial_hashrate = device.get_stats().current_hashrate;

            // Simulate thermal throttling due to high power consumption
            device.stats.current_hashrate = initial_hashrate * 0.85; // 15% reduction

            let throttled_hashrate = device.get_stats().current_hashrate;
            assert!(throttled_hashrate < initial_hashrate);
            
            // Should still maintain high hashrate even when throttled
            assert!(throttled_hashrate > 80.0);

            // Simulate cooling and return to normal
            device.stats.current_hashrate = initial_hashrate;
            assert_eq!(device.get_stats().current_hashrate, initial_hashrate);
        }

        Ok(())
    }

    /// Test pool failover with high-value mining
    pub async fn test_high_value_pool_failover(&mut self) -> Result<()> {
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Whatsminer) {
            // Connect to primary pool
            let primary_pool = "127.0.0.1:3333".parse().unwrap();
            device.connect(primary_pool).await?;
            device.authorize("whatsminer_worker.01".to_string(), "password".to_string()).await?;

            // Submit high-value shares
            for i in 0..10 {
                let job_id = format!("high_value_job_{}", i);
                let nonce = 0x87654321 + i;
                let ntime = chrono::Utc::now().timestamp() as u32;
                device.submit_share(job_id, nonce, ntime).await?;
            }

            let shares_before_failover = device.get_stats().shares_submitted;

            // Simulate primary pool failure
            device.state.connected = false;
            
            // Quick failover to backup pool (critical for high-hashrate device)
            let backup_pool = "127.0.0.1:4444".parse().unwrap();
            device.connect(backup_pool).await?;
            device.authorize("whatsminer_worker.01".to_string(), "password".to_string()).await?;

            // Continue mining on backup pool without losing hashrate
            for i in 10..20 {
                let job_id = format!("backup_job_{}", i);
                let nonce = 0x87654321 + i;
                let ntime = chrono::Utc::now().timestamp() as u32;
                device.submit_share(job_id, nonce, ntime).await?;
            }

            let total_shares = device.get_stats().shares_submitted;
            assert_eq!(total_shares, 20);
            assert!(total_shares > shares_before_failover);
        }

        Ok(())
    }

    /// Run comprehensive Whatsminer test suite
    pub async fn run_full_whatsminer_tests(&mut self) -> Result<Vec<String>> {
        let mut results = Vec::new();

        // Run basic compatibility tests
        match self.test_suite.test_device_connectivity(HardwareDevice::Whatsminer).await {
            Ok(_) => results.push("Whatsminer Connectivity: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Connectivity: FAIL - {}", e)),
        }

        match self.test_sv1_compatibility().await {
            Ok(_) => results.push("Whatsminer SV1 Compatibility: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer SV1 Compatibility: FAIL - {}", e)),
        }

        match self.test_very_high_hashrate_operation().await {
            Ok(_) => results.push("Whatsminer Very High Hashrate: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Very High Hashrate: FAIL - {}", e)),
        }

        match self.test_high_power_consumption().await {
            Ok(_) => results.push("Whatsminer High Power Consumption: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer High Power Consumption: FAIL - {}", e)),
        }

        match self.test_worker_name_requirement().await {
            Ok(_) => results.push("Whatsminer Worker Name Requirement: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Worker Name Requirement: FAIL - {}", e)),
        }

        match self.test_extranonce_subscription_support().await {
            Ok(_) => results.push("Whatsminer Extranonce Subscription: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Extranonce Subscription: FAIL - {}", e)),
        }

        match self.test_no_difficulty_restart().await {
            Ok(_) => results.push("Whatsminer No Difficulty Restart: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer No Difficulty Restart: FAIL - {}", e)),
        }

        match self.test_connection_timeout().await {
            Ok(_) => results.push("Whatsminer Connection Timeout: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Connection Timeout: FAIL - {}", e)),
        }

        match self.test_share_submission_delay().await {
            Ok(_) => results.push("Whatsminer Share Submission Delay: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Share Submission Delay: FAIL - {}", e)),
        }

        match self.test_firmware_version_in_subscribe().await {
            Ok(_) => results.push("Whatsminer Firmware Version Subscribe: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Firmware Version Subscribe: FAIL - {}", e)),
        }

        match self.test_high_volume_share_submission().await {
            Ok(_) => results.push("Whatsminer High Volume Shares: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer High Volume Shares: FAIL - {}", e)),
        }

        match self.test_advanced_difficulty_range().await {
            Ok(_) => results.push("Whatsminer Advanced Difficulty Range: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Advanced Difficulty Range: FAIL - {}", e)),
        }

        match self.test_thermal_management().await {
            Ok(_) => results.push("Whatsminer Thermal Management: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer Thermal Management: FAIL - {}", e)),
        }

        match self.test_high_value_pool_failover().await {
            Ok(_) => results.push("Whatsminer High Value Pool Failover: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer High Value Pool Failover: FAIL - {}", e)),
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_whatsminer_sv1_protocol() {
        let mut whatsminer_tests = WhatsminerTests::new();
        assert!(whatsminer_tests.test_sv1_compatibility().await.is_ok());
    }

    #[tokio::test]
    async fn test_whatsminer_very_high_hashrate() {
        let mut whatsminer_tests = WhatsminerTests::new();
        assert!(whatsminer_tests.test_very_high_hashrate_operation().await.is_ok());
    }

    #[tokio::test]
    async fn test_whatsminer_extranonce_support() {
        let mut whatsminer_tests = WhatsminerTests::new();
        assert!(whatsminer_tests.test_extranonce_subscription_support().await.is_ok());
    }

    #[tokio::test]
    async fn test_whatsminer_no_restart_on_difficulty() {
        let mut whatsminer_tests = WhatsminerTests::new();
        assert!(whatsminer_tests.test_no_difficulty_restart().await.is_ok());
    }

    #[tokio::test]
    async fn test_whatsminer_high_power() {
        let mut whatsminer_tests = WhatsminerTests::new();
        assert!(whatsminer_tests.test_high_power_consumption().await.is_ok());
    }

    #[tokio::test]
    async fn test_whatsminer_high_volume_shares() {
        let mut whatsminer_tests = WhatsminerTests::new();
        assert!(whatsminer_tests.test_high_volume_share_submission().await.is_ok());
    }

    #[tokio::test]
    async fn test_whatsminer_full_test_suite() {
        let mut whatsminer_tests = WhatsminerTests::new();
        let results = whatsminer_tests.run_full_whatsminer_tests().await.unwrap();
        
        println!("Whatsminer Test Results:");
        for result in results {
            println!("  {}", result);
        }
    }
}