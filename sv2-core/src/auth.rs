use crate::{Result, Error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{SystemTime, UNIX_EPOCH, Duration};
use uuid::Uuid;
use sha2::{Sha256, Digest};
use base64::{Engine as _, engine::general_purpose};

/// Authentication and authorization system for sv2d
pub struct AuthSystem {
    /// API keys and their associated permissions
    api_keys: HashMap<String, ApiKeyInfo>,
    /// Active sessions for connection-based authentication
    sessions: HashMap<String, SessionInfo>,
    /// Rate limiting state
    rate_limits: HashMap<String, RateLimitState>,
    /// Configuration
    config: AuthConfig,
}

/// API key information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyInfo {
    /// Unique identifier for the API key
    pub id: String,
    /// Hashed API key (never store plaintext)
    pub key_hash: String,
    /// Human-readable name/description
    pub name: String,
    /// Permissions granted to this key
    pub permissions: Vec<Permission>,
    /// Creation timestamp
    pub created_at: u64,
    /// Optional expiration timestamp
    pub expires_at: Option<u64>,
    /// Whether the key is active
    pub active: bool,
    /// Last used timestamp
    pub last_used: Option<u64>,
}

/// Session information for connection-based authentication
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInfo {
    /// Session ID
    pub id: String,
    /// Associated API key ID
    pub api_key_id: String,
    /// Client identifier (IP address, connection ID, etc.)
    pub client_id: String,
    /// Session creation time
    pub created_at: u64,
    /// Last activity time
    pub last_activity: u64,
    /// Session expiration time
    pub expires_at: u64,
    /// Cached permissions for performance
    pub permissions: Vec<Permission>,
}

/// Rate limiting state
#[derive(Debug, Clone)]
pub struct RateLimitState {
    /// Request count in current window
    pub request_count: u32,
    /// Window start time
    pub window_start: u64,
    /// Whether client is currently blocked
    pub blocked_until: Option<u64>,
}

/// Authentication configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthConfig {
    /// Whether authentication is enabled
    pub enabled: bool,
    /// Session timeout in seconds
    pub session_timeout: u64,
    /// Rate limit per minute per client
    pub rate_limit_per_minute: u32,
    /// Rate limit window size in seconds
    pub rate_limit_window: u64,
    /// Block duration for rate limit violations (seconds)
    pub rate_limit_block_duration: u64,
    /// Maximum number of active sessions per API key
    pub max_sessions_per_key: u32,
    /// Whether to require authentication for read-only operations
    pub require_auth_for_read: bool,
}

/// Permission types for fine-grained access control
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Permission {
    // Connection management
    ViewConnections,
    ManageConnections,
    
    // Share operations
    ViewShares,
    SubmitShares,
    
    // Work template operations
    ViewTemplates,
    CreateTemplates,
    ManageTemplates,
    
    // Configuration management
    ViewConfig,
    UpdateConfig,
    
    // Daemon control
    StartDaemon,
    StopDaemon,
    RestartDaemon,
    ReloadConfig,
    
    // Metrics and monitoring
    ViewMetrics,
    ViewHealth,
    ManageAlerts,
    
    // API access
    ApiAccess,
    AdminAccess,
    
    // Mining operations
    StartMining,
    StopMining,
    ViewMiningStats,
    
    // Database operations
    ViewDatabase,
    ManageDatabase,
}

/// Authentication result
#[derive(Debug, Clone)]
pub enum AuthResult {
    /// Authentication successful
    Success {
        session_id: String,
        permissions: Vec<Permission>,
    },
    /// Authentication failed
    Failed {
        reason: String,
    },
    /// Rate limited
    RateLimited {
        retry_after: u64,
    },
}

/// Authorization result
#[derive(Debug, Clone)]
pub enum AuthzResult {
    /// Authorization granted
    Granted,
    /// Authorization denied
    Denied {
        required_permission: Permission,
    },
    /// Session not found or expired
    SessionInvalid,
}

impl AuthSystem {
    /// Create a new authentication system
    pub fn new(config: AuthConfig) -> Self {
        Self {
            api_keys: HashMap::new(),
            sessions: HashMap::new(),
            rate_limits: HashMap::new(),
            config,
        }
    }

    /// Generate a new API key
    pub fn generate_api_key(
        &mut self,
        name: String,
        permissions: Vec<Permission>,
        expires_at: Option<u64>,
    ) -> Result<(String, String)> {
        let key_id = Uuid::new_v4().to_string();
        let api_key = self.generate_secure_key();
        let key_hash = self.hash_key(&api_key);
        
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let api_key_info = ApiKeyInfo {
            id: key_id.clone(),
            key_hash,
            name,
            permissions,
            created_at: now,
            expires_at,
            active: true,
            last_used: None,
        };

        self.api_keys.insert(key_id.clone(), api_key_info);
        Ok((key_id, api_key))
    }

    /// Authenticate using API key
    pub fn authenticate(&mut self, api_key: &str, client_id: &str) -> Result<AuthResult> {
        if !self.config.enabled {
            // If authentication is disabled, grant all permissions
            return Ok(AuthResult::Success {
                session_id: Uuid::new_v4().to_string(),
                permissions: self.get_all_permissions(),
            });
        }

        // Check rate limiting
        if let Some(retry_after) = self.check_rate_limit(client_id)? {
            return Ok(AuthResult::RateLimited { retry_after });
        }

        // Find API key by hash
        let key_hash = self.hash_key(api_key);
        let api_key_info = self.api_keys.values_mut()
            .find(|info| info.key_hash == key_hash && info.active);

        let api_key_info = match api_key_info {
            Some(info) => info,
            None => {
                self.record_rate_limit_attempt(client_id)?;
                return Ok(AuthResult::Failed {
                    reason: "Invalid API key".to_string(),
                });
            }
        };

        // Check expiration
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if let Some(expires_at) = api_key_info.expires_at {
            if now > expires_at {
                return Ok(AuthResult::Failed {
                    reason: "API key expired".to_string(),
                });
            }
        }

        // Check session limits
        let active_sessions = self.sessions.values()
            .filter(|s| s.api_key_id == api_key_info.id && s.expires_at > now)
            .count();

        if active_sessions >= self.config.max_sessions_per_key as usize {
            return Ok(AuthResult::Failed {
                reason: "Maximum sessions exceeded".to_string(),
            });
        }

        // Create session
        let session_id = Uuid::new_v4().to_string();
        let session = SessionInfo {
            id: session_id.clone(),
            api_key_id: api_key_info.id.clone(),
            client_id: client_id.to_string(),
            created_at: now,
            last_activity: now,
            expires_at: now + self.config.session_timeout,
            permissions: api_key_info.permissions.clone(),
        };

        // Update API key last used
        api_key_info.last_used = Some(now);

        self.sessions.insert(session_id.clone(), session);

        Ok(AuthResult::Success {
            session_id,
            permissions: api_key_info.permissions.clone(),
        })
    }

    /// Check authorization for a session and permission
    pub fn authorize(&mut self, session_id: &str, permission: &Permission) -> Result<AuthzResult> {
        if !self.config.enabled {
            return Ok(AuthzResult::Granted);
        }

        let session = match self.sessions.get_mut(session_id) {
            Some(session) => session,
            None => return Ok(AuthzResult::SessionInvalid),
        };

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Check session expiration
        if now > session.expires_at {
            self.sessions.remove(session_id);
            return Ok(AuthzResult::SessionInvalid);
        }

        // Update last activity
        session.last_activity = now;

        // Check permission
        if session.permissions.contains(permission) || session.permissions.contains(&Permission::AdminAccess) {
            Ok(AuthzResult::Granted)
        } else {
            Ok(AuthzResult::Denied {
                required_permission: permission.clone(),
            })
        }
    }

    /// Revoke an API key
    pub fn revoke_api_key(&mut self, key_id: &str) -> Result<()> {
        if let Some(api_key_info) = self.api_keys.get_mut(key_id) {
            api_key_info.active = false;
            
            // Remove all sessions for this API key
            self.sessions.retain(|_, session| session.api_key_id != key_id);
        }
        Ok(())
    }

    /// Invalidate a session
    pub fn invalidate_session(&mut self, session_id: &str) -> Result<()> {
        self.sessions.remove(session_id);
        Ok(())
    }

    /// Clean up expired sessions and rate limit entries
    pub fn cleanup_expired(&mut self) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Remove expired sessions
        self.sessions.retain(|_, session| session.expires_at > now);

        // Clean up rate limit entries
        self.rate_limits.retain(|_, state| {
            if let Some(blocked_until) = state.blocked_until {
                blocked_until > now
            } else {
                // Keep entries from current window
                now - state.window_start < self.config.rate_limit_window
            }
        });

        Ok(())
    }

    /// Get all active sessions
    pub fn get_active_sessions(&self) -> Vec<&SessionInfo> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        self.sessions.values()
            .filter(|session| session.expires_at > now)
            .collect()
    }

    /// Get API key information (without sensitive data)
    pub fn get_api_keys(&self) -> Vec<ApiKeyInfo> {
        self.api_keys.values()
            .map(|info| {
                let mut safe_info = info.clone();
                safe_info.key_hash = "***".to_string(); // Redact hash
                safe_info
            })
            .collect()
    }

    /// Check if authentication is required for a permission
    pub fn requires_auth(&self, permission: &Permission) -> bool {
        if !self.config.enabled {
            return false;
        }

        match permission {
            // Read-only operations
            Permission::ViewConnections |
            Permission::ViewShares |
            Permission::ViewTemplates |
            Permission::ViewConfig |
            Permission::ViewMetrics |
            Permission::ViewHealth |
            Permission::ViewMiningStats |
            Permission::ViewDatabase => self.config.require_auth_for_read,
            
            // All other operations require authentication
            _ => true,
        }
    }

    // Private helper methods

    fn generate_secure_key(&self) -> String {
        let mut key_bytes = [0u8; 32];
        getrandom::getrandom(&mut key_bytes).expect("Failed to generate random bytes");
        general_purpose::URL_SAFE_NO_PAD.encode(key_bytes)
    }

    fn hash_key(&self, key: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(key.as_bytes());
        let result = hasher.finalize();
        general_purpose::STANDARD.encode(result)
    }

    fn check_rate_limit(&self, client_id: &str) -> Result<Option<u64>> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if let Some(state) = self.rate_limits.get(client_id) {
            // Check if client is blocked
            if let Some(blocked_until) = state.blocked_until {
                if now < blocked_until {
                    return Ok(Some(blocked_until - now));
                }
            }

            // Check rate limit window
            let window_elapsed = now - state.window_start;
            if window_elapsed < self.config.rate_limit_window {
                if state.request_count >= self.config.rate_limit_per_minute {
                    return Ok(Some(self.config.rate_limit_window - window_elapsed));
                }
            }
        }

        Ok(None)
    }

    fn record_rate_limit_attempt(&mut self, client_id: &str) -> Result<()> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let state = self.rate_limits.entry(client_id.to_string())
            .or_insert_with(|| RateLimitState {
                request_count: 0,
                window_start: now,
                blocked_until: None,
            });

        // Reset window if needed
        if now - state.window_start >= self.config.rate_limit_window {
            state.request_count = 0;
            state.window_start = now;
            state.blocked_until = None;
        }

        state.request_count += 1;

        // Block if limit exceeded
        if state.request_count > self.config.rate_limit_per_minute {
            state.blocked_until = Some(now + self.config.rate_limit_block_duration);
        }

        Ok(())
    }

    fn get_all_permissions(&self) -> Vec<Permission> {
        vec![
            Permission::ViewConnections,
            Permission::ManageConnections,
            Permission::ViewShares,
            Permission::SubmitShares,
            Permission::ViewTemplates,
            Permission::CreateTemplates,
            Permission::ManageTemplates,
            Permission::ViewConfig,
            Permission::UpdateConfig,
            Permission::StartDaemon,
            Permission::StopDaemon,
            Permission::RestartDaemon,
            Permission::ReloadConfig,
            Permission::ViewMetrics,
            Permission::ViewHealth,
            Permission::ManageAlerts,
            Permission::ApiAccess,
            Permission::AdminAccess,
            Permission::StartMining,
            Permission::StopMining,
            Permission::ViewMiningStats,
            Permission::ViewDatabase,
            Permission::ManageDatabase,
        ]
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            session_timeout: 3600, // 1 hour
            rate_limit_per_minute: 60,
            rate_limit_window: 60,
            rate_limit_block_duration: 300, // 5 minutes
            max_sessions_per_key: 10,
            require_auth_for_read: false,
        }
    }
}

/// Helper trait for checking permissions
pub trait HasPermission {
    fn has_permission(&self, permission: &Permission) -> bool;
}

impl HasPermission for Vec<Permission> {
    fn has_permission(&self, permission: &Permission) -> bool {
        self.contains(permission) || self.contains(&Permission::AdminAccess)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_api_key_generation() {
        let mut auth = AuthSystem::new(AuthConfig::default());
        let (key_id, api_key) = auth.generate_api_key(
            "test-key".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();

        assert!(!key_id.is_empty());
        assert!(!api_key.is_empty());
        assert_eq!(auth.api_keys.len(), 1);
    }

    #[test]
    fn test_authentication_disabled() {
        let mut auth = AuthSystem::new(AuthConfig {
            enabled: false,
            ..AuthConfig::default()
        });

        let result = auth.authenticate("invalid-key", "client-1").unwrap();
        match result {
            AuthResult::Success { permissions, .. } => {
                assert!(!permissions.is_empty());
            }
            _ => panic!("Expected success when auth is disabled"),
        }
    }

    #[test]
    fn test_authentication_success() {
        let mut auth = AuthSystem::new(AuthConfig {
            enabled: true,
            ..AuthConfig::default()
        });

        let (_, api_key) = auth.generate_api_key(
            "test-key".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();

        let result = auth.authenticate(&api_key, "client-1").unwrap();
        match result {
            AuthResult::Success { session_id, permissions } => {
                assert!(!session_id.is_empty());
                assert_eq!(permissions, vec![Permission::ViewConnections]);
            }
            _ => panic!("Expected successful authentication"),
        }
    }

    #[test]
    fn test_authentication_failure() {
        let mut auth = AuthSystem::new(AuthConfig {
            enabled: true,
            ..AuthConfig::default()
        });

        let result = auth.authenticate("invalid-key", "client-1").unwrap();
        match result {
            AuthResult::Failed { reason } => {
                assert_eq!(reason, "Invalid API key");
            }
            _ => panic!("Expected authentication failure"),
        }
    }

    #[test]
    fn test_authorization() {
        let mut auth = AuthSystem::new(AuthConfig {
            enabled: true,
            ..AuthConfig::default()
        });

        let (_, api_key) = auth.generate_api_key(
            "test-key".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();

        let auth_result = auth.authenticate(&api_key, "client-1").unwrap();
        let session_id = match auth_result {
            AuthResult::Success { session_id, .. } => session_id,
            _ => panic!("Expected successful authentication"),
        };

        // Test authorized permission
        let authz_result = auth.authorize(&session_id, &Permission::ViewConnections).unwrap();
        assert!(matches!(authz_result, AuthzResult::Granted));

        // Test unauthorized permission
        let authz_result = auth.authorize(&session_id, &Permission::ManageConnections).unwrap();
        assert!(matches!(authz_result, AuthzResult::Denied { .. }));
    }

    #[test]
    fn test_rate_limiting() {
        let mut auth = AuthSystem::new(AuthConfig {
            enabled: true,
            rate_limit_per_minute: 2,
            ..AuthConfig::default()
        });

        // First two attempts should succeed (but fail auth)
        for _ in 0..2 {
            let result = auth.authenticate("invalid-key", "client-1").unwrap();
            assert!(matches!(result, AuthResult::Failed { .. }));
        }

        // Third attempt should be rate limited
        let result = auth.authenticate("invalid-key", "client-1").unwrap();
        assert!(matches!(result, AuthResult::RateLimited { .. }));
    }

    #[test]
    fn test_session_expiration() {
        let mut auth = AuthSystem::new(AuthConfig {
            enabled: true,
            session_timeout: 1, // 1 second
            ..AuthConfig::default()
        });

        let (_, api_key) = auth.generate_api_key(
            "test-key".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();

        let auth_result = auth.authenticate(&api_key, "client-1").unwrap();
        let session_id = match auth_result {
            AuthResult::Success { session_id, .. } => session_id,
            _ => panic!("Expected successful authentication"),
        };

        // Session should be valid initially
        let authz_result = auth.authorize(&session_id, &Permission::ViewConnections).unwrap();
        assert!(matches!(authz_result, AuthzResult::Granted));

        // Wait for session to expire
        std::thread::sleep(Duration::from_secs(2));

        // Session should be invalid now
        let authz_result = auth.authorize(&session_id, &Permission::ViewConnections).unwrap();
        assert!(matches!(authz_result, AuthzResult::SessionInvalid));
    }
}