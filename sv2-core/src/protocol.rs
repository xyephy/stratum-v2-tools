use crate::{Result, Protocol, Error};
use async_trait::async_trait;
use std::net::SocketAddr;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Protocol handler interface
#[async_trait]
pub trait ProtocolHandler: Send + Sync {
    /// Detect protocol version from initial connection
    async fn detect_protocol(&self, addr: SocketAddr) -> Result<Protocol>;

    /// Handle protocol-specific message parsing
    async fn parse_message(&self, data: &[u8], protocol: Protocol) -> Result<ProtocolMessage>;

    /// Serialize message for transmission
    async fn serialize_message(&self, message: ProtocolMessage, protocol: Protocol) -> Result<Vec<u8>>;

    /// Translate between SV1 and SV2 protocols
    async fn translate_message(&self, message: ProtocolMessage, target_protocol: Protocol) -> Result<ProtocolMessage>;

    /// Validate protocol message
    fn validate_message(&self, message: &ProtocolMessage) -> Result<()>;
}

/// Network protocol message for server communication
#[derive(Debug, Clone)]
pub enum NetworkProtocolMessage {
    /// New connection established
    Connect {
        connection_id: crate::types::ConnectionId,
        peer_addr: SocketAddr,
        protocol: Protocol,
    },
    /// Connection disconnected
    Disconnect {
        connection_id: crate::types::ConnectionId,
        reason: String,
    },
    /// Stratum V1 message
    StratumV1 {
        connection_id: crate::types::ConnectionId,
        message: StratumMessage,
    },
    /// Stratum V2 message
    StratumV2 {
        connection_id: crate::types::ConnectionId,
        data: Vec<u8>,
    },
    /// Send response to connection
    SendResponse {
        connection_id: crate::types::ConnectionId,
        response: String,
    },
    /// Send work notification to connection
    SendWork {
        connection_id: crate::types::ConnectionId,
        work_template: crate::types::WorkTemplate,
    },
}

/// Stratum V1 message structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct StratumMessage {
    pub method: Option<String>,
    pub params: Option<serde_json::Value>,
    pub id: Option<serde_json::Value>,
    pub result: Option<serde_json::Value>,
    pub error: Option<serde_json::Value>,
}

impl StratumMessage {
    /// Parse a Stratum message from JSON
    pub fn from_json(value: &serde_json::Value) -> Result<Self> {
        serde_json::from_value(value.clone())
            .map_err(|e| Error::Protocol(format!("Failed to parse Stratum message: {}", e)))
    }

    /// Convert to JSON string
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(self)
            .map_err(|e| Error::Protocol(format!("Failed to serialize Stratum message: {}", e)))
    }

    /// Check if this is a request message
    pub fn is_request(&self) -> bool {
        self.method.is_some()
    }

    /// Check if this is a response message
    pub fn is_response(&self) -> bool {
        self.result.is_some() || self.error.is_some()
    }
}

/// Generic protocol message representation
#[derive(Debug, Clone)]
pub enum ProtocolMessage {
    // SV1 Messages
    Subscribe {
        user_agent: String,
        session_id: Option<String>,
    },
    Authorize {
        username: String,
        password: String,
    },
    Submit {
        username: String,
        job_id: String,
        extranonce2: String,
        ntime: String,
        nonce: String,
    },
    Notify {
        job_id: String,
        prevhash: String,
        coinb1: String,
        coinb2: String,
        merkle_branch: Vec<String>,
        version: String,
        nbits: String,
        ntime: String,
        clean_jobs: bool,
    },

    // SV2 Messages
    SetupConnection {
        protocol: u16,
        min_version: u16,
        max_version: u16,
        flags: u32,
        endpoint_host: String,
        endpoint_port: u16,
        vendor: String,
        hardware_version: String,
        firmware: String,
        device_id: String,
    },
    SetupConnectionSuccess {
        used_version: u16,
        flags: u32,
    },
    OpenStandardMiningChannel {
        request_id: u32,
        user_identity: String,
        nominal_hash_rate: f32,
        max_target: [u8; 32],
    },
    OpenStandardMiningChannelSuccess {
        request_id: u32,
        channel_id: u32,
        target: [u8; 32],
        extranonce_prefix: Vec<u8>,
        group_channel_id: u32,
    },
    NewMiningJob {
        channel_id: u32,
        job_id: u32,
        future_job: bool,
        version: u32,
        merkle_path: Vec<[u8; 32]>,
    },
    SubmitSharesStandard {
        channel_id: u32,
        sequence_number: u32,
        job_id: u32,
        nonce: u32,
        ntime: u32,
        version: u32,
    },

    // Common messages
    Error {
        code: u32,
        message: String,
    },
    Ping,
    Pong,
}

impl ProtocolMessage {
    /// Get message type identifier
    pub fn message_type(&self) -> &'static str {
        match self {
            ProtocolMessage::Subscribe { .. } => "subscribe",
            ProtocolMessage::Authorize { .. } => "authorize",
            ProtocolMessage::Submit { .. } => "submit",
            ProtocolMessage::Notify { .. } => "notify",
            ProtocolMessage::SetupConnection { .. } => "setup_connection",
            ProtocolMessage::SetupConnectionSuccess { .. } => "setup_connection_success",
            ProtocolMessage::OpenStandardMiningChannel { .. } => "open_standard_mining_channel",
            ProtocolMessage::OpenStandardMiningChannelSuccess { .. } => "open_standard_mining_channel_success",
            ProtocolMessage::NewMiningJob { .. } => "new_mining_job",
            ProtocolMessage::SubmitSharesStandard { .. } => "submit_shares_standard",
            ProtocolMessage::Error { .. } => "error",
            ProtocolMessage::Ping => "ping",
            ProtocolMessage::Pong => "pong",
        }
    }

    /// Get protocol this message belongs to
    pub fn protocol(&self) -> Protocol {
        match self {
            ProtocolMessage::Subscribe { .. } |
            ProtocolMessage::Authorize { .. } |
            ProtocolMessage::Submit { .. } |
            ProtocolMessage::Notify { .. } => Protocol::Sv1,
            
            ProtocolMessage::SetupConnection { .. } |
            ProtocolMessage::SetupConnectionSuccess { .. } |
            ProtocolMessage::OpenStandardMiningChannel { .. } |
            ProtocolMessage::OpenStandardMiningChannelSuccess { .. } |
            ProtocolMessage::NewMiningJob { .. } |
            ProtocolMessage::SubmitSharesStandard { .. } => Protocol::Sv2,
            
            ProtocolMessage::Error { .. } |
            ProtocolMessage::Ping |
            ProtocolMessage::Pong => Protocol::Sv1, // Default to SV1 for common messages
        }
    }
}

/// Protocol translator that handles bidirectional translation between SV1 and SV2
pub struct ProtocolTranslator {
    translation_state: Arc<RwLock<HashMap<SocketAddr, TranslationState>>>,
}

/// Translation state for maintaining context during protocol conversion
#[derive(Debug, Clone)]
pub struct TranslationState {
    pub detected_protocol: Protocol,
    pub target_protocol: Protocol,
    pub job_id_mapping: HashMap<String, u32>, // SV1 job_id -> SV2 job_id
    pub reverse_job_mapping: HashMap<u32, String>, // SV2 job_id -> SV1 job_id
    pub channel_id: Option<u32>,
    pub extranonce1: String,
    pub sequence_counter: u32,
}

impl Default for TranslationState {
    fn default() -> Self {
        Self {
            detected_protocol: Protocol::Sv1,
            target_protocol: Protocol::Sv2,
            job_id_mapping: HashMap::new(),
            reverse_job_mapping: HashMap::new(),
            channel_id: None,
            extranonce1: String::new(),
            sequence_counter: 0,
        }
    }
}

impl ProtocolTranslator {
    pub fn new() -> Self {
        Self {
            translation_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Translate SV1 message to SV2 equivalent
    pub async fn translate_sv1_to_sv2(&self, addr: SocketAddr, message: ProtocolMessage) -> Result<ProtocolMessage> {
        match message {
            ProtocolMessage::Subscribe { user_agent, session_id } => {
                Ok(ProtocolMessage::SetupConnection {
                    protocol: 2,
                    min_version: 2,
                    max_version: 2,
                    flags: 0,
                    endpoint_host: "localhost".to_string(),
                    endpoint_port: 4444,
                    vendor: user_agent,
                    hardware_version: "1.0".to_string(),
                    firmware: "1.0.0".to_string(),
                    device_id: session_id.unwrap_or_else(|| format!("sv1_client_{}", addr.port())),
                })
            },
            ProtocolMessage::Authorize { username, .. } => {
                let request_id = addr.port() as u32;
                let max_target = [0xFFu8; 32];
                
                Ok(ProtocolMessage::OpenStandardMiningChannel {
                    request_id,
                    user_identity: username,
                    nominal_hash_rate: 1000.0,
                    max_target,
                })
            },
            ProtocolMessage::Submit { job_id, nonce, ntime, .. } => {
                let state = self.get_translation_state(addr).await.unwrap_or_default();
                let channel_id = state.channel_id.unwrap_or(1);
                let sv2_job_id = state.job_id_mapping.get(&job_id).copied().unwrap_or(1);
                
                let nonce_u32 = u32::from_str_radix(&nonce, 16)
                    .map_err(|e| Error::Protocol(format!("Invalid nonce hex: {}", e)))?;
                let ntime_u32 = u32::from_str_radix(&ntime, 16)
                    .map_err(|e| Error::Protocol(format!("Invalid ntime hex: {}", e)))?;

                Ok(ProtocolMessage::SubmitSharesStandard {
                    channel_id,
                    sequence_number: state.sequence_counter,
                    job_id: sv2_job_id,
                    nonce: nonce_u32,
                    ntime: ntime_u32,
                    version: 0x20000000,
                })
            },
            _ => Err(Error::Protocol("Unsupported SV1->SV2 translation".to_string())),
        }
    }

    /// Get translation state for an address
    async fn get_translation_state(&self, addr: SocketAddr) -> Option<TranslationState> {
        let states = self.translation_state.read().await;
        states.get(&addr).cloned()
    }

    /// Update translation state for an address
    pub async fn update_translation_state<F>(&self, addr: SocketAddr, updater: F) -> Result<()>
    where
        F: FnOnce(&mut TranslationState),
    {
        let mut states = self.translation_state.write().await;
        let state = states.entry(addr).or_default();
        updater(state);
        Ok(())
    }
}

#[async_trait]
impl ProtocolHandler for ProtocolTranslator {
    async fn detect_protocol(&self, _addr: SocketAddr) -> Result<Protocol> {
        Ok(Protocol::Sv1)
    }

    async fn parse_message(&self, _data: &[u8], protocol: Protocol) -> Result<ProtocolMessage> {
        match protocol {
            Protocol::Sv1 | Protocol::StratumV1 => Ok(ProtocolMessage::Ping),
            Protocol::Sv2 | Protocol::StratumV2 => Ok(ProtocolMessage::Pong),
        }
    }

    async fn serialize_message(&self, _message: ProtocolMessage, _protocol: Protocol) -> Result<Vec<u8>> {
        Ok(vec![])
    }

    async fn translate_message(&self, message: ProtocolMessage, target_protocol: Protocol) -> Result<ProtocolMessage> {
        let addr = "127.0.0.1:0".parse().unwrap();
        match (message.protocol(), target_protocol) {
            (Protocol::Sv1, Protocol::Sv2) => self.translate_sv1_to_sv2(addr, message).await,
            _ => Ok(message),
        }
    }

    fn validate_message(&self, _message: &ProtocolMessage) -> Result<()> {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_protocol_translation_sv1_to_sv2() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        let subscribe = ProtocolMessage::Subscribe {
            user_agent: "test_miner".to_string(),
            session_id: Some("session123".to_string()),
        };

        let translated = translator.translate_sv1_to_sv2(addr, subscribe).await.unwrap();
        match translated {
            ProtocolMessage::SetupConnection { vendor, device_id, .. } => {
                assert_eq!(vendor, "test_miner");
                assert_eq!(device_id, "session123");
            },
            _ => panic!("Expected SetupConnection message"),
        }
    }

    #[tokio::test]
    async fn test_translation_state_management() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        assert!(translator.get_translation_state(addr).await.is_none());

        translator.update_translation_state(addr, |state| {
            state.detected_protocol = Protocol::Sv1;
            state.target_protocol = Protocol::Sv2;
            state.channel_id = Some(123);
        }).await.unwrap();

        let state = translator.get_translation_state(addr).await.unwrap();
        assert_eq!(state.detected_protocol, Protocol::Sv1);
        assert_eq!(state.target_protocol, Protocol::Sv2);
        assert_eq!(state.channel_id, Some(123));
    }
}