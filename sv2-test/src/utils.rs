use sv2_core::{Connection, Share, WorkTemplate, Protocol, ConnectionId};
use std::net::SocketAddr;
use bitcoin::{BlockHash, Transaction};
use bitcoin::absolute::LockTime;

/// Test utilities for creating mock data
pub struct TestUtils;

impl TestUtils {
    /// Create a mock connection
    pub fn mock_connection() -> Connection {
        Connection::new(
            "127.0.0.1:3333".parse::<SocketAddr>().unwrap(),
            Protocol::Sv2,
        )
    }

    /// Create a mock share
    pub fn mock_share(connection_id: ConnectionId) -> Share {
        Share::new(connection_id, 12345, 1640995200, 1.0)
    }

    /// Create a mock work template
    pub fn mock_work_template() -> WorkTemplate {
        WorkTemplate::new(
            "0000000000000000000000000000000000000000000000000000000000000000".parse().unwrap(),
            Transaction {
                version: 1,
                lock_time: LockTime::ZERO,
                input: vec![],
                output: vec![],
            },
            vec![],
            1.0,
        )
    }

    /// Generate test configuration
    pub fn test_config() -> sv2_core::config::DaemonConfig {
        let mut config = sv2_core::config::DaemonConfig::default();
        config.database.url = "sqlite::memory:".to_string();
        config.network.bind_address = "127.0.0.1:0".parse().unwrap();
        config
    }
}