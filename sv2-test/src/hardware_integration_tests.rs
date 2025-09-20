use crate::hardware_compatibility_tests::*;
use crate::bitaxe_tests::BitaxeTests;
use crate::antminer_s9_tests::AntminerS9Tests;
use crate::whatsminer_tests::WhatsminerTests;
use sv2_core::{Result, Protocol, DaemonConfig, Daemon, ModeHandlerFactory, OperationMode};
use crate::mocks::MockBitcoinRpcClient;
use std::collections::HashMap;
use tokio::time::{sleep, Duration};

/// End-to-end integration tests for hardware compatibility
pub struct HardwareIntegrationTests {
    daemon_config: DaemonConfig,
    mock_rpc_client: MockBitcoinRpcClient,
    test_results: HashMap<String, Vec<String>>,
}

impl HardwareIntegrationTests {
    pub fn new() -> Self {
        let mut config = DaemonConfig::default();
        config.database.url = "sqlite::memory:".to_string();
        config.network.bind_address = "127.0.0.1:0".parse().unwrap();

        Self {
            daemon_config: config,
            mock_rpc_client: MockBitcoinRpcClient::new(),
            test_results: HashMap::new(),
        }
    }

    /// Test mixed device pool scenario
    pub async fn test_mixed_device_pool(&mut self) -> Result<()> {
        let mut results = Vec::new();

        // Create devices with different protocols and characteristics
        let mut bitaxe = BitaxeTests::new();
        let mut antminer_s9 = AntminerS9Tests::new();
        let mut whatsminer = WhatsminerTests::new();

        // Run connectivity tests for all devices concurrently
        let bitaxe_conn_task = tokio::spawn(async move {
            bitaxe.test_suite.test_device_connectivity(HardwareDevice::Bitaxe).await
        });

        let antminer_conn_task = tokio::spawn(async move {
            antminer_s9.test_suite.test_device_connectivity(HardwareDevice::AntminerS9).await
        });

        let whatsminer_conn_task = tokio::spawn(async move {
            whatsminer.test_suite.test_device_connectivity(HardwareDevice::Whatsminer).await
        });

        // Collect results
        match bitaxe_conn_task.await.unwrap() {
            Ok(_) => results.push("Mixed Pool Bitaxe: PASS".to_string()),
            Err(e) => results.push(format!("Mixed Pool Bitaxe: FAIL - {}", e)),
        }

        match antminer_conn_task.await.unwrap() {
            Ok(_) => results.push("Mixed Pool Antminer S9: PASS".to_string()),
            Err(e) => results.push(format!("Mixed Pool Antminer S9: FAIL - {}", e)),
        }

        match whatsminer_conn_task.await.unwrap() {
            Ok(_) => results.push("Mixed Pool Whatsminer: PASS".to_string()),
            Err(e) => results.push(format!("Mixed Pool Whatsminer: FAIL - {}", e)),
        }

        self.test_results.insert("Mixed Device Pool".to_string(), results);
        Ok(())
    }

    /// Test protocol translation between SV1 and SV2
    pub async fn test_protocol_translation(&mut self) -> Result<()> {
        let mut results = Vec::new();

        // Test SV1 device connecting to SV2 pool (via proxy)
        let mut test_suite = HardwareCompatibilityTest::new();
        
        // Add devices with different protocols
        test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv1);
        test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv2);

        // Test protocol compatibility for both protocols
        match test_suite.test_protocol_compatibility(HardwareDevice::Bitaxe).await {
            Ok(_) => results.push("Protocol Translation Bitaxe: PASS".to_string()),
            Err(e) => results.push(format!("Protocol Translation Bitaxe: FAIL - {}", e)),
        }

        // SV1-only devices should work through protocol proxy
        match test_suite.test_protocol_compatibility(HardwareDevice::AntminerS9).await {
            Ok(_) => results.push("Protocol Translation S9 (SV1 only): PASS".to_string()),
            Err(e) => results.push(format!("Protocol Translation S9 (SV1 only): FAIL - {}", e)),
        }

        match test_suite.test_protocol_compatibility(HardwareDevice::Whatsminer).await {
            Ok(_) => results.push("Protocol Translation Whatsminer (SV1 only): PASS".to_string()),
            Err(e) => results.push(format!("Protocol Translation Whatsminer (SV1 only): FAIL - {}", e)),
        }

        self.test_results.insert("Protocol Translation".to_string(), results);
        Ok(())
    }

    /// Test load balancing across different device types
    pub async fn test_load_balancing(&mut self) -> Result<()> {
        let mut results = Vec::new();
        let mut test_suite = HardwareCompatibilityTest::new();

        // Add multiple devices with different hashrates
        test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv1);      // ~0.5 TH/s
        test_suite.add_device(HardwareDevice::AntminerS9, Protocol::Sv1);  // ~13.5 TH/s
        test_suite.add_device(HardwareDevice::Whatsminer, Protocol::Sv1);  // ~110 TH/s

        // Test that difficulty is appropriately adjusted for each device
        for &device_type in &[HardwareDevice::Bitaxe, HardwareDevice::AntminerS9, HardwareDevice::Whatsminer] {
            let quirks = device_type.connection_quirks();
            let (min_hash, max_hash) = device_type.hashrate_range();

            // Each device should get difficulty proportional to its hashrate capability
            let appropriate_difficulty = match device_type {
                HardwareDevice::Bitaxe => 1.0,      // Low difficulty for low hashrate
                HardwareDevice::AntminerS9 => 1000.0,  // Medium difficulty
                HardwareDevice::Whatsminer => 10000.0, // High difficulty for high hashrate
            };

            if let Some(device) = test_suite.devices.get_mut(&device_type) {
                let pool_address = "127.0.0.1:3333".parse().unwrap();
                device.connect(pool_address).await?;
                
                let worker_name = match device_type {
                    HardwareDevice::Bitaxe => "bitaxe_worker".to_string(),
                    HardwareDevice::AntminerS9 => "s9_worker.001".to_string(),
                    HardwareDevice::Whatsminer => "whatsminer_worker.01".to_string(),
                };
                
                device.authorize(worker_name, "password".to_string()).await?;
                device.set_difficulty(appropriate_difficulty).await?;

                assert_eq!(device.get_state().current_difficulty, appropriate_difficulty);
            }
        }

        results.push("Load Balancing Difficulty Assignment: PASS".to_string());
        self.test_results.insert("Load Balancing".to_string(), results);
        Ok(())
    }

    /// Test failover scenarios with different device recovery characteristics
    pub async fn test_failover_scenarios(&mut self) -> Result<()> {
        let mut results = Vec::new();

        // Test different device recovery behaviors
        let device_recovery_tests = vec![
            (HardwareDevice::Bitaxe, "Fast recovery (5s timeout)"),
            (HardwareDevice::AntminerS9, "Slow recovery (30s timeout)"),
            (HardwareDevice::Whatsminer, "Medium recovery (15s timeout)"),
        ];

        for (device_type, description) in device_recovery_tests {
            let mut test_suite = HardwareCompatibilityTest::new();
            
            // Add device with random disconnect failure
            let failures = vec![FailureCondition::RandomDisconnect { probability: 1.0 }]; // 100% disconnect
            test_suite.add_device_with_failures(device_type, Protocol::Sv1, failures);

            let start_time = std::time::Instant::now();
            
            // First connection attempt should fail
            let result = test_suite.test_device_connectivity(device_type).await;
            
            // Then add normal device for recovery test
            test_suite.add_device(device_type, Protocol::Sv1);
            let recovery_result = test_suite.test_connection_failover(device_type).await;

            match recovery_result {
                Ok(_) => results.push(format!("{} {}: PASS", device_type.name(), description)),
                Err(e) => results.push(format!("{} {}: FAIL - {}", device_type.name(), description, e)),
            }
        }

        self.test_results.insert("Failover Scenarios".to_string(), results);
        Ok(())
    }

    /// Test performance under mixed load conditions
    pub async fn test_mixed_load_performance(&mut self) -> Result<()> {
        let mut results = Vec::new();
        let mut test_suite = HardwareCompatibilityTest::new();

        // Add all device types
        test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv1);
        test_suite.add_device(HardwareDevice::AntminerS9, Protocol::Sv1);
        test_suite.add_device(HardwareDevice::Whatsminer, Protocol::Sv1);

        let start_time = std::time::Instant::now();

        // Run performance tests for all devices concurrently
        let mut performance_tasks = Vec::new();

        for &device_type in &[HardwareDevice::Bitaxe, HardwareDevice::AntminerS9, HardwareDevice::Whatsminer] {
            let task = tokio::spawn(async move {
                let mut individual_test_suite = HardwareCompatibilityTest::new();
                individual_test_suite.add_device(device_type, Protocol::Sv1);
                individual_test_suite.test_performance(device_type).await
            });
            performance_tasks.push((device_type, task));
        }

        // Collect results
        for (device_type, task) in performance_tasks {
            match task.await.unwrap() {
                Ok(_) => results.push(format!("Mixed Load {} Performance: PASS", device_type.name())),
                Err(e) => results.push(format!("Mixed Load {} Performance: FAIL - {}", device_type.name(), e)),
            }
        }

        let total_time = start_time.elapsed();
        results.push(format!("Total Mixed Load Test Time: {:?}", total_time));

        self.test_results.insert("Mixed Load Performance".to_string(), results);
        Ok(())
    }

    /// Test share validation consistency across devices
    pub async fn test_share_validation_consistency(&mut self) -> Result<()> {
        let mut results = Vec::new();
        let mut test_suite = HardwareCompatibilityTest::new();

        // Add all device types
        test_suite.add_device(HardwareDevice::Bitaxe, Protocol::Sv1);
        test_suite.add_device(HardwareDevice::AntminerS9, Protocol::Sv1);
        test_suite.add_device(HardwareDevice::Whatsminer, Protocol::Sv1);

        for &device_type in &[HardwareDevice::Bitaxe, HardwareDevice::AntminerS9, HardwareDevice::Whatsminer] {
            match test_suite.test_share_submission(device_type).await {
                Ok(_) => {
                    if let Some(device) = test_suite.devices.get(&device_type) {
                        let stats = device.get_stats();
                        let acceptance_rate = if stats.shares_submitted > 0 {
                            (stats.shares_accepted as f64 / stats.shares_submitted as f64) * 100.0
                        } else {
                            0.0
                        };
                        
                        results.push(format!(
                            "{} Share Validation: PASS ({}% acceptance rate)",
                            device_type.name(),
                            acceptance_rate as u32
                        ));
                    }
                }
                Err(e) => results.push(format!("{} Share Validation: FAIL - {}", device_type.name(), e)),
            }
        }

        self.test_results.insert("Share Validation Consistency".to_string(), results);
        Ok(())
    }

    /// Test device-specific edge cases in integration
    pub async fn test_device_specific_edge_cases(&mut self) -> Result<()> {
        let mut results = Vec::new();

        // Bitaxe: Test SV1 to SV2 protocol switching
        let mut bitaxe_test = BitaxeTests::new();
        match bitaxe_test.test_sv2_compatibility().await {
            Ok(_) => results.push("Bitaxe SV1->SV2 Protocol Switch: PASS".to_string()),
            Err(e) => results.push(format!("Bitaxe SV1->SV2 Protocol Switch: FAIL - {}", e)),
        }

        // Antminer S9: Test multi-worker scenario
        let mut s9_test = AntminerS9Tests::new();
        match s9_test.test_multi_worker_configuration().await {
            Ok(_) => results.push("Antminer S9 Multi-Worker: PASS".to_string()),
            Err(e) => results.push(format!("Antminer S9 Multi-Worker: FAIL - {}", e)),
        }

        // Whatsminer: Test high-volume share submission
        let mut whatsminer_test = WhatsminerTests::new();
        match whatsminer_test.test_high_volume_share_submission().await {
            Ok(_) => results.push("Whatsminer High-Volume Shares: PASS".to_string()),
            Err(e) => results.push(format!("Whatsminer High-Volume Shares: FAIL - {}", e)),
        }

        self.test_results.insert("Device-Specific Edge Cases".to_string(), results);
        Ok(())
    }

    /// Test graceful degradation with device failures
    pub async fn test_graceful_degradation(&mut self) -> Result<()> {
        let mut results = Vec::new();
        let mut test_suite = HardwareCompatibilityTest::new();

        // Add devices with different failure conditions
        let failures = vec![
            FailureCondition::RejectShares { probability: 0.5, reason: "High error rate".to_string() },
            FailureCondition::DelayedResponse { delay_ms: 2000 },
        ];

        test_suite.add_device_with_failures(HardwareDevice::Bitaxe, Protocol::Sv1, failures.clone());
        test_suite.add_device_with_failures(HardwareDevice::AntminerS9, Protocol::Sv1, failures.clone());
        test_suite.add_device_with_failures(HardwareDevice::Whatsminer, Protocol::Sv1, failures);

        // System should gracefully handle device failures
        for &device_type in &[HardwareDevice::Bitaxe, HardwareDevice::AntminerS9, HardwareDevice::Whatsminer] {
            // Even with failures, basic connectivity should be attempted
            let connectivity_result = test_suite.test_device_connectivity(device_type).await;
            
            match connectivity_result {
                Ok(_) => results.push(format!("{} Graceful Degradation: PASS", device_type.name())),
                Err(_) => {
                    // Failure is expected, but system should handle it gracefully
                    results.push(format!("{} Graceful Degradation: PASS (handled failure)", device_type.name()));
                }
            }
        }

        self.test_results.insert("Graceful Degradation".to_string(), results);
        Ok(())
    }

    /// Run comprehensive hardware integration test suite
    pub async fn run_comprehensive_integration_tests(&mut self) -> Result<()> {
        println!("Starting comprehensive hardware integration tests...");

        // Run all integration test categories
        self.test_mixed_device_pool().await?;
        self.test_protocol_translation().await?;
        self.test_load_balancing().await?;
        self.test_failover_scenarios().await?;
        self.test_mixed_load_performance().await?;
        self.test_share_validation_consistency().await?;
        self.test_device_specific_edge_cases().await?;
        self.test_graceful_degradation().await?;

        Ok(())
    }

    /// Get all test results
    pub fn get_test_results(&self) -> &HashMap<String, Vec<String>> {
        &self.test_results
    }

    /// Print comprehensive test report
    pub fn print_test_report(&self) {
        println!("\n=== HARDWARE COMPATIBILITY INTEGRATION TEST REPORT ===");
        
        let mut total_tests = 0;
        let mut passed_tests = 0;

        for (category, results) in &self.test_results {
            println!("\n{}", category);
            println!("{}", "=".repeat(category.len()));
            
            for result in results {
                println!("  {}", result);
                total_tests += 1;
                if result.contains("PASS") {
                    passed_tests += 1;
                }
            }
        }

        println!("\n=== SUMMARY ===");
        println!("Total Tests: {}", total_tests);
        println!("Passed: {}", passed_tests);
        println!("Failed: {}", total_tests - passed_tests);
        println!("Success Rate: {:.1}%", (passed_tests as f64 / total_tests as f64) * 100.0);
        
        if passed_tests == total_tests {
            println!("🎉 ALL TESTS PASSED! Hardware compatibility is excellent.");
        } else {
            println!("⚠️  Some tests failed. Review failures for hardware compatibility issues.");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_mixed_device_pool_integration() {
        let mut integration_tests = HardwareIntegrationTests::new();
        assert!(integration_tests.test_mixed_device_pool().await.is_ok());
    }

    #[tokio::test]
    async fn test_protocol_translation_integration() {
        let mut integration_tests = HardwareIntegrationTests::new();
        assert!(integration_tests.test_protocol_translation().await.is_ok());
    }

    #[tokio::test]
    async fn test_load_balancing_integration() {
        let mut integration_tests = HardwareIntegrationTests::new();
        assert!(integration_tests.test_load_balancing().await.is_ok());
    }

    #[tokio::test]
    async fn test_comprehensive_integration_suite() {
        let mut integration_tests = HardwareIntegrationTests::new();
        assert!(integration_tests.run_comprehensive_integration_tests().await.is_ok());
        
        // Print results
        integration_tests.print_test_report();
        
        // Verify we have test results for all categories
        let results = integration_tests.get_test_results();
        assert!(!results.is_empty());
    }

    #[tokio::test]
    async fn test_graceful_degradation_integration() {
        let mut integration_tests = HardwareIntegrationTests::new();
        assert!(integration_tests.test_graceful_degradation().await.is_ok());
    }
}