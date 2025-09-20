//! Protocol translation service for proxy mode
//! 
//! This module handles the translation between SV1 and SV2 protocols
//! for downstream miners connecting to the proxy.

use crate::{
    Result, Error, Connection, Share, WorkTemplate, ConnectionId,
    protocol::{ProtocolMessage, ProtocolTranslator},
    types::{Protocol, Job, ShareSubmission},
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{debug, warn, error};
use uuid::Uuid;

/// Protocol translation service for proxy mode
pub struct ProxyProtocolService {
    translator: Arc<ProtocolTranslator>,
    /// Maps downstream connection IDs to their protocol state
    connection_states: Arc<RwLock<HashMap<ConnectionId, ConnectionProtocolState>>>,
    /// Maps SV1 job IDs to SV2 work templates
    job_mappings: Arc<RwLock<HashMap<String, WorkTemplate>>>,
    /// Maps SV2 template IDs to SV1 job IDs
    reverse_job_mappings: Arc<RwLock<HashMap<Uuid, String>>>,
}

/// Protocol state for a downstream connection
#[derive(Debug, Clone)]
pub struct ConnectionProtocolState {
    pub connection_id: ConnectionId,
    pub protocol: Protocol,
    pub subscribed: bool,
    pub authorized: bool,
    pub difficulty: f64,
    pub extranonce1: String,
    pub extranonce2_size: u8,
    pub worker_name: Option<String>,
    pub current_job_id: Option<String>,
}

impl Default for ConnectionProtocolState {
    fn default() -> Self {
        Self {
            connection_id: Uuid::new_v4(),
            protocol: Protocol::Sv1,
            subscribed: false,
            authorized: false,
            difficulty: 1.0,
            extranonce1: format!("{:08x}", rand::random::<u32>()),
            extranonce2_size: 4,
            worker_name: None,
            current_job_id: None,
        }
    }
}

impl ProxyProtocolService {
    pub fn new() -> Self {
        Self {
            translator: Arc::new(ProtocolTranslator::new()),
            connection_states: Arc::new(RwLock::new(HashMap::new())),
            job_mappings: Arc::new(RwLock::new(HashMap::new())),
            reverse_job_mappings: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Initialize protocol state for a new downstream connection
    pub async fn initialize_connection(&self, connection: &Connection) -> Result<()> {
        let mut states = self.connection_states.write().await;
        let state = ConnectionProtocolState {
            connection_id: connection.id,
            protocol: connection.protocol,
            ..Default::default()
        };
        states.insert(connection.id, state);
        debug!("Initialized protocol state for connection: {}", connection.id);
        Ok(())
    }

    /// Handle incoming message from downstream miner
    pub async fn handle_downstream_message(
        &self,
        connection_id: ConnectionId,
        message: ProtocolMessage,
    ) -> Result<Vec<ProtocolMessage>> {
        debug!("Handling downstream message from {}: {:?}", connection_id, message.message_type());

        match message {
            ProtocolMessage::Subscribe { user_agent, session_id } => {
                self.handle_subscribe(connection_id, user_agent, session_id).await
            }
            ProtocolMessage::Authorize { username, password } => {
                self.handle_authorize(connection_id, username, password).await
            }
            ProtocolMessage::Submit { username, job_id, extranonce2, ntime, nonce } => {
                self.handle_submit(connection_id, username, job_id, extranonce2, ntime, nonce).await
            }
            _ => {
                warn!("Unsupported downstream message type: {}", message.message_type());
                Ok(vec![ProtocolMessage::Error {
                    code: 20,
                    message: "Unsupported method".to_string(),
                }])
            }
        }
    }

    /// Handle subscription request from SV1 miner
    async fn handle_subscribe(
        &self,
        connection_id: ConnectionId,
        user_agent: String,
        session_id: Option<String>,
    ) -> Result<Vec<ProtocolMessage>> {
        debug!("Handling subscribe from connection: {}", connection_id);

        let mut states = self.connection_states.write().await;
        if let Some(state) = states.get_mut(&connection_id) {
            state.subscribed = true;
            
            // Generate extranonce1 if not already set
            if state.extranonce1.is_empty() {
                state.extranonce1 = format!("{:08x}", rand::random::<u32>());
            }

            // Return subscription response
            Ok(vec![ProtocolMessage::Subscribe {
                user_agent: format!("sv2-proxy/{}", env!("CARGO_PKG_VERSION")),
                session_id: Some(state.extranonce1.clone()),
            }])
        } else {
            error!("Connection state not found for: {}", connection_id);
            Ok(vec![ProtocolMessage::Error {
                code: 25,
                message: "Connection not found".to_string(),
            }])
        }
    }

    /// Handle authorization request from SV1 miner
    async fn handle_authorize(
        &self,
        connection_id: ConnectionId,
        username: String,
        password: String,
    ) -> Result<Vec<ProtocolMessage>> {
        debug!("Handling authorize from connection: {} (user: {})", connection_id, username);

        let mut states = self.connection_states.write().await;
        if let Some(state) = states.get_mut(&connection_id) {
            // For proxy mode, we generally accept all authorizations
            // In a real implementation, you might validate against upstream pool requirements
            state.authorized = true;
            state.worker_name = Some(username.clone());

            debug!("Authorized worker: {} for connection: {}", username, connection_id);
            
            // Return success response (SV1 authorize response is just a boolean)
            Ok(vec![])
        } else {
            error!("Connection state not found for: {}", connection_id);
            Ok(vec![ProtocolMessage::Error {
                code: 25,
                message: "Connection not found".to_string(),
            }])
        }
    }

    /// Handle share submission from SV1 miner
    async fn handle_submit(
        &self,
        connection_id: ConnectionId,
        username: String,
        job_id: String,
        extranonce2: String,
        ntime: String,
        nonce: String,
    ) -> Result<Vec<ProtocolMessage>> {
        debug!("Handling submit from connection: {} (job: {})", connection_id, job_id);

        let states = self.connection_states.read().await;
        let state = match states.get(&connection_id) {
            Some(state) => state.clone(),
            None => {
                error!("Connection state not found for: {}", connection_id);
                return Ok(vec![ProtocolMessage::Error {
                    code: 25,
                    message: "Connection not found".to_string(),
                }]);
            }
        };

        if !state.authorized {
            warn!("Unauthorized share submission from connection: {}", connection_id);
            return Ok(vec![ProtocolMessage::Error {
                code: 24,
                message: "Unauthorized worker".to_string(),
            }]);
        }

        // Get the work template for this job
        let job_mappings = self.job_mappings.read().await;
        let template = match job_mappings.get(&job_id) {
            Some(template) => template.clone(),
            None => {
                warn!("Unknown job ID: {} from connection: {}", job_id, connection_id);
                return Ok(vec![ProtocolMessage::Error {
                    code: 21,
                    message: "Job not found".to_string(),
                }]);
            }
        };

        // Parse nonce and ntime
        let nonce_u32 = u32::from_str_radix(&nonce, 16)
            .map_err(|e| Error::Protocol(format!("Invalid nonce hex: {}", e)))?;
        let ntime_u32 = u32::from_str_radix(&ntime, 16)
            .map_err(|e| Error::Protocol(format!("Invalid ntime hex: {}", e)))?;

        // Create share submission
        let share_submission = ShareSubmission::new(
            connection_id,
            job_id.clone(),
            extranonce2,
            ntime_u32,
            nonce_u32,
            username,
            state.difficulty,
        );

        debug!("Created share submission for connection: {}", connection_id);

        // Return success response (actual validation happens upstream)
        Ok(vec![])
    }

    /// Forward work template from upstream to downstream miners
    pub async fn forward_work_template(
        &self,
        template: &WorkTemplate,
        target_connections: &[ConnectionId],
    ) -> Result<Vec<(ConnectionId, ProtocolMessage)>> {
        debug!("Forwarding work template to {} connections", target_connections.len());

        let mut responses = Vec::new();
        let job_id = format!("{:x}", template.id.as_u128());

        // Store job mapping
        {
            let mut job_mappings = self.job_mappings.write().await;
            let mut reverse_mappings = self.reverse_job_mappings.write().await;
            
            job_mappings.insert(job_id.clone(), template.clone());
            reverse_mappings.insert(template.id, job_id.clone());
        }

        let states = self.connection_states.read().await;
        
        for &connection_id in target_connections {
            if let Some(state) = states.get(&connection_id) {
                if state.subscribed && state.authorized {
                    match state.protocol {
                        Protocol::Sv1 | Protocol::StratumV1 => {
                            let notify_message = self.create_sv1_notify_message(template, &job_id, state)?;
                            responses.push((connection_id, notify_message));
                        }
                        Protocol::Sv2 | Protocol::StratumV2 => {
                            // For SV2 connections, we would create appropriate SV2 messages
                            // This is simplified for now
                            debug!("SV2 work forwarding not fully implemented");
                        }
                    }
                }
            }
        }

        debug!("Created {} work notifications", responses.len());
        Ok(responses)
    }

    /// Create SV1 notify message from work template
    fn create_sv1_notify_message(
        &self,
        template: &WorkTemplate,
        job_id: &str,
        state: &ConnectionProtocolState,
    ) -> Result<ProtocolMessage> {
        // Simplified SV1 notify message creation
        // In a real implementation, this would properly construct all fields
        
        let prevhash = format!("{:x}", template.previous_hash);
        let coinb1 = "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff".to_string();
        let coinb2 = format!("{}ffffffff", state.extranonce1);
        let merkle_branch = vec![]; // Simplified - would contain actual merkle branch
        let version = "20000000".to_string();
        let nbits = "207fffff".to_string(); // Simplified difficulty
        let ntime = format!("{:08x}", template.timestamp);
        let clean_jobs = true;

        Ok(ProtocolMessage::Notify {
            job_id: job_id.to_string(),
            prevhash,
            coinb1,
            coinb2,
            merkle_branch,
            version,
            nbits,
            ntime,
            clean_jobs,
        })
    }

    /// Get share submission for upstream forwarding
    pub async fn create_share_for_upstream(
        &self,
        connection_id: ConnectionId,
        job_id: &str,
        extranonce2: &str,
        ntime: u32,
        nonce: u32,
    ) -> Result<Share> {
        let states = self.connection_states.read().await;
        let state = states.get(&connection_id)
            .ok_or_else(|| Error::Protocol("Connection state not found".to_string()))?;

        let share = Share::new(connection_id, nonce, ntime, state.difficulty);
        debug!("Created share for upstream forwarding: connection={}", connection_id);
        Ok(share)
    }

    /// Update connection difficulty
    pub async fn update_connection_difficulty(
        &self,
        connection_id: ConnectionId,
        new_difficulty: f64,
    ) -> Result<()> {
        let mut states = self.connection_states.write().await;
        if let Some(state) = states.get_mut(&connection_id) {
            state.difficulty = new_difficulty;
            debug!("Updated difficulty for connection {}: {}", connection_id, new_difficulty);
        }
        Ok(())
    }

    /// Remove connection state when connection is closed
    pub async fn cleanup_connection(&self, connection_id: ConnectionId) -> Result<()> {
        let mut states = self.connection_states.write().await;
        states.remove(&connection_id);
        debug!("Cleaned up protocol state for connection: {}", connection_id);
        Ok(())
    }

    /// Get connection state for debugging/monitoring
    pub async fn get_connection_state(&self, connection_id: ConnectionId) -> Option<ConnectionProtocolState> {
        let states = self.connection_states.read().await;
        states.get(&connection_id).cloned()
    }

    /// Get all active connections
    pub async fn get_active_connections(&self) -> Vec<ConnectionId> {
        let states = self.connection_states.read().await;
        states.keys().copied().collect()
    }

    /// Get statistics about protocol translation
    pub async fn get_translation_stats(&self) -> TranslationStats {
        let states = self.connection_states.read().await;
        let job_mappings = self.job_mappings.read().await;

        let total_connections = states.len();
        let subscribed_connections = states.values().filter(|s| s.subscribed).count();
        let authorized_connections = states.values().filter(|s| s.authorized).count();
        let sv1_connections = states.values().filter(|s| s.protocol == Protocol::Sv1).count();
        let sv2_connections = states.values().filter(|s| s.protocol == Protocol::Sv2).count();
        let active_jobs = job_mappings.len();

        TranslationStats {
            total_connections,
            subscribed_connections,
            authorized_connections,
            sv1_connections,
            sv2_connections,
            active_jobs,
        }
    }
}

/// Statistics about protocol translation
#[derive(Debug, Clone)]
pub struct TranslationStats {
    pub total_connections: usize,
    pub subscribed_connections: usize,
    pub authorized_connections: usize,
    pub sv1_connections: usize,
    pub sv2_connections: usize,
    pub active_jobs: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Protocol;
    use std::net::SocketAddr;

    fn create_test_connection(protocol: Protocol) -> Connection {
        let addr: SocketAddr = "127.0.0.1:3333".parse().unwrap();
        Connection::new(addr, protocol)
    }

    fn create_test_template() -> WorkTemplate {
        use bitcoin::{BlockHash, Transaction};
        use std::str::FromStr;
        
        let prev_hash = BlockHash::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap();
        let coinbase_tx = Transaction {
            version: 1,
            lock_time: bitcoin::absolute::LockTime::ZERO,
            input: vec![bitcoin::TxIn::default()],
            output: vec![bitcoin::TxOut::default()],
        };
        
        WorkTemplate::new(prev_hash, coinbase_tx, vec![], 1.0)
    }

    #[tokio::test]
    async fn test_protocol_service_creation() {
        let service = ProxyProtocolService::new();
        let stats = service.get_translation_stats().await;
        
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.active_jobs, 0);
    }

    #[tokio::test]
    async fn test_connection_initialization() {
        let service = ProxyProtocolService::new();
        let connection = create_test_connection(Protocol::Sv1);
        
        let result = service.initialize_connection(&connection).await;
        assert!(result.is_ok());
        
        let state = service.get_connection_state(connection.id).await;
        assert!(state.is_some());
        assert_eq!(state.unwrap().protocol, Protocol::Sv1);
    }

    #[tokio::test]
    async fn test_subscribe_handling() {
        let service = ProxyProtocolService::new();
        let connection = create_test_connection(Protocol::Sv1);
        
        service.initialize_connection(&connection).await.unwrap();
        
        let subscribe_msg = ProtocolMessage::Subscribe {
            user_agent: "test_miner".to_string(),
            session_id: Some("session123".to_string()),
        };
        
        let responses = service.handle_downstream_message(connection.id, subscribe_msg).await.unwrap();
        assert_eq!(responses.len(), 1);
        
        let state = service.get_connection_state(connection.id).await.unwrap();
        assert!(state.subscribed);
    }

    #[tokio::test]
    async fn test_authorize_handling() {
        let service = ProxyProtocolService::new();
        let connection = create_test_connection(Protocol::Sv1);
        
        service.initialize_connection(&connection).await.unwrap();
        
        let authorize_msg = ProtocolMessage::Authorize {
            username: "test_worker".to_string(),
            password: "password".to_string(),
        };
        
        let responses = service.handle_downstream_message(connection.id, authorize_msg).await.unwrap();
        assert!(responses.is_empty()); // Authorize response is handled differently in SV1
        
        let state = service.get_connection_state(connection.id).await.unwrap();
        assert!(state.authorized);
        assert_eq!(state.worker_name, Some("test_worker".to_string()));
    }

    #[tokio::test]
    async fn test_work_template_forwarding() {
        let service = ProxyProtocolService::new();
        let connection = create_test_connection(Protocol::Sv1);
        
        service.initialize_connection(&connection).await.unwrap();
        
        // Subscribe and authorize first
        let subscribe_msg = ProtocolMessage::Subscribe {
            user_agent: "test_miner".to_string(),
            session_id: None,
        };
        service.handle_downstream_message(connection.id, subscribe_msg).await.unwrap();
        
        let authorize_msg = ProtocolMessage::Authorize {
            username: "test_worker".to_string(),
            password: "password".to_string(),
        };
        service.handle_downstream_message(connection.id, authorize_msg).await.unwrap();
        
        // Forward work template
        let template = create_test_template();
        let responses = service.forward_work_template(&template, &[connection.id]).await.unwrap();
        
        assert_eq!(responses.len(), 1);
        assert_eq!(responses[0].0, connection.id);
        
        match &responses[0].1 {
            ProtocolMessage::Notify { job_id, clean_jobs, .. } => {
                assert!(!job_id.is_empty());
                assert!(*clean_jobs);
            }
            _ => panic!("Expected Notify message"),
        }
    }

    #[tokio::test]
    async fn test_share_creation() {
        let service = ProxyProtocolService::new();
        let connection = create_test_connection(Protocol::Sv1);
        
        service.initialize_connection(&connection).await.unwrap();
        
        let share = service.create_share_for_upstream(
            connection.id,
            "job123",
            "abcd",
            1234567890,
            0x12345678,
        ).await.unwrap();
        
        assert_eq!(share.connection_id, connection.id);
        assert_eq!(share.nonce, 0x12345678);
        assert_eq!(share.timestamp, 1234567890);
    }

    #[tokio::test]
    async fn test_difficulty_update() {
        let service = ProxyProtocolService::new();
        let connection = create_test_connection(Protocol::Sv1);
        
        service.initialize_connection(&connection).await.unwrap();
        
        let result = service.update_connection_difficulty(connection.id, 2.5).await;
        assert!(result.is_ok());
        
        let state = service.get_connection_state(connection.id).await.unwrap();
        assert_eq!(state.difficulty, 2.5);
    }

    #[tokio::test]
    async fn test_connection_cleanup() {
        let service = ProxyProtocolService::new();
        let connection = create_test_connection(Protocol::Sv1);
        
        service.initialize_connection(&connection).await.unwrap();
        assert!(service.get_connection_state(connection.id).await.is_some());
        
        service.cleanup_connection(connection.id).await.unwrap();
        assert!(service.get_connection_state(connection.id).await.is_none());
    }

    #[tokio::test]
    async fn test_translation_stats() {
        let service = ProxyProtocolService::new();
        let connection1 = create_test_connection(Protocol::Sv1);
        let connection2 = create_test_connection(Protocol::Sv2);
        
        service.initialize_connection(&connection1).await.unwrap();
        service.initialize_connection(&connection2).await.unwrap();
        
        let stats = service.get_translation_stats().await;
        assert_eq!(stats.total_connections, 2);
        assert_eq!(stats.sv1_connections, 1);
        assert_eq!(stats.sv2_connections, 1);
        assert_eq!(stats.subscribed_connections, 0);
        assert_eq!(stats.authorized_connections, 0);
    }
}