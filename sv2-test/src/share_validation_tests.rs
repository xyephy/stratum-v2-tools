use sv2_core::{
    Share, ShareResult, WorkTemplate, ShareValidator, ShareValidatorConfig, ShareValidationError,
    database::{DatabaseOps, MockDatabaseOps, ShareStats},
    types::{ShareSubmission, ConnectionId, Protocol},
    Connection,
};
use std::sync::Arc;
use std::net::SocketAddr;
use bitcoin::{BlockHash, Transaction};
use std::str::FromStr;
use uuid::Uuid;

/// Test utilities for share validation
pub struct ShareValidationTestUtils;

impl ShareValidationTestUtils {
    /// Create a test work template
    pub fn create_test_template() -> WorkTemplate {
        let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        
        WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0)
    }

    /// Create a test share
    pub fn create_test_share(connection_id: ConnectionId, nonce: u32, difficulty: f64) -> Share {
        Share::new(connection_id, nonce, chrono::Utc::now().timestamp() as u32, difficulty)
    }

    /// Create a test share submission
    pub fn create_test_submission(
        connection_id: ConnectionId,
        job_id: String,
        nonce: u32,
        difficulty: f64,
        worker_name: String,
    ) -> ShareSubmission {
        ShareSubmission::new(
            connection_id,
            job_id,
            "abcd".to_string(), // extranonce2
            chrono::Utc::now().timestamp() as u32,
            nonce,
            worker_name,
            difficulty,
        )
    }

    /// Create expired template
    pub fn create_expired_template() -> WorkTemplate {
        let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        
        let mut template = WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0);
        // Set expiry to past
        template.expires_at = chrono::Utc::now() - chrono::Duration::minutes(10);
        template
    }
}

/// Comprehensive share validation tests
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_basic_share_validation() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let connection_id = Uuid::new_v4();
        let share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        
        // Valid share should pass
        let result = validator.validate_basic_share_data(&share);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_invalid_difficulty_validation() {
        let config = ShareValidatorConfig {
            min_difficulty: 1.0,
            max_difficulty: 1000.0,
            ..Default::default()
        };
        let validator = ShareValidator::new(config);
        
        let connection_id = Uuid::new_v4();
        
        // Test below minimum difficulty
        let low_diff_share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 0.5);
        let result = validator.validate_basic_share_data(&low_diff_share);
        assert!(result.is_err());
        
        // Test above maximum difficulty
        let high_diff_share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 2000.0);
        let result = validator.validate_basic_share_data(&high_diff_share);
        assert!(result.is_err());
        
        // Test valid difficulty
        let valid_share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 100.0);
        let result = validator.validate_basic_share_data(&valid_share);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_timestamp_validation() {
        let config = ShareValidatorConfig {
            max_share_age_seconds: 300, // 5 minutes
            ..Default::default()
        };
        let validator = ShareValidator::new(config);
        
        let connection_id = Uuid::new_v4();
        let now = chrono::Utc::now().timestamp() as u32;
        
        // Test old timestamp
        let mut old_share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        old_share.timestamp = now - 600; // 10 minutes ago
        let result = validator.validate_basic_share_data(&old_share);
        assert!(result.is_err());
        
        // Test future timestamp
        let mut future_share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        future_share.timestamp = now + 600; // 10 minutes in future
        let result = validator.validate_basic_share_data(&future_share);
        assert!(result.is_err());
        
        // Test valid timestamp
        let valid_share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        let result = validator.validate_basic_share_data(&valid_share);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_nonce_validation() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let connection_id = Uuid::new_v4();
        
        // Test zero nonce (invalid)
        let zero_nonce_share = ShareValidationTestUtils::create_test_share(connection_id, 0, 1.0);
        let result = validator.validate_basic_share_data(&zero_nonce_share);
        assert!(result.is_err());
        
        // Test valid nonce
        let valid_share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        let result = validator.validate_basic_share_data(&valid_share);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_template_management() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let template = ShareValidationTestUtils::create_test_template();
        let template_id = template.id;
        
        // Add template
        validator.add_template(template).await;
        
        // Should be able to retrieve template
        let job_id = template_id.to_string();
        let retrieved = validator.get_template(&job_id).await;
        assert!(retrieved.is_ok());
        assert_eq!(retrieved.unwrap().id, template_id);
        
        // Test non-existent template
        let fake_id = Uuid::new_v4().to_string();
        let result = validator.get_template(&fake_id).await;
        assert!(result.is_err());
        
        // Test invalid job ID format
        let result = validator.get_template("invalid-format").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_expired_template_validation() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let expired_template = ShareValidationTestUtils::create_expired_template();
        let connection_id = Uuid::new_v4();
        let share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        
        // Should fail validation against expired template
        let result = validator.validate_against_template(&share, &expired_template);
        assert!(result.is_err());
        
        // Valid template should pass
        let valid_template = ShareValidationTestUtils::create_test_template();
        let result = validator.validate_against_template(&share, &valid_template);
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_duplicate_share_detection() {
        let config = ShareValidatorConfig {
            enable_duplicate_detection: true,
            duplicate_window_seconds: 3600,
            ..Default::default()
        };
        let validator = ShareValidator::new(config);
        
        let connection_id = Uuid::new_v4();
        let share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        
        // First check should pass
        let result = validator.check_duplicate_share(&share).await;
        assert!(result.is_ok());
        
        // Record the share
        validator.record_share(&share).await;
        
        // Second check should fail (duplicate)
        let result = validator.check_duplicate_share(&share).await;
        assert!(result.is_err());
        
        // Different nonce should pass
        let different_share = ShareValidationTestUtils::create_test_share(connection_id, 54321, 1.0);
        let result = validator.check_duplicate_share(&different_share).await;
        assert!(result.is_ok());
        
        // Different connection should pass even with same nonce
        let different_connection = Uuid::new_v4();
        let different_conn_share = ShareValidationTestUtils::create_test_share(different_connection, 12345, 1.0);
        let result = validator.check_duplicate_share(&different_conn_share).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_duplicate_cleanup() {
        let config = ShareValidatorConfig {
            enable_duplicate_detection: true,
            duplicate_window_seconds: 1, // Very short window for testing
            ..Default::default()
        };
        let validator = ShareValidator::new(config);
        
        let connection_id = Uuid::new_v4();
        let share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        
        // Record share
        validator.record_share(&share).await;
        
        // Should be tracked
        let stats = validator.get_stats().await;
        assert_eq!(stats.recent_shares_tracked, 1);
        
        // Wait for cleanup window
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        // Cleanup
        validator.cleanup_old_shares().await;
        
        // Should be cleaned up
        let stats = validator.get_stats().await;
        assert_eq!(stats.recent_shares_tracked, 0);
        
        // Should now pass duplicate check
        let result = validator.check_duplicate_share(&share).await;
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn test_difficulty_to_target_conversion() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        // Valid difficulties
        let target1 = validator.difficulty_to_target(1.0);
        assert!(target1.is_ok());
        
        let target2 = validator.difficulty_to_target(100.0);
        assert!(target2.is_ok());
        
        // Invalid difficulties
        let invalid1 = validator.difficulty_to_target(0.0);
        assert!(invalid1.is_err());
        
        let invalid2 = validator.difficulty_to_target(-1.0);
        assert!(invalid2.is_err());
    }

    #[tokio::test]
    async fn test_block_header_construction() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let template = ShareValidationTestUtils::create_test_template();
        let connection_id = Uuid::new_v4();
        let share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        
        let header = validator.build_block_header(&share, &template, "abcd");
        assert!(header.is_ok());
        
        let header_bytes = header.unwrap();
        assert_eq!(header_bytes.len(), 80); // Standard Bitcoin block header size
        
        // Test with different extranonce2
        let header2 = validator.build_block_header(&share, &template, "efgh");
        assert!(header2.is_ok());
        
        // Headers should be different
        assert_ne!(header_bytes, header2.unwrap());
    }

    #[tokio::test]
    async fn test_hash_calculation() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let test_data = b"test block header data for hashing";
        let hash = validator.calculate_block_hash(test_data);
        assert!(hash.is_ok());
        
        let hash_bytes = hash.unwrap();
        assert_eq!(hash_bytes.len(), 32);
        
        // Same input should produce same hash
        let hash2 = validator.calculate_block_hash(test_data);
        assert_eq!(hash_bytes, hash2.unwrap());
        
        // Different input should produce different hash
        let different_data = b"different test data";
        let hash3 = validator.calculate_block_hash(different_data);
        assert_ne!(hash_bytes, hash3.unwrap());
    }

    #[tokio::test]
    async fn test_merkle_root_calculation() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let template = ShareValidationTestUtils::create_test_template();
        
        let merkle1 = validator.calculate_merkle_root(&template, "abcd");
        assert!(merkle1.is_ok());
        
        let merkle2 = validator.calculate_merkle_root(&template, "efgh");
        assert!(merkle2.is_ok());
        
        // Different extranonce2 should produce different merkle root
        assert_ne!(merkle1.unwrap(), merkle2.unwrap());
        
        // Same extranonce2 should produce same merkle root
        let merkle3 = validator.calculate_merkle_root(&template, "abcd");
        assert_eq!(merkle1.unwrap(), merkle3.unwrap());
    }

    #[tokio::test]
    async fn test_full_share_validation() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        // Add a template
        let template = ShareValidationTestUtils::create_test_template();
        let job_id = template.id.to_string();
        validator.add_template(template).await;
        
        // Create a valid submission
        let connection_id = Uuid::new_v4();
        let submission = ShareValidationTestUtils::create_test_submission(
            connection_id,
            job_id,
            12345,
            1.0,
            "worker1".to_string(),
        );
        
        // Should validate successfully
        let result = validator.validate_share(&submission).await;
        assert!(result.is_ok());
        
        // Result should be Valid or Invalid (depending on proof of work)
        let share_result = result.unwrap();
        assert!(matches!(share_result, ShareResult::Valid | ShareResult::Invalid(_)));
    }

    #[tokio::test]
    async fn test_validator_statistics() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config.clone());
        
        // Initial stats
        let stats = validator.get_stats().await;
        assert_eq!(stats.active_templates, 0);
        assert_eq!(stats.recent_shares_tracked, 0);
        assert_eq!(stats.min_difficulty, config.min_difficulty);
        assert_eq!(stats.max_difficulty, config.max_difficulty);
        
        // Add template and share
        let template = ShareValidationTestUtils::create_test_template();
        validator.add_template(template).await;
        
        let connection_id = Uuid::new_v4();
        let share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        validator.record_share(&share).await;
        
        // Updated stats
        let stats = validator.get_stats().await;
        assert_eq!(stats.active_templates, 1);
        assert_eq!(stats.recent_shares_tracked, 1);
    }

    #[tokio::test]
    async fn test_template_cleanup() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        // Add valid template
        let valid_template = ShareValidationTestUtils::create_test_template();
        validator.add_template(valid_template).await;
        
        // Add expired template
        let expired_template = ShareValidationTestUtils::create_expired_template();
        validator.add_template(expired_template).await;
        
        // Should have 2 templates
        let stats = validator.get_stats().await;
        assert_eq!(stats.active_templates, 2);
        
        // Cleanup expired templates
        validator.cleanup_expired_templates().await;
        
        // Should have 1 template left
        let stats = validator.get_stats().await;
        assert_eq!(stats.active_templates, 1);
    }

    #[tokio::test]
    async fn test_error_handling() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let connection_id = Uuid::new_v4();
        
        // Test with non-existent template
        let submission = ShareValidationTestUtils::create_test_submission(
            connection_id,
            Uuid::new_v4().to_string(),
            12345,
            1.0,
            "worker1".to_string(),
        );
        
        let result = validator.validate_share(&submission).await;
        assert!(result.is_err());
        
        // Test with invalid job ID format
        let invalid_submission = ShareValidationTestUtils::create_test_submission(
            connection_id,
            "invalid-job-id".to_string(),
            12345,
            1.0,
            "worker1".to_string(),
        );
        
        let result = validator.validate_share(&invalid_submission).await;
        assert!(result.is_err());
    }
}

/// Database operations tests for share tracking
#[cfg(test)]
mod database_tests {
    use super::*;

    #[tokio::test]
    async fn test_share_storage_and_retrieval() {
        let database = Arc::new(MockDatabaseOps::new());
        
        let connection_id = Uuid::new_v4();
        let share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        
        // Store share
        let result = database.create_share(&share).await;
        assert!(result.is_ok());
        
        // Retrieve shares
        let shares = database.get_shares(Some(connection_id), Some(10)).await;
        assert!(shares.is_ok());
        
        let retrieved_shares = shares.unwrap();
        assert_eq!(retrieved_shares.len(), 1);
        assert_eq!(retrieved_shares[0].connection_id, connection_id);
        assert_eq!(retrieved_shares[0].nonce, 12345);
    }

    #[tokio::test]
    async fn test_share_statistics() {
        let database = Arc::new(MockDatabaseOps::new());
        
        let connection_id = Uuid::new_v4();
        
        // Add valid shares
        for i in 0..10 {
            let mut share = ShareValidationTestUtils::create_test_share(connection_id, i, 1.0);
            share.is_valid = true;
            database.create_share(&share).await.unwrap();
        }
        
        // Add invalid shares
        for i in 10..15 {
            let mut share = ShareValidationTestUtils::create_test_share(connection_id, i, 1.0);
            share.is_valid = false;
            database.create_share(&share).await.unwrap();
        }
        
        // Get statistics
        let stats = database.get_share_stats(Some(connection_id)).await;
        assert!(stats.is_ok());
        
        let share_stats = stats.unwrap();
        assert_eq!(share_stats.total_shares, 15);
        assert_eq!(share_stats.valid_shares, 10);
        assert_eq!(share_stats.invalid_shares, 5);
        assert!((share_stats.acceptance_rate - 66.66666666666667).abs() < 0.0001);
    }

    #[tokio::test]
    async fn test_share_filtering_by_connection() {
        let database = Arc::new(MockDatabaseOps::new());
        
        let connection1 = Uuid::new_v4();
        let connection2 = Uuid::new_v4();
        
        // Add shares for connection 1
        for i in 0..5 {
            let share = ShareValidationTestUtils::create_test_share(connection1, i, 1.0);
            database.create_share(&share).await.unwrap();
        }
        
        // Add shares for connection 2
        for i in 0..3 {
            let share = ShareValidationTestUtils::create_test_share(connection2, i + 100, 1.0);
            database.create_share(&share).await.unwrap();
        }
        
        // Get shares for connection 1
        let shares1 = database.get_shares(Some(connection1), None).await.unwrap();
        assert_eq!(shares1.len(), 5);
        assert!(shares1.iter().all(|s| s.connection_id == connection1));
        
        // Get shares for connection 2
        let shares2 = database.get_shares(Some(connection2), None).await.unwrap();
        assert_eq!(shares2.len(), 3);
        assert!(shares2.iter().all(|s| s.connection_id == connection2));
        
        // Get all shares
        let all_shares = database.get_shares(None, None).await.unwrap();
        assert_eq!(all_shares.len(), 8);
    }

    #[tokio::test]
    async fn test_share_limit() {
        let database = Arc::new(MockDatabaseOps::new());
        
        let connection_id = Uuid::new_v4();
        
        // Add many shares
        for i in 0..20 {
            let share = ShareValidationTestUtils::create_test_share(connection_id, i, 1.0);
            database.create_share(&share).await.unwrap();
        }
        
        // Get limited shares
        let limited_shares = database.get_shares(Some(connection_id), Some(5)).await.unwrap();
        assert_eq!(limited_shares.len(), 5);
        
        // Get all shares
        let all_shares = database.get_shares(Some(connection_id), None).await.unwrap();
        assert_eq!(all_shares.len(), 20);
    }

    #[tokio::test]
    async fn test_block_detection_in_shares() {
        let database = Arc::new(MockDatabaseOps::new());
        
        let connection_id = Uuid::new_v4();
        let block_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000001").unwrap();
        
        // Add regular share
        let regular_share = ShareValidationTestUtils::create_test_share(connection_id, 12345, 1.0);
        database.create_share(&regular_share).await.unwrap();
        
        // Add block share
        let mut block_share = ShareValidationTestUtils::create_test_share(connection_id, 54321, 1.0);
        block_share.is_valid = true;
        block_share.block_hash = Some(block_hash);
        database.create_share(&block_share).await.unwrap();
        
        // Get statistics
        let stats = database.get_share_stats(Some(connection_id)).await.unwrap();
        assert_eq!(stats.total_shares, 2);
        assert_eq!(stats.blocks_found, 1);
        
        // Retrieve shares and verify block hash
        let shares = database.get_shares(Some(connection_id), None).await.unwrap();
        let block_shares: Vec<_> = shares.iter().filter(|s| s.block_hash.is_some()).collect();
        assert_eq!(block_shares.len(), 1);
        assert_eq!(block_shares[0].block_hash.unwrap(), block_hash);
    }

    #[tokio::test]
    async fn test_share_timestamps() {
        let database = Arc::new(MockDatabaseOps::new());
        
        let connection_id = Uuid::new_v4();
        
        // Add shares with different timestamps
        let now = chrono::Utc::now();
        let mut shares = Vec::new();
        
        for i in 0..5 {
            let mut share = ShareValidationTestUtils::create_test_share(connection_id, i, 1.0);
            share.submitted_at = now - chrono::Duration::minutes(i as i64);
            shares.push(share.clone());
            database.create_share(&share).await.unwrap();
        }
        
        // Get statistics
        let stats = database.get_share_stats(Some(connection_id)).await.unwrap();
        assert!(stats.first_share.is_some());
        assert!(stats.last_share.is_some());
        
        // First share should be the oldest
        let first_share_time = stats.first_share.unwrap();
        let last_share_time = stats.last_share.unwrap();
        assert!(first_share_time <= last_share_time);
    }
}