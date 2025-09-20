use crate::{Result, Error, Share, ShareResult, WorkTemplate, types::ShareSubmission};
use bitcoin::{BlockHash, Target, CompactTarget};
use std::str::FromStr;
use sha2::{Sha256, Digest};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Share validation configuration
#[derive(Debug, Clone)]
pub struct ShareValidatorConfig {
    pub min_difficulty: f64,
    pub max_difficulty: f64,
    pub max_share_age_seconds: u64,
    pub enable_duplicate_detection: bool,
    pub duplicate_window_seconds: u64,
    pub enable_block_detection: bool,
    pub network_target: Target,
}

impl Default for ShareValidatorConfig {
    fn default() -> Self {
        Self {
            min_difficulty: 0.001,
            max_difficulty: 1000000.0,
            max_share_age_seconds: 300, // 5 minutes
            enable_duplicate_detection: true,
            duplicate_window_seconds: 3600, // 1 hour
            enable_block_detection: true,
            network_target: Target::MAX, // Simplified
        }
    }
}

/// Share validation error types
#[derive(Debug, Clone, PartialEq)]
pub enum ShareValidationError {
    InvalidDifficulty(String),
    InvalidTimestamp(String),
    InvalidNonce(String),
    InvalidTarget(String),
    DuplicateShare(String),
    ExpiredTemplate(String),
    InsufficientWork(String),
    MalformedData(String),
    TemplateNotFound(String),
}

impl std::fmt::Display for ShareValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShareValidationError::InvalidDifficulty(msg) => write!(f, "Invalid difficulty: {}", msg),
            ShareValidationError::InvalidTimestamp(msg) => write!(f, "Invalid timestamp: {}", msg),
            ShareValidationError::InvalidNonce(msg) => write!(f, "Invalid nonce: {}", msg),
            ShareValidationError::InvalidTarget(msg) => write!(f, "Invalid target: {}", msg),
            ShareValidationError::DuplicateShare(msg) => write!(f, "Duplicate share: {}", msg),
            ShareValidationError::ExpiredTemplate(msg) => write!(f, "Expired template: {}", msg),
            ShareValidationError::InsufficientWork(msg) => write!(f, "Insufficient work: {}", msg),
            ShareValidationError::MalformedData(msg) => write!(f, "Malformed data: {}", msg),
            ShareValidationError::TemplateNotFound(msg) => write!(f, "Template not found: {}", msg),
        }
    }
}

impl std::error::Error for ShareValidationError {}

/// Share hash for duplicate detection
#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct ShareHash {
    connection_id: uuid::Uuid,
    nonce: u32,
    timestamp: u32,
}

/// Share validator with comprehensive validation logic
pub struct ShareValidator {
    config: ShareValidatorConfig,
    recent_shares: Arc<RwLock<HashMap<ShareHash, chrono::DateTime<chrono::Utc>>>>,
    templates: Arc<RwLock<HashMap<uuid::Uuid, WorkTemplate>>>,
}

impl ShareValidator {
    /// Create a new share validator
    pub fn new(config: ShareValidatorConfig) -> Self {
        Self {
            config,
            recent_shares: Arc::new(RwLock::new(HashMap::new())),
            templates: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Add a work template for validation
    pub async fn add_template(&self, template: WorkTemplate) {
        let mut templates = self.templates.write().await;
        templates.insert(template.id, template);
    }

    /// Remove expired templates
    pub async fn cleanup_expired_templates(&self) {
        let mut templates = self.templates.write().await;
        let now = chrono::Utc::now();
        templates.retain(|_, template| template.expires_at > now);
    }

    /// Validate a share submission
    pub async fn validate_share(&self, submission: &ShareSubmission) -> Result<ShareResult> {
        // Basic validation
        self.validate_basic_share_data(&submission.share)?;
        
        // Get the work template
        let template = self.get_template(&submission.job_id).await?;
        
        // Validate against template
        self.validate_against_template(&submission.share, &template)?;
        
        // Check for duplicates
        if self.config.enable_duplicate_detection {
            self.check_duplicate_share(&submission.share).await?;
        }
        
        // Validate proof of work
        let work_result = self.validate_proof_of_work(&submission.share, &template, &submission.extranonce2)?;
        
        // Record share for duplicate detection
        if self.config.enable_duplicate_detection {
            self.record_share(&submission.share).await;
        }
        
        // Clean up old shares periodically
        self.cleanup_old_shares().await;
        
        Ok(work_result)
    }

    /// Validate basic share data
    fn validate_basic_share_data(&self, share: &Share) -> Result<()> {
        // Validate difficulty
        if share.difficulty < self.config.min_difficulty {
            return Err(Error::ShareValidation(ShareValidationError::InvalidDifficulty(
                format!("Difficulty {} below minimum {}", share.difficulty, self.config.min_difficulty)
            )));
        }
        
        if share.difficulty > self.config.max_difficulty {
            return Err(Error::ShareValidation(ShareValidationError::InvalidDifficulty(
                format!("Difficulty {} above maximum {}", share.difficulty, self.config.max_difficulty)
            )));
        }
        
        // Validate timestamp
        let now = chrono::Utc::now().timestamp() as u32;
        let max_age = self.config.max_share_age_seconds as u32;
        
        if share.timestamp < now.saturating_sub(max_age) {
            return Err(Error::ShareValidation(ShareValidationError::InvalidTimestamp(
                format!("Share timestamp {} too old", share.timestamp)
            )));
        }
        
        if share.timestamp > now + 300 { // 5 minutes in future
            return Err(Error::ShareValidation(ShareValidationError::InvalidTimestamp(
                format!("Share timestamp {} too far in future", share.timestamp)
            )));
        }
        
        // Validate nonce (basic check)
        if share.nonce == 0 {
            return Err(Error::ShareValidation(ShareValidationError::InvalidNonce(
                "Nonce cannot be zero".to_string()
            )));
        }
        
        Ok(())
    }

    /// Get work template by job ID
    async fn get_template(&self, job_id: &str) -> Result<WorkTemplate> {
        // Parse job ID to template ID (simplified)
        let template_id = uuid::Uuid::parse_str(job_id)
            .map_err(|_| Error::ShareValidation(ShareValidationError::MalformedData(
                format!("Invalid job ID format: {}", job_id)
            )))?;
        
        let templates = self.templates.read().await;
        templates.get(&template_id).cloned()
            .ok_or_else(|| Error::ShareValidation(ShareValidationError::TemplateNotFound(
                format!("Template not found for job ID: {}", job_id)
            )))
    }

    /// Validate share against work template
    fn validate_against_template(&self, share: &Share, template: &WorkTemplate) -> Result<()> {
        // Check if template is expired
        if template.is_expired() {
            return Err(Error::ShareValidation(ShareValidationError::ExpiredTemplate(
                format!("Template {} expired at {}", template.id, template.expires_at)
            )));
        }
        
        // Validate timestamp is not before template creation
        if share.timestamp < template.timestamp {
            return Err(Error::ShareValidation(ShareValidationError::InvalidTimestamp(
                format!("Share timestamp {} before template timestamp {}", 
                        share.timestamp, template.timestamp)
            )));
        }
        
        Ok(())
    }

    /// Check for duplicate shares
    async fn check_duplicate_share(&self, share: &Share) -> Result<()> {
        let share_hash = ShareHash {
            connection_id: share.connection_id,
            nonce: share.nonce,
            timestamp: share.timestamp,
        };
        
        let recent_shares = self.recent_shares.read().await;
        if recent_shares.contains_key(&share_hash) {
            return Err(Error::ShareValidation(ShareValidationError::DuplicateShare(
                format!("Duplicate share detected: nonce {} from connection {}", 
                        share.nonce, share.connection_id)
            )));
        }
        
        Ok(())
    }

    /// Record share for duplicate detection
    async fn record_share(&self, share: &Share) {
        let share_hash = ShareHash {
            connection_id: share.connection_id,
            nonce: share.nonce,
            timestamp: share.timestamp,
        };
        
        let mut recent_shares = self.recent_shares.write().await;
        recent_shares.insert(share_hash, chrono::Utc::now());
    }

    /// Clean up old shares from duplicate detection
    async fn cleanup_old_shares(&self) {
        let mut recent_shares = self.recent_shares.write().await;
        let cutoff = chrono::Utc::now() - chrono::Duration::seconds(self.config.duplicate_window_seconds as i64);
        
        recent_shares.retain(|_, timestamp| *timestamp > cutoff);
    }

    /// Validate proof of work
    fn validate_proof_of_work(
        &self, 
        share: &Share, 
        template: &WorkTemplate, 
        extranonce2: &str
    ) -> Result<ShareResult> {
        // Calculate target from difficulty
        let target = self.difficulty_to_target(share.difficulty)?;
        
        // Build block header for hashing
        let block_header = self.build_block_header(share, template, extranonce2)?;
        
        // Calculate hash
        let hash = self.calculate_block_hash(&block_header)?;
        
        // Check if hash meets share difficulty
        if !self.hash_meets_target(&hash, &target) {
            return Ok(ShareResult::Invalid("Hash does not meet target difficulty".to_string()));
        }
        
        // Check if it's a block (meets network difficulty)
        if self.config.enable_block_detection && self.hash_meets_target(&hash, &self.config.network_target) {
            // Create a simplified block hash from the hash bytes
            let block_hash = BlockHash::from_str(&hex::encode(&hash))
                .map_err(|e| Error::ShareValidation(ShareValidationError::MalformedData(
                    format!("Invalid block hash: {}", e)
                )))?;
            return Ok(ShareResult::Block(block_hash));
        }
        
        Ok(ShareResult::Valid)
    }

    /// Convert difficulty to target
    fn difficulty_to_target(&self, difficulty: f64) -> Result<Target> {
        if difficulty <= 0.0 {
            return Err(Error::ShareValidation(ShareValidationError::InvalidDifficulty(
                "Difficulty must be positive".to_string()
            )));
        }
        
        // Simplified target calculation
        // In reality, this would use proper Bitcoin target calculation
        let max_target_value = 0x1d00ffff_u32; // Bitcoin's max target in compact form
        let target_value = (max_target_value as f64 / difficulty) as u32;
        
        // Create a simplified target (this is not the real Bitcoin target calculation)
        let compact_target = CompactTarget::from_consensus(target_value);
        Ok(Target::from_compact(compact_target))
    }

    /// Build block header for hashing
    fn build_block_header(&self, share: &Share, template: &WorkTemplate, extranonce2: &str) -> Result<Vec<u8>> {
        // Simplified block header construction
        // In reality, this would build a proper Bitcoin block header
        let mut header = Vec::new();
        
        // Version (4 bytes)
        header.extend_from_slice(&1u32.to_le_bytes());
        
        // Previous block hash (32 bytes) - simplified approach
        let hash_bytes = template.previous_hash.to_string();
        let hash_decoded = hex::decode(&hash_bytes).unwrap_or_else(|_| vec![0u8; 32]);
        header.extend_from_slice(&hash_decoded[..32.min(hash_decoded.len())]);
        
        // Merkle root (32 bytes) - simplified
        let merkle_root = self.calculate_merkle_root(template, extranonce2)?;
        header.extend_from_slice(&merkle_root);
        
        // Timestamp (4 bytes)
        header.extend_from_slice(&share.timestamp.to_le_bytes());
        
        // Bits (4 bytes) - difficulty target
        header.extend_from_slice(&0x207fffffu32.to_le_bytes());
        
        // Nonce (4 bytes)
        header.extend_from_slice(&share.nonce.to_le_bytes());
        
        Ok(header)
    }

    /// Calculate merkle root (simplified)
    fn calculate_merkle_root(&self, template: &WorkTemplate, extranonce2: &str) -> Result<[u8; 32]> {
        // Simplified merkle root calculation
        // In reality, this would properly calculate the merkle root with coinbase transaction
        let mut hasher = Sha256::new();
        
        // Hash coinbase transaction
        let coinbase_bytes = bitcoin::consensus::encode::serialize(&template.coinbase_tx);
        hasher.update(&coinbase_bytes);
        hasher.update(extranonce2.as_bytes());
        
        // Hash other transactions
        for tx in &template.transactions {
            let tx_bytes = bitcoin::consensus::encode::serialize(tx);
            hasher.update(&tx_bytes);
        }
        
        Ok(hasher.finalize().into())
    }

    /// Calculate block hash
    fn calculate_block_hash(&self, header: &[u8]) -> Result<[u8; 32]> {
        // Double SHA256 hash
        let mut hasher = Sha256::new();
        hasher.update(header);
        let first_hash = hasher.finalize();
        
        let mut hasher = Sha256::new();
        hasher.update(&first_hash);
        Ok(hasher.finalize().into())
    }

    /// Check if hash meets target
    fn hash_meets_target(&self, hash: &[u8; 32], target: &Target) -> bool {
        // Convert hash to big-endian for comparison
        let mut hash_be = *hash;
        hash_be.reverse();
        
        let compact_value = u32::from_be_bytes([hash_be[0], hash_be[1], hash_be[2], hash_be[3]]);
        let compact_target = CompactTarget::from_consensus(compact_value);
        let hash_target = Target::from_compact(compact_target);
        hash_target <= *target
    }

    /// Get validation statistics
    pub async fn get_stats(&self) -> ShareValidatorStats {
        let recent_shares = self.recent_shares.read().await;
        let templates = self.templates.read().await;
        
        ShareValidatorStats {
            active_templates: templates.len(),
            recent_shares_tracked: recent_shares.len(),
            duplicate_window_seconds: self.config.duplicate_window_seconds,
            min_difficulty: self.config.min_difficulty,
            max_difficulty: self.config.max_difficulty,
        }
    }
}

/// Share validator statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShareValidatorStats {
    pub active_templates: usize,
    pub recent_shares_tracked: usize,
    pub duplicate_window_seconds: u64,
    pub min_difficulty: f64,
    pub max_difficulty: f64,
}

// Add ShareValidation error to the main Error enum
impl From<ShareValidationError> for Error {
    fn from(err: ShareValidationError) -> Self {
        Error::ShareValidation(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Share, WorkTemplate};
    use bitcoin::{BlockHash, Transaction};
    use std::str::FromStr;

    fn create_test_template() -> WorkTemplate {
        let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        
        WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0)
    }

    fn create_test_share(connection_id: uuid::Uuid, nonce: u32) -> Share {
        Share::new(connection_id, nonce, chrono::Utc::now().timestamp() as u32, 1.0)
    }

    #[tokio::test]
    async fn test_basic_share_validation() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let connection_id = uuid::Uuid::new_v4();
        let share = create_test_share(connection_id, 12345);
        
        // Should pass basic validation
        assert!(validator.validate_basic_share_data(&share).is_ok());
        
        // Test invalid difficulty
        let mut invalid_share = share.clone();
        invalid_share.difficulty = -1.0;
        assert!(validator.validate_basic_share_data(&invalid_share).is_err());
        
        // Test invalid timestamp
        let mut invalid_share = share.clone();
        invalid_share.timestamp = 0;
        assert!(validator.validate_basic_share_data(&invalid_share).is_err());
    }

    #[tokio::test]
    async fn test_template_management() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let template = create_test_template();
        let template_id = template.id;
        
        // Add template
        validator.add_template(template).await;
        
        // Should be able to get template
        let job_id = template_id.to_string();
        let retrieved = validator.get_template(&job_id).await;
        assert!(retrieved.is_ok());
        assert_eq!(retrieved.unwrap().id, template_id);
        
        // Test invalid job ID
        let invalid_result = validator.get_template("invalid-job-id").await;
        assert!(invalid_result.is_err());
    }

    #[tokio::test]
    async fn test_duplicate_detection() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let connection_id = uuid::Uuid::new_v4();
        let share = create_test_share(connection_id, 12345);
        
        // First submission should pass
        assert!(validator.check_duplicate_share(&share).await.is_ok());
        
        // Record the share
        validator.record_share(&share).await;
        
        // Second submission should fail
        assert!(validator.check_duplicate_share(&share).await.is_err());
        
        // Different nonce should pass
        let different_share = create_test_share(connection_id, 54321);
        assert!(validator.check_duplicate_share(&different_share).await.is_ok());
    }

    #[tokio::test]
    async fn test_difficulty_to_target() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        // Valid difficulty
        let target = validator.difficulty_to_target(1.0);
        assert!(target.is_ok());
        
        // Invalid difficulty
        let invalid_target = validator.difficulty_to_target(-1.0);
        assert!(invalid_target.is_err());
        
        let zero_target = validator.difficulty_to_target(0.0);
        assert!(zero_target.is_err());
    }

    #[tokio::test]
    async fn test_block_header_construction() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let template = create_test_template();
        let connection_id = uuid::Uuid::new_v4();
        let share = create_test_share(connection_id, 12345);
        
        let header = validator.build_block_header(&share, &template, "abcd");
        assert!(header.is_ok());
        
        let header_bytes = header.unwrap();
        assert_eq!(header_bytes.len(), 80); // Standard Bitcoin block header size
    }

    #[tokio::test]
    async fn test_hash_calculation() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config);
        
        let test_data = b"test block header data";
        let hash = validator.calculate_block_hash(test_data);
        assert!(hash.is_ok());
        
        let hash_bytes = hash.unwrap();
        assert_eq!(hash_bytes.len(), 32);
    }

    #[tokio::test]
    async fn test_cleanup_operations() {
        let config = ShareValidatorConfig {
            duplicate_window_seconds: 1, // Very short window for testing
            ..Default::default()
        };
        let validator = ShareValidator::new(config);
        
        let connection_id = uuid::Uuid::new_v4();
        let share = create_test_share(connection_id, 12345);
        
        // Record a share
        validator.record_share(&share).await;
        
        // Should have one share tracked
        let stats = validator.get_stats().await;
        assert_eq!(stats.recent_shares_tracked, 1);
        
        // Wait for cleanup window
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        // Cleanup should remove old shares
        validator.cleanup_old_shares().await;
        
        let stats = validator.get_stats().await;
        assert_eq!(stats.recent_shares_tracked, 0);
    }

    #[tokio::test]
    async fn test_validator_stats() {
        let config = ShareValidatorConfig::default();
        let validator = ShareValidator::new(config.clone());
        
        let template = create_test_template();
        validator.add_template(template).await;
        
        let connection_id = uuid::Uuid::new_v4();
        let share = create_test_share(connection_id, 12345);
        validator.record_share(&share).await;
        
        let stats = validator.get_stats().await;
        assert_eq!(stats.active_templates, 1);
        assert_eq!(stats.recent_shares_tracked, 1);
        assert_eq!(stats.min_difficulty, config.min_difficulty);
        assert_eq!(stats.max_difficulty, config.max_difficulty);
    }
}