use crate::{
    auth::{AuthSystem, AuthConfig, Permission, AuthResult, AuthzResult},
    connection_auth::{ConnectionAuthManager, ConnectionAuthResult},
};
use std::sync::Arc;
use tokio::sync::RwLock;
use std::net::{SocketAddr, IpAddr, Ipv4Addr};

/// Integration tests for the authentication system
#[cfg(test)]
mod integration_tests {
    use super::*;

    #[tokio::test]
    async fn test_full_authentication_flow() {
        // Create authentication system
        let auth_config = AuthConfig {
            enabled: true,
            session_timeout: 3600,
            rate_limit_per_minute: 10,
            rate_limit_window: 60,
            rate_limit_block_duration: 300,
            max_sessions_per_key: 5,
            require_auth_for_read: true,
        };
        
        let mut auth_system = AuthSystem::new(auth_config);
        
        // Generate API key
        let (key_id, api_key) = auth_system.generate_api_key(
            "test-integration".to_string(),
            vec![Permission::ViewConnections, Permission::ViewShares, Permission::ApiAccess],
            None,
        ).unwrap();
        
        // Create connection auth manager
        let auth_system_arc = Arc::new(RwLock::new(auth_system));
        let connection_auth = ConnectionAuthManager::new(auth_system_arc.clone());
        
        // Register a connection
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let connection_id = connection_auth.register_connection(
            addr,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();
        
        // Authenticate the connection
        let auth_result = connection_auth.authenticate_connection(&connection_id, &api_key).await.unwrap();
        
        let session_id = match auth_result {
            ConnectionAuthResult::Success { session_id, permissions, .. } => {
                assert_eq!(permissions.len(), 3);
                assert!(permissions.contains(&Permission::ViewConnections));
                assert!(permissions.contains(&Permission::ViewShares));
                assert!(permissions.contains(&Permission::ApiAccess));
                session_id
            }
            _ => panic!("Expected successful authentication"),
        };
        
        // Test authorization for allowed permission
        let authz_result = connection_auth.authorize_connection(&connection_id, &Permission::ViewConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::Granted));
        
        // Test authorization for denied permission
        let authz_result = connection_auth.authorize_connection(&connection_id, &Permission::ManageConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::Denied { .. }));
        
        // Test session management
        let active_connections = connection_auth.get_active_connections().await;
        assert_eq!(active_connections.len(), 1);
        
        let authenticated_connections = connection_auth.get_authenticated_connections().await;
        assert_eq!(authenticated_connections.len(), 1);
        
        // Test connection stats
        let stats = connection_auth.get_connection_stats().await;
        assert_eq!(stats.total_connections, 1);
        assert_eq!(stats.authenticated_connections, 1);
        assert_eq!(stats.unauthenticated_connections, 0);
        
        // Disconnect the connection
        connection_auth.disconnect_connection(&connection_id).await.unwrap();
        
        // Verify connection is removed
        let active_connections = connection_auth.get_active_connections().await;
        assert_eq!(active_connections.len(), 0);
        
        // Verify session is invalidated
        let authz_result = connection_auth.authorize_connection(&connection_id, &Permission::ViewConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::SessionInvalid));
    }

    #[tokio::test]
    async fn test_rate_limiting_integration() {
        let auth_config = AuthConfig {
            enabled: true,
            rate_limit_per_minute: 2, // Very low limit for testing
            rate_limit_window: 60,
            rate_limit_block_duration: 300,
            ..AuthConfig::default()
        };
        
        let auth_system_arc = Arc::new(RwLock::new(AuthSystem::new(auth_config)));
        let connection_auth = ConnectionAuthManager::new(auth_system_arc.clone());
        
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let connection_id = connection_auth.register_connection(
            addr,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();
        
        // First two attempts should fail (invalid key) but not be rate limited
        for _ in 0..2 {
            let result = connection_auth.authenticate_connection(&connection_id, "invalid-key").await.unwrap();
            assert!(matches!(result, ConnectionAuthResult::Failed { .. }));
        }
        
        // Third attempt should be rate limited
        let result = connection_auth.authenticate_connection(&connection_id, "invalid-key").await.unwrap();
        assert!(matches!(result, ConnectionAuthResult::RateLimited { .. }));
    }

    #[tokio::test]
    async fn test_session_expiration_integration() {
        let auth_config = AuthConfig {
            enabled: true,
            session_timeout: 1, // 1 second for testing
            ..AuthConfig::default()
        };
        
        let mut auth_system = AuthSystem::new(auth_config);
        let (_, api_key) = auth_system.generate_api_key(
            "test-expiration".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();
        
        let auth_system_arc = Arc::new(RwLock::new(auth_system));
        let connection_auth = ConnectionAuthManager::new(auth_system_arc.clone());
        
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let connection_id = connection_auth.register_connection(
            addr,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();
        
        // Authenticate successfully
        let result = connection_auth.authenticate_connection(&connection_id, &api_key).await.unwrap();
        assert!(matches!(result, ConnectionAuthResult::Success { .. }));
        
        // Verify authorization works initially
        let authz_result = connection_auth.authorize_connection(&connection_id, &Permission::ViewConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::Granted));
        
        // Wait for session to expire
        tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        
        // Verify session is now invalid
        let authz_result = connection_auth.authorize_connection(&connection_id, &Permission::ViewConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::SessionInvalid));
    }

    #[tokio::test]
    async fn test_multiple_connections_same_key() {
        let auth_config = AuthConfig {
            enabled: true,
            max_sessions_per_key: 2,
            ..AuthConfig::default()
        };
        
        let mut auth_system = AuthSystem::new(auth_config);
        let (_, api_key) = auth_system.generate_api_key(
            "test-multi".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();
        
        let auth_system_arc = Arc::new(RwLock::new(auth_system));
        let connection_auth = ConnectionAuthManager::new(auth_system_arc.clone());
        
        // Create multiple connections
        let mut connection_ids = Vec::new();
        for i in 0..3 {
            let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080 + i);
            let connection_id = connection_auth.register_connection(
                addr,
                "sv2".to_string(),
                "pool".to_string(),
            ).await.unwrap();
            connection_ids.push(connection_id);
        }
        
        // First two authentications should succeed
        for i in 0..2 {
            let result = connection_auth.authenticate_connection(&connection_ids[i], &api_key).await.unwrap();
            assert!(matches!(result, ConnectionAuthResult::Success { .. }));
        }
        
        // Third authentication should fail due to session limit
        let result = connection_auth.authenticate_connection(&connection_ids[2], &api_key).await.unwrap();
        assert!(matches!(result, ConnectionAuthResult::Failed { .. }));
        
        // Verify stats
        let stats = connection_auth.get_connection_stats().await;
        assert_eq!(stats.total_connections, 3);
        assert_eq!(stats.authenticated_connections, 2);
        assert_eq!(stats.unauthenticated_connections, 1);
    }

    #[tokio::test]
    async fn test_permission_hierarchy() {
        let auth_config = AuthConfig {
            enabled: true,
            ..AuthConfig::default()
        };
        
        let mut auth_system = AuthSystem::new(auth_config);
        
        // Create API key with admin access
        let (_, admin_key) = auth_system.generate_api_key(
            "admin".to_string(),
            vec![Permission::AdminAccess],
            None,
        ).unwrap();
        
        // Create API key with limited access
        let (_, limited_key) = auth_system.generate_api_key(
            "limited".to_string(),
            vec![Permission::ViewConnections],
            None,
        ).unwrap();
        
        let auth_system_arc = Arc::new(RwLock::new(auth_system));
        let connection_auth = ConnectionAuthManager::new(auth_system_arc.clone());
        
        // Test admin connection
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let admin_conn = connection_auth.register_connection(
            addr1,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();
        
        let result = connection_auth.authenticate_connection(&admin_conn, &admin_key).await.unwrap();
        assert!(matches!(result, ConnectionAuthResult::Success { .. }));
        
        // Admin should have access to everything
        let authz_result = connection_auth.authorize_connection(&admin_conn, &Permission::ViewConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::Granted));
        
        let authz_result = connection_auth.authorize_connection(&admin_conn, &Permission::ManageConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::Granted));
        
        // Test limited connection
        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 2)), 8080);
        let limited_conn = connection_auth.register_connection(
            addr2,
            "sv2".to_string(),
            "pool".to_string(),
        ).await.unwrap();
        
        let result = connection_auth.authenticate_connection(&limited_conn, &limited_key).await.unwrap();
        assert!(matches!(result, ConnectionAuthResult::Success { .. }));
        
        // Limited should only have specific access
        let authz_result = connection_auth.authorize_connection(&limited_conn, &Permission::ViewConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::Granted));
        
        let authz_result = connection_auth.authorize_connection(&limited_conn, &Permission::ManageConnections).await.unwrap();
        assert!(matches!(authz_result, AuthzResult::Denied { .. }));
    }
}