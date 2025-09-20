use crate::{Result, Error, auth::{AuthSystem, Permission, AuthResult, AuthzResult}};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

/// Connection authentication manager
pub struct ConnectionAuthManager {
    /// Authentication system
    auth_system: Arc<RwLock<AuthSystem>>,
    /// Connection sessions mapping
    connection_sessions: Arc<RwLock<HashMap<String, String>>>,
    /// Connection metadata
    connection_metadata: Arc<RwLock<HashMap<String, ConnectionMetadata>>>,
}

/// Connection metadata for authentication
#[derive(Debug, Clone)]
pub struct ConnectionMetadata {
    /// Connection ID
    pub id: String,
    /// Remote address
    pub remote_addr: SocketAddr,
    /// Protocol type (SV1 or SV2)
    pub protocol: String,
    /// Connection mode (pool, client, etc.)
    pub mode: String,
    /// Session ID if authenticated
    pub session_id: Option<String>,
    /// Connection start time
    pub connected_at: u64,
    /// Last activity time
    pub last_activity: u64,
}

/// Authentication challenge for connection
#[derive(Debug, Clone)]
pub struct AuthChallenge {
    /// Challenge ID
    pub id: String,
    /// Challenge data
    pub challenge: String,
    /// Expected response hash
    pub expected_response: String,
    /// Challenge expiration time
    pub expires_at: u64,
}

/// Connection authentication result
#[derive(Debug, Clone)]
pub enum ConnectionAuthResult {
    /// Authentication successful
    Success {
        connection_id: String,
        session_id: String,
        permissions: Vec<Permission>,
    },
    /// Authentication required
    AuthRequired {
        challenge: AuthChallenge,
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

impl ConnectionAuthManager {
    /// Create a new connection authentication manager
    pub fn new(auth_system: Arc<RwLock<AuthSystem>>) -> Self {
        Self {
            auth_system,
            connection_sessions: Arc::new(RwLock::new(HashMap::new())),
            connection_metadata: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Register a new connection
    pub async fn register_connection(
        &self,
        remote_addr: SocketAddr,
        protocol: String,
        mode: String,
    ) -> Result<String> {
        let connection_id = Uuid::new_v4().to_string();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let metadata = ConnectionMetadata {
            id: connection_id.clone(),
            remote_addr,
            protocol,
            mode,
            session_id: None,
            connected_at: now,
            last_activity: now,
        };

        let mut connections = self.connection_metadata.write().await;
        connections.insert(connection_id.clone(), metadata);

        Ok(connection_id)
    }

    /// Authenticate a connection using API key
    pub async fn authenticate_connection(
        &self,
        connection_id: &str,
        api_key: &str,
    ) -> Result<ConnectionAuthResult> {
        let client_id = {
            let connections = self.connection_metadata.read().await;
            let metadata = connections.get(connection_id)
                .ok_or_else(|| Error::Authentication("Connection not found".to_string()))?;
            metadata.remote_addr.to_string()
        };

        let mut auth_system = self.auth_system.write().await;
        let auth_result = auth_system.authenticate(api_key, &client_id)?;

        match auth_result {
            AuthResult::Success { session_id, permissions } => {
                // Update connection metadata
                {
                    let mut connections = self.connection_metadata.write().await;
                    if let Some(metadata) = connections.get_mut(connection_id) {
                        metadata.session_id = Some(session_id.clone());
                        metadata.last_activity = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .unwrap()
                            .as_secs();
                    }
                }

                // Store session mapping
                {
                    let mut sessions = self.connection_sessions.write().await;
                    sessions.insert(connection_id.to_string(), session_id.clone());
                }

                Ok(ConnectionAuthResult::Success {
                    connection_id: connection_id.to_string(),
                    session_id,
                    permissions,
                })
            }
            AuthResult::Failed { reason } => {
                Ok(ConnectionAuthResult::Failed { reason })
            }
            AuthResult::RateLimited { retry_after } => {
                Ok(ConnectionAuthResult::RateLimited { retry_after })
            }
        }
    }

    /// Check authorization for a connection operation
    pub async fn authorize_connection(
        &self,
        connection_id: &str,
        permission: &Permission,
    ) -> Result<AuthzResult> {
        let session_id = {
            let sessions = self.connection_sessions.read().await;
            sessions.get(connection_id).cloned()
        };

        let session_id = match session_id {
            Some(id) => id,
            None => return Ok(AuthzResult::SessionInvalid),
        };

        let mut auth_system = self.auth_system.write().await;
        let result = auth_system.authorize(&session_id, permission)?;

        // Update last activity
        if matches!(result, AuthzResult::Granted) {
            let mut connections = self.connection_metadata.write().await;
            if let Some(metadata) = connections.get_mut(connection_id) {
                metadata.last_activity = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();
            }
        }

        Ok(result)
    }

    /// Disconnect a connection
    pub async fn disconnect_connection(&self, connection_id: &str) -> Result<()> {
        // Remove session mapping
        let session_id = {
            let mut sessions = self.connection_sessions.write().await;
            sessions.remove(connection_id)
        };

        // Invalidate session if exists
        if let Some(session_id) = session_id {
            let mut auth_system = self.auth_system.write().await;
            auth_system.invalidate_session(&session_id)?;
        }

        // Remove connection metadata
        {
            let mut connections = self.connection_metadata.write().await;
            connections.remove(connection_id);
        }

        Ok(())
    }

    /// Get connection metadata
    pub async fn get_connection_metadata(&self, connection_id: &str) -> Option<ConnectionMetadata> {
        let connections = self.connection_metadata.read().await;
        connections.get(connection_id).cloned()
    }

    /// Get all active connections
    pub async fn get_active_connections(&self) -> Vec<ConnectionMetadata> {
        let connections = self.connection_metadata.read().await;
        connections.values().cloned().collect()
    }

    /// Get authenticated connections
    pub async fn get_authenticated_connections(&self) -> Vec<ConnectionMetadata> {
        let connections = self.connection_metadata.read().await;
        connections.values()
            .filter(|metadata| metadata.session_id.is_some())
            .cloned()
            .collect()
    }

    /// Clean up expired connections and sessions
    pub async fn cleanup_expired(&self) -> Result<()> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        // Clean up auth system
        {
            let mut auth_system = self.auth_system.write().await;
            auth_system.cleanup_expired()?;
        }

        // Get expired sessions from auth system
        let active_sessions: std::collections::HashSet<String> = {
            let auth_system = self.auth_system.read().await;
            auth_system.get_active_sessions()
                .iter()
                .map(|session| session.id.clone())
                .collect()
        };

        // Remove connections with expired sessions
        let mut expired_connections = Vec::new();
        {
            let sessions = self.connection_sessions.read().await;
            for (connection_id, session_id) in sessions.iter() {
                if !active_sessions.contains(session_id) {
                    expired_connections.push(connection_id.clone());
                }
            }
        }

        for connection_id in expired_connections {
            self.disconnect_connection(&connection_id).await?;
        }

        Ok(())
    }

    /// Check if connection requires authentication for a specific operation
    pub async fn requires_auth(&self, permission: &Permission) -> bool {
        let auth_system = self.auth_system.read().await;
        auth_system.requires_auth(permission)
    }

    /// Get connection statistics
    pub async fn get_connection_stats(&self) -> ConnectionStats {
        let connections = self.connection_metadata.read().await;
        let sessions = self.connection_sessions.read().await;

        let total_connections = connections.len();
        let authenticated_connections = sessions.len();
        let unauthenticated_connections = total_connections - authenticated_connections;

        let mut protocol_stats = HashMap::new();
        let mut mode_stats = HashMap::new();

        for metadata in connections.values() {
            *protocol_stats.entry(metadata.protocol.clone()).or_insert(0) += 1;
            *mode_stats.entry(metadata.mode.clone()).or_insert(0) += 1;
        }

        ConnectionStats {
            total_connections,
            authenticated_connections,
            unauthenticated_connections,
            protocol_stats,
            mode_stats,
        }
    }
}

/// Connection statistics
#[derive(Debug, Clone)]
pub struct ConnectionStats {
    pub total_connections: usize,
    pub authenticated_connections: usize,
    pub unauthenticated_connections: usize,
    pub protocol_stats: HashMap<String, usize>,
    pub mode_stats: HashMap<String, usize>,
}

/// Helper trait for connection authentication
pub trait ConnectionAuth {
    /// Check if connection is authenticated
    fn is_authenticated(&self) -> bool;
    
    /// Get connection session ID
    fn get_session_id(&self) -> Option<&str>;
    
    /// Check if connection has permission
    fn has_permission(&self, permission: &Permission) -> bool;
}

impl ConnectionAuth for ConnectionMetadata {
    fn is_authenticated(&self) -> bool {
        self.session_id.is_some()
    }
    
    fn get_session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }
    
    fn has_permission(&self, _permission: &Permission) -> bool {
        // This would need to be implemented with actual permission checking
        // For now, return true if authenticated
        self.is_authenticated()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{AuthSystem, AuthConfig};
    use std::net::{IpAddr, Ipv4Addr};

    #[tokio::test]
    async fn test_connection_registration() {
        let auth_system = Arc::new(RwLock::new(AuthSystem::new(AuthConfig::default())));
        let manager = ConnectionAuthManager::new(auth_system);

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let connection_id = manager.register_connection(
            addr,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();

        assert!(!connection_id.is_empty());

        let metadata = manager.get_connection_metadata(&connection_id).await.unwrap();
        assert_eq!(metadata.remote_addr, addr);
        assert_eq!(metadata.protocol, "sv2");
        assert_eq!(metadata.mode, "pool");
        assert!(metadata.session_id.is_none());
    }

    #[tokio::test]
    async fn test_connection_authentication() {
        let mut auth_system = AuthSystem::new(AuthConfig {
            enabled: true,
            ..AuthConfig::default()
        });

        let (_, api_key) = auth_system.generate_api_key(
            "test-key".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();

        let auth_system = Arc::new(RwLock::new(auth_system));
        let manager = ConnectionAuthManager::new(auth_system);

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let connection_id = manager.register_connection(
            addr,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();

        let result = manager.authenticate_connection(&connection_id, &api_key).await.unwrap();
        match result {
            ConnectionAuthResult::Success { session_id, permissions, .. } => {
                assert!(!session_id.is_empty());
                assert_eq!(permissions, vec![Permission::ViewConnections]);
            }
            _ => panic!("Expected successful authentication"),
        }

        let metadata = manager.get_connection_metadata(&connection_id).await.unwrap();
        assert!(metadata.session_id.is_some());
    }

    #[tokio::test]
    async fn test_connection_authorization() {
        let mut auth_system = AuthSystem::new(AuthConfig {
            enabled: true,
            ..AuthConfig::default()
        });

        let (_, api_key) = auth_system.generate_api_key(
            "test-key".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();

        let auth_system = Arc::new(RwLock::new(auth_system));
        let manager = ConnectionAuthManager::new(auth_system);

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let connection_id = manager.register_connection(
            addr,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();

        // Authenticate first
        manager.authenticate_connection(&connection_id, &api_key).await.unwrap();

        // Test authorized permission
        let result = manager.authorize_connection(&connection_id, &Permission::ViewConnections).await.unwrap();
        assert!(matches!(result, AuthzResult::Granted));

        // Test unauthorized permission
        let result = manager.authorize_connection(&connection_id, &Permission::ManageConnections).await.unwrap();
        assert!(matches!(result, AuthzResult::Denied { .. }));
    }

    #[tokio::test]
    async fn test_connection_disconnect() {
        let auth_system = Arc::new(RwLock::new(AuthSystem::new(AuthConfig::default())));
        let manager = ConnectionAuthManager::new(auth_system);

        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let connection_id = manager.register_connection(
            addr,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();

        assert!(manager.get_connection_metadata(&connection_id).await.is_some());

        manager.disconnect_connection(&connection_id).await.unwrap();

        assert!(manager.get_connection_metadata(&connection_id).await.is_none());
    }
}