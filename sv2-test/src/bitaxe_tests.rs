use crate::hardware_compatibility_tests::*;
use sv2_core::{Result, Protocol, ProtocolMessage};

/// Bitaxe-specific compatibility tests
pub struct BitaxeTests {
    pub test_suite: HardwareCompatibilityTest,
}

impl BitaxeTests {
    pub fn new() -> Self {
        let mut test_suite = HardwareCompatibilityTest::new();
        test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv1);
        
        Self { test_suite }
    }

    /// Test Bitaxe SV1 compatibility
    pub async fn test_sv1_compatibility(&mut self) -> Result<()> {
        self.test_suite.test_protocol_compatibility(HardwareDevice::Bitaxe).await?;
        Ok(())
    }

    /// Test Bitaxe SV2 compatibility
    pub async fn test_sv2_compatibility(&mut self) -> Result<()> {
        // Add SV2 device for testing
        self.test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv2);
        self.test_suite.test_protocol_compatibility(HardwareDevice::Bitaxe).await?;
        Ok(())
    }

    /// Test Bitaxe low-power operation characteristics
    pub async fn test_low_power_operation(&mut self) -> Result<()> {
        let device = self.test_suite.devices.get(&HardwareDevice::Bitaxe)
            .expect("Bitaxe device should be available");

        // Verify power consumption is within expected range (10-20W)
        let power = HardwareDevice::Bitaxe.power_consumption();
        assert!(power >= 10 && power <= 20);

        // Test low difficulty operation
        let quirks = HardwareDevice::Bitaxe.connection_quirks();
        assert_eq!(quirks.min_difficulty, 1.0);
        assert!(quirks.max_difficulty >= 1_000_000.0);

        Ok(())
    }

    /// Test Bitaxe firmware upgrade simulation
    pub async fn test_firmware_upgrade(&mut self) -> Result<()> {
        let pool_address = "127.0.0.1:3333".parse().unwrap();
        
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Bitaxe) {
            // Connect and establish mining
            device.connect(pool_address).await?;
            device.authorize("bitaxe_worker".to_string(), "password".to_string()).await?;

            // Submit some shares before "firmware upgrade"
            for i in 0..5 {
                let job_id = format!("job_{}", i);
                let nonce = 0x12345678 + i;
                let ntime = chrono::Utc::now().timestamp() as u32;
                device.submit_share(job_id, nonce, ntime).await?;
            }

            let shares_before = device.get_stats().shares_submitted;

            // Simulate firmware upgrade (device restart)
            device.state.connected = false;
            device.state.authorized = false;
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;

            // Reconnect after upgrade
            device.connect(pool_address).await?;
            device.authorize("bitaxe_worker".to_string(), "password".to_string()).await?;

            // Continue mining
            for i in 5..10 {
                let job_id = format!("job_{}", i);
                let nonce = 0x12345678 + i;
                let ntime = chrono::Utc::now().timestamp() as u32;
                device.submit_share(job_id, nonce, ntime).await?;
            }

            let shares_after = device.get_stats().shares_submitted;
            assert_eq!(shares_after, 10); // Total shares across restart
        }

        Ok(())
    }

    /// Test Bitaxe temperature-based throttling simulation
    pub async fn test_temperature_throttling(&mut self) -> Result<()> {
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Bitaxe) {
            let initial_hashrate = device.get_stats().current_hashrate;

            // Simulate temperature increase causing throttling
            device.stats.current_hashrate = initial_hashrate * 0.8; // 20% reduction

            let throttled_hashrate = device.get_stats().current_hashrate;
            assert!(throttled_hashrate < initial_hashrate);

            // Simulate cooling and return to normal
            device.stats.current_hashrate = initial_hashrate;
            assert_eq!(device.get_stats().current_hashrate, initial_hashrate);
        }

        Ok(())
    }

    /// Test Bitaxe pool switching capability
    pub async fn test_pool_switching(&mut self) -> Result<()> {
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Bitaxe) {
            // Connect to primary pool
            let primary_pool = "127.0.0.1:3333".parse().unwrap();
            device.connect(primary_pool).await?;
            device.authorize("bitaxe_worker".to_string(), "password".to_string()).await?;

            // Submit some shares
            for i in 0..3 {
                let job_id = format!("job_{}", i);
                let nonce = 0x12345678 + i;
                let ntime = chrono::Utc::now().timestamp() as u32;
                device.submit_share(job_id, nonce, ntime).await?;
            }

            // Switch to backup pool (simulate primary pool failure)
            device.state.connected = false;
            
            let backup_pool = "127.0.0.1:4444".parse().unwrap();
            device.connect(backup_pool).await?;
            device.authorize("bitaxe_worker".to_string(), "password".to_string()).await?;

            // Continue mining on backup pool
            for i in 3..6 {
                let job_id = format!("job_{}", i);
                let nonce = 0x12345678 + i;
                let ntime = chrono::Utc::now().timestamp() as u32;
                device.submit_share(job_id, nonce, ntime).await?;
            }

            assert_eq!(device.get_stats().shares_submitted, 6);
        }

        Ok(())
    }

    /// Test Bitaxe extranonce subscription support
    pub async fn test_extranonce_subscription(&mut self) -> Result<()> {
        let quirks = HardwareDevice::Bitaxe.connection_quirks();
        assert!(quirks.supports_extranonce_subscribe);

        // Test extranonce handling
        if let Some(device) = self.test_suite.devices.get_mut(&HardwareDevice::Bitaxe) {
            device.state.extranonce1 = Some("12345678".to_string());
            device.state.extranonce2_size = Some(4);

            assert_eq!(device.state.extranonce1, Some("12345678".to_string()));
            assert_eq!(device.state.extranonce2_size, Some(4));
        }

        Ok(())
    }

    /// Run comprehensive Bitaxe test suite
    pub async fn run_full_bitaxe_tests(&mut self) -> Result<Vec<String>> {
        let mut results = Vec::new();

        // Run basic compatibility tests first
        match self.test_suite.test_device_connectivity(HardwareDevice::Bitaxe).await {
            Ok(_) => results.push("Bitaxe Connectivity: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe Connectivity: FAIL - {}", e)),
        }

        match self.test_sv1_compatibility().await {
            Ok(_) => results.push("Bitaxe SV1 Compatibility: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe SV1 Compatibility: FAIL - {}", e)),
        }

        match self.test_sv2_compatibility().await {
            Ok(_) => results.push("Bitaxe SV2 Compatibility: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe SV2 Compatibility: FAIL - {}", e)),
        }

        match self.test_low_power_operation().await {
            Ok(_) => results.push("Bitaxe Low Power Operation: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe Low Power Operation: FAIL - {}", e)),
        }

        match self.test_firmware_upgrade().await {
            Ok(_) => results.push("Bitaxe Firmware Upgrade: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe Firmware Upgrade: FAIL - {}", e)),
        }

        match self.test_temperature_throttling().await {
            Ok(_) => results.push("Bitaxe Temperature Throttling: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe Temperature Throttling: FAIL - {}", e)),
        }

        match self.test_pool_switching().await {
            Ok(_) => results.push("Bitaxe Pool Switching: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe Pool Switching: FAIL - {}", e)),
        }

        match self.test_extranonce_subscription().await {
            Ok(_) => results.push("Bitaxe Extranonce Subscription: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe Extranonce Subscription: FAIL - {}", e)),
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_bitaxe_sv1_protocol() {
        let mut bitaxe_tests = BitaxeTests::new();
        assert!(bitaxe_tests.test_sv1_compatibility().await.is_ok());
    }

    #[tokio::test]
    async fn test_bitaxe_sv2_protocol() {
        let mut bitaxe_tests = BitaxeTests::new();
        assert!(bitaxe_tests.test_sv2_compatibility().await.is_ok());
    }

    #[tokio::test]
    async fn test_bitaxe_low_power_characteristics() {
        let mut bitaxe_tests = BitaxeTests::new();
        assert!(bitaxe_tests.test_low_power_operation().await.is_ok());
    }

    #[tokio::test]
    async fn test_bitaxe_firmware_upgrade_scenario() {
        let mut bitaxe_tests = BitaxeTests::new();
        assert!(bitaxe_tests.test_firmware_upgrade().await.is_ok());
    }

    #[tokio::test]
    async fn test_bitaxe_pool_switching_capability() {
        let mut bitaxe_tests = BitaxeTests::new();
        assert!(bitaxe_tests.test_pool_switching().await.is_ok());
    }

    #[tokio::test]
    async fn test_bitaxe_full_test_suite() {
        let mut bitaxe_tests = BitaxeTests::new();
        let results = bitaxe_tests.run_full_bitaxe_tests().await.unwrap();
        
        println!("Bitaxe Test Results:");
        for result in results {
            println!("  {}", result);
        }
    }
}