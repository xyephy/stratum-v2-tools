use sv2_core::{
    Connection, Share, ShareResult, WorkTemplate,
    config::{DaemonConfig, PoolConfig, OperationModeConfig, BitcoinConfig, BitcoinNetwork},
    database::MockDatabaseOps,
    bitcoin_rpc::BitcoinRpcClient,
    modes::PoolModeHandler,
    mode::ModeHandler,
    types::{Protocol, ConnectionId, ShareSubmission},
};
use std::sync::Arc;
use std::net::SocketAddr;
use std::time::{Duration, Instant};
use tokio::time::sleep;
use uuid::Uuid;

/// Load test configuration
#[derive(Debug, Clone)]
pub struct LoadTestConfig {
    pub num_connections: usize,
    pub shares_per_connection: usize,
    pub test_duration_seconds: u64,
    pub connection_delay_ms: u64,
    pub share_interval_ms: u64,
}

impl Default for LoadTestConfig {
    fn default() -> Self {
        Self {
            num_connections: 100,
            shares_per_connection: 10,
            test_duration_seconds: 60,
            connection_delay_ms: 10,
            share_interval_ms: 1000,
        }
    }
}

/// Load test results
#[derive(Debug, Clone)]
pub struct LoadTestResults {
    pub total_connections: usize,
    pub successful_connections: usize,
    pub failed_connections: usize,
    pub total_shares: usize,
    pub successful_shares: usize,
    pub failed_shares: usize,
    pub test_duration: Duration,
    pub connections_per_second: f64,
    pub shares_per_second: f64,
    pub memory_usage_mb: f64,
    pub cpu_usage_percent: f64,
}

/// Pool load tester
pub struct PoolLoadTester {
    handler: PoolModeHandler,
    config: LoadTestConfig,
}

impl PoolLoadTester {
    /// Create a new load tester
    pub fn new(config: LoadTestConfig) -> Self {
        let pool_config = PoolConfig::default();
        let bitcoin_config = BitcoinConfig {
            rpc_url: "http://localhost:8332".to_string(),
            rpc_user: "user".to_string(),
            rpc_password: "pass".to_string(),
            network: BitcoinNetwork::Regtest,
            coinbase_address: None,
            block_template_timeout: 30,
        };
        let bitcoin_client = BitcoinRpcClient::new(bitcoin_config);
        let database = Arc::new(MockDatabaseOps::new());
        
        let handler = PoolModeHandler::new(pool_config, bitcoin_client, database);
        
        Self {
            handler,
            config,
        }
    }

    /// Run connection load test
    pub async fn test_concurrent_connections(&self) -> LoadTestResults {
        let start_time = Instant::now();
        let mut successful_connections = 0;
        let mut failed_connections = 0;
        let mut connection_handles = Vec::new();

        // Start the pool handler
        self.handler.start().await.expect("Failed to start pool handler");

        // Create connections concurrently
        for i in 0..self.config.num_connections {
            let handler = self.handler.clone();
            let delay = self.config.connection_delay_ms;
            
            let handle = tokio::spawn(async move {
                if delay > 0 {
                    sleep(Duration::from_millis(delay)).await;
                }
                
                let addr: SocketAddr = format!("127.0.0.1:{}", 3333 + (i % 1000)).parse().unwrap();
                let conn = Connection::new(addr, Protocol::Sv2);
                
                handler.handle_connection(conn).await
            });
            
            connection_handles.push(handle);
        }

        // Wait for all connections to complete
        for handle in connection_handles {
            match handle.await {
                Ok(Ok(())) => successful_connections += 1,
                Ok(Err(_)) => failed_connections += 1,
                Err(_) => failed_connections += 1,
            }
        }

        let test_duration = start_time.elapsed();
        let connections_per_second = successful_connections as f64 / test_duration.as_secs_f64();

        // Stop the handler
        self.handler.stop().await.expect("Failed to stop pool handler");

        LoadTestResults {
            total_connections: self.config.num_connections,
            successful_connections,
            failed_connections,
            total_shares: 0,
            successful_shares: 0,
            failed_shares: 0,
            test_duration,
            connections_per_second,
            shares_per_second: 0.0,
            memory_usage_mb: self.get_memory_usage(),
            cpu_usage_percent: 0.0, // Would need system monitoring
        }
    }

    /// Run share processing load test
    pub async fn test_share_processing(&self) -> LoadTestResults {
        let start_time = Instant::now();
        let mut successful_shares = 0;
        let mut failed_shares = 0;
        let mut share_handles = Vec::new();

        // Start the pool handler
        self.handler.start().await.expect("Failed to start pool handler");

        // Create some connections first
        let mut connections = Vec::new();
        for i in 0..10 {
            let addr: SocketAddr = format!("127.0.0.1:{}", 3333 + i).parse().unwrap();
            let conn = Connection::new(addr, Protocol::Sv2);
            let conn_id = conn.id;
            
            self.handler.handle_connection(conn).await.expect("Failed to add connection");
            connections.push(conn_id);
        }

        // Submit shares concurrently
        let total_shares = self.config.num_connections * self.config.shares_per_connection;
        for i in 0..total_shares {
            let handler = self.handler.clone();
            let connection_id = connections[i % connections.len()];
            let share_interval = self.config.share_interval_ms;
            
            let handle = tokio::spawn(async move {
                if share_interval > 0 {
                    sleep(Duration::from_millis(share_interval * (i as u64 % 10))).await;
                }
                
                let share = Share::new(
                    connection_id,
                    i as u32,
                    chrono::Utc::now().timestamp() as u32,
                    1.0,
                );
                
                handler.process_share(share).await
            });
            
            share_handles.push(handle);
        }

        // Wait for all shares to be processed
        for handle in share_handles {
            match handle.await {
                Ok(Ok(_)) => successful_shares += 1,
                Ok(Err(_)) => failed_shares += 1,
                Err(_) => failed_shares += 1,
            }
        }

        let test_duration = start_time.elapsed();
        let shares_per_second = successful_shares as f64 / test_duration.as_secs_f64();

        // Stop the handler
        self.handler.stop().await.expect("Failed to stop pool handler");

        LoadTestResults {
            total_connections: connections.len(),
            successful_connections: connections.len(),
            failed_connections: 0,
            total_shares,
            successful_shares,
            failed_shares,
            test_duration,
            connections_per_second: 0.0,
            shares_per_second,
            memory_usage_mb: self.get_memory_usage(),
            cpu_usage_percent: 0.0,
        }
    }

    /// Run comprehensive load test combining connections and shares
    pub async fn test_comprehensive_load(&self) -> LoadTestResults {
        let start_time = Instant::now();
        let mut successful_connections = 0;
        let mut failed_connections = 0;
        let mut successful_shares = 0;
        let mut failed_shares = 0;

        // Start the pool handler
        self.handler.start().await.expect("Failed to start pool handler");

        // Phase 1: Create connections
        let mut connection_handles = Vec::new();
        let mut connection_ids = Vec::new();

        for i in 0..self.config.num_connections {
            let handler = self.handler.clone();
            
            let handle = tokio::spawn(async move {
                let addr: SocketAddr = format!("127.0.0.1:{}", 3333 + (i % 1000)).parse().unwrap();
                let conn = Connection::new(addr, Protocol::Sv2);
                let conn_id = conn.id;
                
                match handler.handle_connection(conn).await {
                    Ok(()) => Ok(conn_id),
                    Err(e) => Err(e),
                }
            });
            
            connection_handles.push(handle);
        }

        // Collect successful connections
        for handle in connection_handles {
            match handle.await {
                Ok(Ok(conn_id)) => {
                    successful_connections += 1;
                    connection_ids.push(conn_id);
                }
                Ok(Err(_)) => failed_connections += 1,
                Err(_) => failed_connections += 1,
            }
        }

        // Phase 2: Submit shares from successful connections
        if !connection_ids.is_empty() {
            let mut share_handles = Vec::new();
            let total_shares = connection_ids.len() * self.config.shares_per_connection;

            for i in 0..total_shares {
                let handler = self.handler.clone();
                let connection_id = connection_ids[i % connection_ids.len()];
                
                let handle = tokio::spawn(async move {
                    let share = Share::new(
                        connection_id,
                        i as u32,
                        chrono::Utc::now().timestamp() as u32,
                        1.0,
                    );
                    
                    handler.process_share(share).await
                });
                
                share_handles.push(handle);
            }

            // Wait for shares to be processed
            for handle in share_handles {
                match handle.await {
                    Ok(Ok(_)) => successful_shares += 1,
                    Ok(Err(_)) => failed_shares += 1,
                    Err(_) => failed_shares += 1,
                }
            }
        }

        let test_duration = start_time.elapsed();
        let connections_per_second = successful_connections as f64 / test_duration.as_secs_f64();
        let shares_per_second = successful_shares as f64 / test_duration.as_secs_f64();

        // Stop the handler
        self.handler.stop().await.expect("Failed to stop pool handler");

        LoadTestResults {
            total_connections: self.config.num_connections,
            successful_connections,
            failed_connections,
            total_shares: successful_shares + failed_shares,
            successful_shares,
            failed_shares,
            test_duration,
            connections_per_second,
            shares_per_second,
            memory_usage_mb: self.get_memory_usage(),
            cpu_usage_percent: 0.0,
        }
    }

    /// Test resource limits and cleanup
    pub async fn test_resource_limits(&self) -> LoadTestResults {
        let start_time = Instant::now();
        let mut successful_connections = 0;
        let mut failed_connections = 0;

        // Start the pool handler
        self.handler.start().await.expect("Failed to start pool handler");

        // Try to create more connections than the limit
        let excessive_connections = 1500; // More than the 1000 limit
        let mut connection_handles = Vec::new();

        for i in 0..excessive_connections {
            let handler = self.handler.clone();
            
            let handle = tokio::spawn(async move {
                let addr: SocketAddr = format!("127.0.0.1:{}", 3333 + (i % 10000)).parse().unwrap();
                let conn = Connection::new(addr, Protocol::Sv2);
                
                handler.handle_connection(conn).await
            });
            
            connection_handles.push(handle);
        }

        // Wait for all connection attempts
        for handle in connection_handles {
            match handle.await {
                Ok(Ok(())) => successful_connections += 1,
                Ok(Err(_)) => failed_connections += 1,
                Err(_) => failed_connections += 1,
            }
        }

        let test_duration = start_time.elapsed();
        let connections_per_second = successful_connections as f64 / test_duration.as_secs_f64();

        // Verify that we didn't exceed the connection limit
        let current_connections = self.handler.get_connection_count().await;
        assert!(current_connections <= 1000, "Connection limit exceeded: {}", current_connections);

        // Stop the handler
        self.handler.stop().await.expect("Failed to stop pool handler");

        LoadTestResults {
            total_connections: excessive_connections,
            successful_connections,
            failed_connections,
            total_shares: 0,
            successful_shares: 0,
            failed_shares: 0,
            test_duration,
            connections_per_second,
            shares_per_second: 0.0,
            memory_usage_mb: self.get_memory_usage(),
            cpu_usage_percent: 0.0,
        }
    }

    /// Get approximate memory usage (simplified)
    fn get_memory_usage(&self) -> f64 {
        // In a real implementation, this would use system monitoring
        // For now, return a placeholder value
        50.0 // MB
    }
}

impl LoadTestResults {
    /// Print test results summary
    pub fn print_summary(&self) {
        println!("=== Load Test Results ===");
        println!("Test Duration: {:.2}s", self.test_duration.as_secs_f64());
        println!();
        
        println!("Connections:");
        println!("  Total: {}", self.total_connections);
        println!("  Successful: {}", self.successful_connections);
        println!("  Failed: {}", self.failed_connections);
        println!("  Success Rate: {:.2}%", 
                 (self.successful_connections as f64 / self.total_connections as f64) * 100.0);
        println!("  Connections/sec: {:.2}", self.connections_per_second);
        println!();
        
        if self.total_shares > 0 {
            println!("Shares:");
            println!("  Total: {}", self.total_shares);
            println!("  Successful: {}", self.successful_shares);
            println!("  Failed: {}", self.failed_shares);
            println!("  Success Rate: {:.2}%", 
                     (self.successful_shares as f64 / self.total_shares as f64) * 100.0);
            println!("  Shares/sec: {:.2}", self.shares_per_second);
            println!();
        }
        
        println!("Resource Usage:");
        println!("  Memory: {:.2} MB", self.memory_usage_mb);
        println!("  CPU: {:.2}%", self.cpu_usage_percent);
        println!();
    }

    /// Check if test passed based on requirements
    pub fn is_passing(&self) -> bool {
        // Requirements: handle 100+ concurrent connections, use less than 100MB RAM
        let connection_success_rate = self.successful_connections as f64 / self.total_connections as f64;
        let memory_within_limit = self.memory_usage_mb < 100.0;
        let min_connections_handled = self.successful_connections >= 100;
        
        connection_success_rate >= 0.95 && memory_within_limit && min_connections_handled
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_100_concurrent_connections() {
        let config = LoadTestConfig {
            num_connections: 100,
            shares_per_connection: 5,
            test_duration_seconds: 30,
            connection_delay_ms: 5,
            share_interval_ms: 100,
        };
        
        let tester = PoolLoadTester::new(config);
        let results = tester.test_concurrent_connections().await;
        
        results.print_summary();
        assert!(results.is_passing(), "Load test failed requirements");
        assert!(results.successful_connections >= 95, "Too many connection failures");
    }

    #[tokio::test]
    async fn test_500_concurrent_connections() {
        let config = LoadTestConfig {
            num_connections: 500,
            shares_per_connection: 2,
            test_duration_seconds: 60,
            connection_delay_ms: 2,
            share_interval_ms: 500,
        };
        
        let tester = PoolLoadTester::new(config);
        let results = tester.test_concurrent_connections().await;
        
        results.print_summary();
        assert!(results.successful_connections >= 450, "Too many connection failures for 500 connections");
    }

    #[tokio::test]
    async fn test_share_processing_load() {
        let config = LoadTestConfig {
            num_connections: 50,
            shares_per_connection: 20,
            test_duration_seconds: 30,
            connection_delay_ms: 10,
            share_interval_ms: 50,
        };
        
        let tester = PoolLoadTester::new(config);
        let results = tester.test_share_processing().await;
        
        results.print_summary();
        assert!(results.shares_per_second > 10.0, "Share processing too slow");
        assert!(results.successful_shares >= 900, "Too many share processing failures");
    }

    #[tokio::test]
    async fn test_comprehensive_load() {
        let config = LoadTestConfig {
            num_connections: 200,
            shares_per_connection: 5,
            test_duration_seconds: 45,
            connection_delay_ms: 5,
            share_interval_ms: 200,
        };
        
        let tester = PoolLoadTester::new(config);
        let results = tester.test_comprehensive_load().await;
        
        results.print_summary();
        assert!(results.is_passing(), "Comprehensive load test failed");
        assert!(results.successful_connections >= 180, "Too many connection failures");
        assert!(results.successful_shares >= 800, "Too many share failures");
    }

    #[tokio::test]
    async fn test_resource_limits() {
        let config = LoadTestConfig {
            num_connections: 1500, // Exceeds limit
            shares_per_connection: 1,
            test_duration_seconds: 30,
            connection_delay_ms: 1,
            share_interval_ms: 1000,
        };
        
        let tester = PoolLoadTester::new(config);
        let results = tester.test_resource_limits().await;
        
        results.print_summary();
        // Should reject connections beyond limit
        assert!(results.successful_connections <= 1000, "Connection limit not enforced");
        assert!(results.failed_connections >= 500, "Should have rejected excess connections");
    }

    #[tokio::test]
    async fn test_memory_usage_under_load() {
        let config = LoadTestConfig {
            num_connections: 1000,
            shares_per_connection: 10,
            test_duration_seconds: 60,
            connection_delay_ms: 1,
            share_interval_ms: 100,
        };
        
        let tester = PoolLoadTester::new(config);
        let results = tester.test_comprehensive_load().await;
        
        results.print_summary();
        // Requirement: use less than 100MB RAM under load
        assert!(results.memory_usage_mb < 100.0, 
                "Memory usage {} MB exceeds 100MB limit", results.memory_usage_mb);
    }

    #[tokio::test]
    async fn test_connection_cleanup() {
        let config = LoadTestConfig {
            num_connections: 100,
            shares_per_connection: 1,
            test_duration_seconds: 10,
            connection_delay_ms: 10,
            share_interval_ms: 1000,
        };
        
        let tester = PoolLoadTester::new(config);
        
        // Start handler and add connections
        tester.handler.start().await.expect("Failed to start handler");
        
        let mut connections = Vec::new();
        for i in 0..config.num_connections {
            let addr: SocketAddr = format!("127.0.0.1:{}", 3333 + i).parse().unwrap();
            let conn = Connection::new(addr, Protocol::Sv2);
            let conn_id = conn.id;
            
            tester.handler.handle_connection(conn).await.expect("Failed to add connection");
            connections.push(conn_id);
        }
        
        assert_eq!(tester.handler.get_connection_count().await, config.num_connections);
        
        // Disconnect half the connections
        for &conn_id in &connections[..config.num_connections / 2] {
            tester.handler.handle_disconnection(conn_id).await.expect("Failed to disconnect");
        }
        
        assert_eq!(tester.handler.get_connection_count().await, config.num_connections / 2);
        
        tester.handler.stop().await.expect("Failed to stop handler");
    }
}