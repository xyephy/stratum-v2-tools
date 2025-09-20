use crate::{Result, Protocol, Error};
use async_trait::async_trait;
use std::net::SocketAddr;
use serde_json::{Value, json};
use std::collections::HashMap;
use tokio::net::TcpStream;
use tokio::io::{AsyncBufReadExt, BufReader};
use tracing::{debug, warn};
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

    /// Check if message requires authentication
    pub fn requires_auth(&self) -> bool {
        match self {
            ProtocolMessage::Submit { .. } |
            ProtocolMessage::SubmitSharesStandard { .. } => true,
            _ => false,
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

/// SV1 JSON-RPC message structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sv1Message {
    pub id: Option<Value>,
    pub method: Option<String>,
    pub params: Option<Value>,
    pub result: Option<Value>,
    pub error: Option<Sv1Error>,
}

/// SV1 error structure
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Sv1Error {
    pub code: i32,
    pub message: String,
    pub data: Option<Value>,
}

/// Connection state for SV1 clients
#[derive(Debug, Clone)]
pub struct Sv1ConnectionState {
    pub subscribed: bool,
    pub authorized: bool,
    pub difficulty: f64,
    pub extranonce1: String,
    pub extranonce2_size: u8,
    pub session_id: Option<String>,
    pub user_agent: Option<String>,
    pub authorized_workers: HashMap<String, bool>,
    pub last_job_id: Option<String>,
}

impl Default for Sv1ConnectionState {
    fn default() -> Self {
        Self {
            subscribed: false,
            authorized: false,
            difficulty: 1.0,
            extranonce1: String::new(),
            extranonce2_size: 4,
            session_id: None,
            user_agent: None,
            authorized_workers: HashMap::new(),
            last_job_id: None,
        }
    }
}

/// SV1 protocol handler implementation
pub struct Sv1ProtocolHandler {
    connection_states: Arc<RwLock<HashMap<SocketAddr, Sv1ConnectionState>>>,
}

impl Sv1ProtocolHandler {
    pub fn new() -> Self {
        Self {
            connection_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Parse SV1 JSON-RPC message from raw bytes
    pub async fn parse_sv1_message(&self, data: &[u8]) -> Result<Sv1Message> {
        let text = std::str::from_utf8(data)
            .map_err(|e| Error::Protocol(format!("Invalid UTF-8 in SV1 message: {}", e)))?;
        
        let message: Sv1Message = serde_json::from_str(text)
            .map_err(|e| Error::Protocol(format!("Invalid JSON in SV1 message: {}", e)))?;
        
        debug!("Parsed SV1 message: {:?}", message);
        Ok(message)
    }

    /// Serialize SV1 message to JSON bytes
    pub async fn serialize_sv1_message(&self, message: &Sv1Message) -> Result<Vec<u8>> {
        let json = serde_json::to_string(message)
            .map_err(|e| Error::Protocol(format!("Failed to serialize SV1 message: {}", e)))?;
        
        let mut bytes = json.into_bytes();
        bytes.push(b'\n'); // SV1 messages are line-delimited
        
        debug!("Serialized SV1 message: {} bytes", bytes.len());
        Ok(bytes)
    }

    /// Convert SV1 message to generic ProtocolMessage
    pub async fn sv1_to_protocol_message(&self, sv1_msg: &Sv1Message) -> Result<ProtocolMessage> {
        match sv1_msg.method.as_deref() {
            Some("mining.subscribe") => {
                let params = sv1_msg.params.as_ref()
                    .ok_or_else(|| Error::Protocol("Missing params in subscribe message".to_string()))?;
                
                let user_agent = params.get(0)
                    .and_then(|v| v.as_str())
                    .unwrap_or("unknown")
                    .to_string();
                
                let session_id = params.get(1)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                
                Ok(ProtocolMessage::Subscribe { user_agent, session_id })
            },
            
            Some("mining.authorize") => {
                let params = sv1_msg.params.as_ref()
                    .ok_or_else(|| Error::Protocol("Missing params in authorize message".to_string()))?;
                
                let username = params.get(0)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing username in authorize message".to_string()))?
                    .to_string();
                
                let password = params.get(1)
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                
                Ok(ProtocolMessage::Authorize { username, password })
            },
            
            Some("mining.submit") => {
                let params = sv1_msg.params.as_ref()
                    .ok_or_else(|| Error::Protocol("Missing params in submit message".to_string()))?;
                
                let username = params.get(0)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing username in submit message".to_string()))?
                    .to_string();
                
                let job_id = params.get(1)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing job_id in submit message".to_string()))?
                    .to_string();
                
                let extranonce2 = params.get(2)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing extranonce2 in submit message".to_string()))?
                    .to_string();
                
                let ntime = params.get(3)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing ntime in submit message".to_string()))?
                    .to_string();
                
                let nonce = params.get(4)
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| Error::Protocol("Missing nonce in submit message".to_string()))?
                    .to_string();
                
                Ok(ProtocolMessage::Submit {
                    username,
                    job_id,
                    extranonce2,
                    ntime,
                    nonce,
                })
            },
            
            _ => {
                if sv1_msg.error.is_some() {
                    let error = sv1_msg.error.as_ref().unwrap();
                    Ok(ProtocolMessage::Error {
                        code: error.code as u32,
                        message: error.message.clone(),
                    })
                } else {
                    Err(Error::Protocol(format!("Unknown SV1 method: {:?}", sv1_msg.method)))
                }
            }
        }
    }

    /// Convert ProtocolMessage to SV1 message
    pub async fn protocol_message_to_sv1(&self, msg: &ProtocolMessage, id: Option<Value>) -> Result<Sv1Message> {
        match msg {
            ProtocolMessage::Subscribe { .. } => {
                // This would be a response to a subscribe request
                Ok(Sv1Message {
                    id,
                    method: None,
                    params: None,
                    result: Some(json!([
                        [["mining.set_difficulty", "subscription_id"], ["mining.notify", "subscription_id"]],
                        "extranonce1",
                        4
                    ])),
                    error: None,
                })
            },
            
            ProtocolMessage::Authorize { .. } => {
                // This would be a response to an authorize request
                Ok(Sv1Message {
                    id,
                    method: None,
                    params: None,
                    result: Some(json!(true)),
                    error: None,
                })
            },
            
            ProtocolMessage::Notify {
                job_id,
                prevhash,
                coinb1,
                coinb2,
                merkle_branch,
                version,
                nbits,
                ntime,
                clean_jobs,
            } => {
                Ok(Sv1Message {
                    id: None,
                    method: Some("mining.notify".to_string()),
                    params: Some(json!([
                        job_id,
                        prevhash,
                        coinb1,
                        coinb2,
                        merkle_branch,
                        version,
                        nbits,
                        ntime,
                        clean_jobs
                    ])),
                    result: None,
                    error: None,
                })
            },
            
            ProtocolMessage::Error { code, message } => {
                Ok(Sv1Message {
                    id,
                    method: None,
                    params: None,
                    result: None,
                    error: Some(Sv1Error {
                        code: *code as i32,
                        message: message.clone(),
                        data: None,
                    }),
                })
            },
            
            _ => Err(Error::Protocol(format!("Cannot convert {:?} to SV1 message", msg.message_type()))),
        }
    }

    /// Validate SV1 message structure and content
    pub fn validate_sv1_message(&self, message: &Sv1Message) -> Result<()> {
        // Check for required fields based on message type
        if let Some(method) = &message.method {
            match method.as_str() {
                "mining.subscribe" => {
                    if message.params.is_none() {
                        return Err(Error::Protocol("mining.subscribe requires params".to_string()));
                    }
                },
                "mining.authorize" => {
                    let params = message.params.as_ref()
                        .ok_or_else(|| Error::Protocol("mining.authorize requires params".to_string()))?;
                    
                    if !params.is_array() || params.as_array().unwrap().len() < 2 {
                        return Err(Error::Protocol("mining.authorize requires username and password".to_string()));
                    }
                },
                "mining.submit" => {
                    let params = message.params.as_ref()
                        .ok_or_else(|| Error::Protocol("mining.submit requires params".to_string()))?;
                    
                    if !params.is_array() || params.as_array().unwrap().len() < 5 {
                        return Err(Error::Protocol("mining.submit requires 5 parameters".to_string()));
                    }
                },
                _ => {
                    warn!("Unknown SV1 method: {}", method);
                }
            }
        }

        // Validate JSON-RPC structure
        if message.method.is_some() && message.result.is_some() {
            return Err(Error::Protocol("Message cannot have both method and result".to_string()));
        }

        if message.method.is_none() && message.result.is_none() && message.error.is_none() {
            return Err(Error::Protocol("Message must have method, result, or error".to_string()));
        }

        Ok(())
    }

    /// Get connection state for an address
    pub async fn get_connection_state(&self, addr: SocketAddr) -> Option<Sv1ConnectionState> {
        let states = self.connection_states.read().await;
        states.get(&addr).cloned()
    }

    /// Update connection state for an address
    pub async fn update_connection_state<F>(&self, addr: SocketAddr, updater: F) -> Result<()>
    where
        F: FnOnce(&mut Sv1ConnectionState),
    {
        let mut states = self.connection_states.write().await;
        let state = states.entry(addr).or_default();
        updater(state);
        Ok(())
    }

    /// Remove connection state for an address
    pub async fn remove_connection_state(&self, addr: SocketAddr) {
        let mut states = self.connection_states.write().await;
        states.remove(&addr);
    }

    /// Handle SV1 subscription process
    pub async fn handle_subscribe(&self, addr: SocketAddr, user_agent: String, session_id: Option<String>) -> Result<Sv1Message> {
        self.update_connection_state(addr, |state| {
            state.subscribed = true;
            state.user_agent = Some(user_agent);
            state.session_id = session_id;
            state.extranonce1 = format!("{:08x}", addr.port()); // Simple extranonce1 generation
            state.extranonce2_size = 4;
        }).await?;

        Ok(Sv1Message {
            id: Some(json!(1)),
            method: None,
            params: None,
            result: Some(json!([
                [["mining.set_difficulty", "subscription_id"], ["mining.notify", "subscription_id"]],
                format!("{:08x}", addr.port()),
                4
            ])),
            error: None,
        })
    }

    /// Handle SV1 authorization process
    pub async fn handle_authorize(&self, addr: SocketAddr, username: String, _password: String) -> Result<Sv1Message> {
        self.update_connection_state(addr, |state| {
            state.authorized = true;
            state.authorized_workers.insert(username, true);
        }).await?;

        Ok(Sv1Message {
            id: Some(json!(2)),
            method: None,
            params: None,
            result: Some(json!(true)),
            error: None,
        })
    }

    /// Handle SV1 share submission
    pub async fn handle_submit(&self, addr: SocketAddr, username: String, job_id: String, 
                              extranonce2: String, ntime: String, nonce: String) -> Result<Sv1Message> {
        let state = self.get_connection_state(addr).await
            .ok_or_else(|| Error::Protocol("Connection not found".to_string()))?;

        if !state.authorized {
            return Ok(Sv1Message {
                id: Some(json!(3)),
                method: None,
                params: None,
                result: None,
                error: Some(Sv1Error {
                    code: 24,
                    message: "Unauthorized worker".to_string(),
                    data: None,
                }),
            });
        }

        if !state.authorized_workers.contains_key(&username) {
            return Ok(Sv1Message {
                id: Some(json!(3)),
                method: None,
                params: None,
                result: None,
                error: Some(Sv1Error {
                    code: 24,
                    message: "Unknown worker".to_string(),
                    data: None,
                }),
            });
        }

        // Validate share parameters
        if extranonce2.len() != (state.extranonce2_size as usize * 2) {
            return Ok(Sv1Message {
                id: Some(json!(3)),
                method: None,
                params: None,
                result: None,
                error: Some(Sv1Error {
                    code: 20,
                    message: "Invalid extranonce2 length".to_string(),
                    data: None,
                }),
            });
        }

        // Validate hex strings
        if hex::decode(&extranonce2).is_err() || hex::decode(&ntime).is_err() || hex::decode(&nonce).is_err() {
            return Ok(Sv1Message {
                id: Some(json!(3)),
                method: None,
                params: None,
                result: None,
                error: Some(Sv1Error {
                    code: 20,
                    message: "Invalid hex data".to_string(),
                    data: None,
                }),
            });
        }

        debug!("Valid share submitted by {} for job {}", username, job_id);

        Ok(Sv1Message {
            id: Some(json!(3)),
            method: None,
            params: None,
            result: Some(json!(true)),
            error: None,
        })
    }

    /// Create a mining.notify message
    pub fn create_notify_message(&self, job_id: String, prevhash: String, coinb1: String, 
                                coinb2: String, merkle_branch: Vec<String>, version: String,
                                nbits: String, ntime: String, clean_jobs: bool) -> Sv1Message {
        Sv1Message {
            id: None,
            method: Some("mining.notify".to_string()),
            params: Some(json!([
                job_id,
                prevhash,
                coinb1,
                coinb2,
                merkle_branch,
                version,
                nbits,
                ntime,
                clean_jobs
            ])),
            result: None,
            error: None,
        }
    }

    /// Create a mining.set_difficulty message
    pub fn create_set_difficulty_message(&self, difficulty: f64) -> Sv1Message {
        Sv1Message {
            id: None,
            method: Some("mining.set_difficulty".to_string()),
            params: Some(json!([difficulty])),
            result: None,
            error: None,
        }
    }

    /// Detect if incoming data is SV1 protocol
    pub fn detect_sv1_protocol(&self, data: &[u8]) -> bool {
        // SV1 uses JSON-RPC over TCP with newline delimiters
        if let Ok(text) = std::str::from_utf8(data) {
            // Look for JSON-RPC structure
            if text.trim_start().starts_with('{') && text.contains("method") {
                return true;
            }
            // Also check for common SV1 methods
            if text.contains("mining.subscribe") || text.contains("mining.authorize") || text.contains("mining.submit") {
                return true;
            }
        }
        false
    }
}
#[async_trait]
impl ProtocolHandler for Sv1ProtocolHandler {
    /// Detect protocol version from initial connection
    async fn detect_protocol(&self, _addr: SocketAddr) -> Result<Protocol> {
        // For SV1 handler, we always return SV1
        // In practice, this would be called by a higher-level protocol detector
        Ok(Protocol::Sv1)
    }

    /// Handle protocol-specific message parsing
    async fn parse_message(&self, data: &[u8], protocol: Protocol) -> Result<ProtocolMessage> {
        match protocol {
            Protocol::Sv1 => {
                let sv1_msg = self.parse_sv1_message(data).await?;
                self.validate_sv1_message(&sv1_msg)?;
                self.sv1_to_protocol_message(&sv1_msg).await
            },
            Protocol::Sv2 => {
                Err(Error::Protocol("SV1 handler cannot parse SV2 messages".to_string()))
            }
        }
    }

    /// Serialize message for transmission
    async fn serialize_message(&self, message: ProtocolMessage, protocol: Protocol) -> Result<Vec<u8>> {
        match protocol {
            Protocol::Sv1 => {
                let sv1_msg = self.protocol_message_to_sv1(&message, Some(json!(1))).await?;
                self.serialize_sv1_message(&sv1_msg).await
            },
            Protocol::Sv2 => {
                Err(Error::Protocol("SV1 handler cannot serialize SV2 messages".to_string()))
            }
        }
    }

    /// Translate between SV1 and SV2 protocols (not implemented in SV1 handler)
    async fn translate_message(&self, _message: ProtocolMessage, _target_protocol: Protocol) -> Result<ProtocolMessage> {
        Err(Error::Protocol("Translation not implemented in SV1 handler".to_string()))
    }

    /// Validate protocol message
    fn validate_message(&self, message: &ProtocolMessage) -> Result<()> {
        match message {
            ProtocolMessage::Subscribe { user_agent, .. } => {
                if user_agent.is_empty() {
                    return Err(Error::Protocol("User agent cannot be empty".to_string()));
                }
                Ok(())
            },
            ProtocolMessage::Authorize { username, .. } => {
                if username.is_empty() {
                    return Err(Error::Protocol("Username cannot be empty".to_string()));
                }
                Ok(())
            },
            ProtocolMessage::Submit { username, job_id, extranonce2, ntime, nonce } => {
                if username.is_empty() {
                    return Err(Error::Protocol("Username cannot be empty".to_string()));
                }
                if job_id.is_empty() {
                    return Err(Error::Protocol("Job ID cannot be empty".to_string()));
                }
                if extranonce2.is_empty() {
                    return Err(Error::Protocol("Extranonce2 cannot be empty".to_string()));
                }
                if ntime.is_empty() {
                    return Err(Error::Protocol("Ntime cannot be empty".to_string()));
                }
                if nonce.is_empty() {
                    return Err(Error::Protocol("Nonce cannot be empty".to_string()));
                }
                
                // Validate hex strings
                if hex::decode(extranonce2).is_err() {
                    return Err(Error::Protocol("Invalid extranonce2 hex".to_string()));
                }
                if hex::decode(ntime).is_err() {
                    return Err(Error::Protocol("Invalid ntime hex".to_string()));
                }
                if hex::decode(nonce).is_err() {
                    return Err(Error::Protocol("Invalid nonce hex".to_string()));
                }
                
                Ok(())
            },
            ProtocolMessage::Notify { job_id, prevhash, coinb1, coinb2, merkle_branch, version, nbits, ntime, .. } => {
                if job_id.is_empty() {
                    return Err(Error::Protocol("Job ID cannot be empty".to_string()));
                }
                if prevhash.len() != 64 {
                    return Err(Error::Protocol("Previous hash must be 64 hex characters".to_string()));
                }
                if coinb1.is_empty() || coinb2.is_empty() {
                    return Err(Error::Protocol("Coinbase parts cannot be empty".to_string()));
                }
                if version.len() != 8 {
                    return Err(Error::Protocol("Version must be 8 hex characters".to_string()));
                }
                if nbits.len() != 8 {
                    return Err(Error::Protocol("NBits must be 8 hex characters".to_string()));
                }
                if ntime.len() != 8 {
                    return Err(Error::Protocol("Ntime must be 8 hex characters".to_string()));
                }
                
                // Validate hex strings
                if hex::decode(prevhash).is_err() {
                    return Err(Error::Protocol("Invalid prevhash hex".to_string()));
                }
                if hex::decode(coinb1).is_err() {
                    return Err(Error::Protocol("Invalid coinb1 hex".to_string()));
                }
                if hex::decode(coinb2).is_err() {
                    return Err(Error::Protocol("Invalid coinb2 hex".to_string()));
                }
                for branch in merkle_branch {
                    if branch.len() != 64 || hex::decode(branch).is_err() {
                        return Err(Error::Protocol("Invalid merkle branch hex".to_string()));
                    }
                }
                if hex::decode(version).is_err() {
                    return Err(Error::Protocol("Invalid version hex".to_string()));
                }
                if hex::decode(nbits).is_err() {
                    return Err(Error::Protocol("Invalid nbits hex".to_string()));
                }
                if hex::decode(ntime).is_err() {
                    return Err(Error::Protocol("Invalid ntime hex".to_string()));
                }
                
                Ok(())
            },
            _ => {
                // For SV2 messages or other types, we don't validate in SV1 handler
                Ok(())
            }
        }
    }
}

/// Protocol detector that can identify SV1 vs SV2 protocols
pub struct ProtocolDetector;

impl ProtocolDetector {
    pub fn new() -> Self {
        Self
    }

    /// Detect protocol from initial connection data
    pub async fn detect_protocol_from_data(&self, data: &[u8]) -> Result<Protocol> {
        // SV1 detection: JSON-RPC format
        if let Ok(text) = std::str::from_utf8(data) {
            let trimmed = text.trim();
            if trimmed.starts_with('{') && (trimmed.contains("method") || trimmed.contains("result")) {
                // Try to parse as JSON to confirm
                if serde_json::from_str::<Value>(trimmed).is_ok() {
                    debug!("Detected SV1 protocol from JSON-RPC format");
                    return Ok(Protocol::Sv1);
                }
            }
        }

        // SV2 detection: Binary format with specific headers
        if data.len() >= 6 {
            // SV2 messages start with a specific header format
            // This is a simplified detection - real implementation would be more sophisticated
            let extension_type = data[0];
            let msg_type = data[1];
            
            // Check for common SV2 message types
            if extension_type == 0x00 && (msg_type == 0x00 || msg_type == 0x01 || msg_type == 0x02) {
                debug!("Detected SV2 protocol from binary header");
                return Ok(Protocol::Sv2);
            }
        }

        // Default to SV1 for backward compatibility
        debug!("Defaulting to SV1 protocol");
        Ok(Protocol::Sv1)
    }

    /// Detect protocol from TCP stream by reading initial data
    pub async fn detect_protocol_from_stream(&self, stream: &mut TcpStream) -> Result<Protocol> {
        let mut reader = BufReader::new(stream);
        
        // Try to read the first line/message
        let mut line = String::new();
        match tokio::time::timeout(std::time::Duration::from_secs(5), reader.read_line(&mut line)).await {
            Ok(Ok(0)) => {
                return Err(Error::Connection("Connection closed during protocol detection".to_string()));
            },
            Ok(Ok(_)) => {
                return self.detect_protocol_from_data(line.as_bytes()).await;
            },
            Ok(Err(e)) => {
                return Err(Error::Connection(format!("Error reading from stream: {}", e)));
            },
            Err(_) => {
                return Err(Error::Connection("Timeout during protocol detection".to_string()));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn test_sv1_message_parsing() {
        let handler = Sv1ProtocolHandler::new();
        
        // Test subscribe message
        let subscribe_json = r#"{"id": 1, "method": "mining.subscribe", "params": ["cpuminer/2.5.0", null]}"#;
        let message = handler.parse_sv1_message(subscribe_json.as_bytes()).await.unwrap();
        
        assert_eq!(message.method, Some("mining.subscribe".to_string()));
        assert!(message.params.is_some());
        
        let protocol_msg = handler.sv1_to_protocol_message(&message).await.unwrap();
        match protocol_msg {
            ProtocolMessage::Subscribe { user_agent, .. } => {
                assert_eq!(user_agent, "cpuminer/2.5.0");
            },
            _ => panic!("Expected Subscribe message"),
        }
    }

    #[tokio::test]
    async fn test_sv1_message_validation() {
        let handler = Sv1ProtocolHandler::new();
        
        // Valid message
        let valid_msg = Sv1Message {
            id: Some(json!(1)),
            method: Some("mining.subscribe".to_string()),
            params: Some(json!(["cpuminer/2.5.0"])),
            result: None,
            error: None,
        };
        assert!(handler.validate_sv1_message(&valid_msg).is_ok());
        
        // Invalid message - both method and result
        let invalid_msg = Sv1Message {
            id: Some(json!(1)),
            method: Some("mining.subscribe".to_string()),
            params: Some(json!(["cpuminer/2.5.0"])),
            result: Some(json!(true)),
            error: None,
        };
        assert!(handler.validate_sv1_message(&invalid_msg).is_err());
    }

    #[tokio::test]
    async fn test_sv1_authorize_handling() {
        let handler = Sv1ProtocolHandler::new();
        let addr = "127.0.0.1:3333".parse().unwrap();
        
        let response = handler.handle_authorize(addr, "worker1".to_string(), "password".to_string()).await.unwrap();
        
        assert!(response.result.is_some());
        assert_eq!(response.result.unwrap(), json!(true));
        
        let state = handler.get_connection_state(addr).await.unwrap();
        assert!(state.authorized);
        assert!(state.authorized_workers.contains_key("worker1"));
    }

    #[tokio::test]
    async fn test_sv1_submit_validation() {
        let handler = Sv1ProtocolHandler::new();
        let addr = "127.0.0.1:3333".parse().unwrap();
        
        // First authorize the worker
        handler.handle_authorize(addr, "worker1".to_string(), "password".to_string()).await.unwrap();
        
        // Valid submit
        let response = handler.handle_submit(
            addr,
            "worker1".to_string(),
            "job123".to_string(),
            "12345678".to_string(),
            "deadbeef".to_string(),
            "abcdef01".to_string(),
        ).await.unwrap();
        
        assert!(response.result.is_some());
        assert_eq!(response.result.unwrap(), json!(true));
        
        // Invalid submit - unauthorized worker
        let response = handler.handle_submit(
            addr,
            "worker2".to_string(),
            "job123".to_string(),
            "12345678".to_string(),
            "deadbeef".to_string(),
            "abcdef01".to_string(),
        ).await.unwrap();
        
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap().code, 24);
    }

    #[tokio::test]
    async fn test_protocol_detection() {
        let detector = ProtocolDetector::new();
        
        // Test SV1 detection
        let sv1_data = r#"{"id": 1, "method": "mining.subscribe", "params": []}"#;
        let protocol = detector.detect_protocol_from_data(sv1_data.as_bytes()).await.unwrap();
        assert_eq!(protocol, Protocol::Sv1);
        
        // Test SV2 detection (simplified)
        let sv2_data = [0x00, 0x01, 0x00, 0x04, 0x00, 0x00];
        let protocol = detector.detect_protocol_from_data(&sv2_data).await.unwrap();
        assert_eq!(protocol, Protocol::Sv2);
        
        // Test default fallback
        let unknown_data = b"unknown data";
        let protocol = detector.detect_protocol_from_data(unknown_data).await.unwrap();
        assert_eq!(protocol, Protocol::Sv1);
    }

    #[tokio::test]
    async fn test_message_serialization() {
        let handler = Sv1ProtocolHandler::new();
        
        let notify_msg = ProtocolMessage::Notify {
            job_id: "job123".to_string(),
            prevhash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            coinb1: "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff".to_string(),
            coinb2: "ffffffff0100f2052a01000000434104".to_string(),
            merkle_branch: vec!["abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".to_string()],
            version: "20000000".to_string(),
            nbits: "1d00ffff".to_string(),
            ntime: "504e86b9".to_string(),
            clean_jobs: true,
        };
        
        let serialized = handler.serialize_message(notify_msg, Protocol::Sv1).await.unwrap();
        let text = String::from_utf8(serialized).unwrap();
        
        assert!(text.contains("mining.notify"));
        assert!(text.contains("job123"));
        assert!(text.ends_with('\n'));
    }

    #[tokio::test]
    async fn test_connection_state_management() {
        let handler = Sv1ProtocolHandler::new();
        let addr = "127.0.0.1:3333".parse().unwrap();
        
        // Initially no state
        assert!(handler.get_connection_state(addr).await.is_none());
        
        // Update state
        handler.update_connection_state(addr, |state| {
            state.subscribed = true;
            state.difficulty = 2.0;
        }).await.unwrap();
        
        let state = handler.get_connection_state(addr).await.unwrap();
        assert!(state.subscribed);
        assert_eq!(state.difficulty, 2.0);
        
        // Remove state
        handler.remove_connection_state(addr).await;
        assert!(handler.get_connection_state(addr).await.is_none());
    }

    #[test]
    fn test_protocol_message_validation() {
        let handler = Sv1ProtocolHandler::new();
        
        // Valid subscribe message
        let subscribe = ProtocolMessage::Subscribe {
            user_agent: "test".to_string(),
            session_id: None,
        };
        assert!(handler.validate_message(&subscribe).is_ok());
        
        // Invalid subscribe message
        let invalid_subscribe = ProtocolMessage::Subscribe {
            user_agent: "".to_string(),
            session_id: None,
        };
        assert!(handler.validate_message(&invalid_subscribe).is_err());
        
        // Valid submit message
        let submit = ProtocolMessage::Submit {
            username: "worker1".to_string(),
            job_id: "job123".to_string(),
            extranonce2: "12345678".to_string(),
            ntime: "deadbeef".to_string(),
            nonce: "abcdef01".to_string(),
        };
        assert!(handler.validate_message(&submit).is_ok());
        
        // Invalid submit message - bad hex
        let invalid_submit = ProtocolMessage::Submit {
            username: "worker1".to_string(),
            job_id: "job123".to_string(),
            extranonce2: "invalid_hex".to_string(),
            ntime: "deadbeef".to_string(),
            nonce: "abcdef01".to_string(),
        };
        assert!(handler.validate_message(&invalid_submit).is_err());
    }

    // SV2 Tests
    #[tokio::test]
    async fn test_sv2_message_parsing() {
        let handler = Sv2ProtocolHandler::new();
        
        // Test SetupConnection message
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u16.to_le_bytes()); // protocol
        payload.extend_from_slice(&2u16.to_le_bytes()); // min_version
        payload.extend_from_slice(&2u16.to_le_bytes()); // max_version
        payload.extend_from_slice(&0u32.to_le_bytes()); // flags
        
        // Add strings
        handler.encode_string(&mut payload, "localhost");
        payload.extend_from_slice(&4444u16.to_le_bytes()); // port
        handler.encode_string(&mut payload, "test_vendor");
        handler.encode_string(&mut payload, "1.0");
        handler.encode_string(&mut payload, "1.0.0");
        handler.encode_string(&mut payload, "device123");
        
        let frame = handler.create_sv2_frame(0, 0, payload).unwrap();
        let message = handler.parse_sv2_message(&frame).await.unwrap();
        
        match message {
            ProtocolMessage::SetupConnection { protocol, device_id, .. } => {
                assert_eq!(protocol, 2);
                assert_eq!(device_id, "device123");
            },
            _ => panic!("Expected SetupConnection message"),
        }
    }

    #[tokio::test]
    async fn test_protocol_translation_sv1_to_sv2() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        // Test SV1 Subscribe -> SV2 SetupConnection
        let subscribe = ProtocolMessage::Subscribe {
            user_agent: "test_miner".to_string(),
            session_id: Some("session123".to_string()),
        };

        translator.update_translation_state(addr, |state| {
            state.detected_protocol = Protocol::Sv1;
            state.target_protocol = Protocol::Sv2;
        }).await.unwrap();

        let translated = translator.translate_sv1_to_sv2(addr, subscribe).await.unwrap();
        match translated {
            ProtocolMessage::SetupConnection { vendor, device_id, .. } => {
                assert_eq!(vendor, "test_miner");
                assert_eq!(device_id, "session123");
            },
            _ => panic!("Expected SetupConnection message"),
        }
    }
}

/// SV2 protocol handler implementation using SRI crates
pub struct Sv2ProtocolHandler {
    connection_states: Arc<RwLock<HashMap<SocketAddr, Sv2ConnectionState>>>,
}

/// Connection state for SV2 clients
#[derive(Debug, Clone)]
pub struct Sv2ConnectionState {
    pub setup_complete: bool,
    pub channel_id: Option<u32>,
    pub group_channel_id: Option<u32>,
    pub extranonce_prefix: Vec<u8>,
    pub target: Option<[u8; 32]>,
    pub version: u16,
    pub flags: u32,
    pub device_id: String,
    pub last_job_id: Option<u32>,
    pub sequence_number: u32,
}

impl Default for Sv2ConnectionState {
    fn default() -> Self {
        Self {
            setup_complete: false,
            channel_id: None,
            group_channel_id: None,
            extranonce_prefix: Vec::new(),
            target: None,
            version: 2,
            flags: 0,
            device_id: String::new(),
            last_job_id: None,
            sequence_number: 0,
        }
    }
}

impl Sv2ProtocolHandler {
    pub fn new() -> Self {
        Self {
            connection_states: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Parse SV2 binary message from raw bytes
    pub async fn parse_sv2_message(&self, data: &[u8]) -> Result<ProtocolMessage> {
        if data.len() < 6 {
            return Err(Error::Protocol("SV2 message too short".to_string()));
        }

        let extension_type = data[0];
        let msg_type = data[1];
        let msg_length = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);

        if data.len() < (6 + msg_length as usize) {
            return Err(Error::Protocol("Incomplete SV2 message".to_string()));
        }

        let payload = &data[6..(6 + msg_length as usize)];

        debug!("Parsing SV2 message: ext_type={}, msg_type={}, length={}", 
               extension_type, msg_type, msg_length);

        match (extension_type, msg_type) {
            // Common messages (extension_type = 0)
            (0, 0) => self.parse_setup_connection(payload).await,
            (0, 1) => self.parse_setup_connection_success(payload).await,
            (0, 2) => self.parse_setup_connection_error(payload).await,
            
            // Standard mining messages (extension_type = 0)
            (0, 0x10) => self.parse_open_standard_mining_channel(payload).await,
            (0, 0x11) => self.parse_open_standard_mining_channel_success(payload).await,
            (0, 0x12) => self.parse_open_standard_mining_channel_error(payload).await,
            (0, 0x15) => self.parse_new_mining_job(payload).await,
            (0, 0x1a) => self.parse_submit_shares_standard(payload).await,
            (0, 0x1c) => self.parse_submit_shares_success(payload).await,
            (0, 0x1d) => self.parse_submit_shares_error(payload).await,
            
            // Job negotiation messages (extension_type = 1)
            (1, 0x50) => self.parse_allocate_mining_job_token(payload).await,
            (1, 0x51) => self.parse_allocate_mining_job_token_success(payload).await,
            (1, 0x53) => self.parse_declare_mining_job(payload).await,
            (1, 0x54) => self.parse_declare_mining_job_success(payload).await,
            
            _ => {
                warn!("Unknown SV2 message type: ext_type={}, msg_type={}", extension_type, msg_type);
                Err(Error::Protocol(format!("Unknown SV2 message type: {}/{}", extension_type, msg_type)))
            }
        }
    }

    /// Serialize SV2 message to binary format
    pub async fn serialize_sv2_message(&self, message: &ProtocolMessage) -> Result<Vec<u8>> {
        match message {
            ProtocolMessage::SetupConnection { protocol, min_version, max_version, flags, 
                                             endpoint_host, endpoint_port, vendor, 
                                             hardware_version, firmware, device_id } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&protocol.to_le_bytes());
                payload.extend_from_slice(&min_version.to_le_bytes());
                payload.extend_from_slice(&max_version.to_le_bytes());
                payload.extend_from_slice(&flags.to_le_bytes());
                
                // Encode strings with length prefix
                self.encode_string(&mut payload, endpoint_host);
                payload.extend_from_slice(&endpoint_port.to_le_bytes());
                self.encode_string(&mut payload, vendor);
                self.encode_string(&mut payload, hardware_version);
                self.encode_string(&mut payload, firmware);
                self.encode_string(&mut payload, device_id);
                
                self.create_sv2_frame(0, 0, payload)
            },
            
            ProtocolMessage::SetupConnectionSuccess { used_version, flags } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&used_version.to_le_bytes());
                payload.extend_from_slice(&flags.to_le_bytes());
                
                self.create_sv2_frame(0, 1, payload)
            },
            
            ProtocolMessage::OpenStandardMiningChannel { request_id, user_identity, 
                                                       nominal_hash_rate, max_target } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&request_id.to_le_bytes());
                self.encode_string(&mut payload, user_identity);
                payload.extend_from_slice(&nominal_hash_rate.to_le_bytes());
                payload.extend_from_slice(max_target);
                
                self.create_sv2_frame(0, 0x10, payload)
            },
            
            ProtocolMessage::OpenStandardMiningChannelSuccess { request_id, channel_id, 
                                                               target, extranonce_prefix, 
                                                               group_channel_id } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&request_id.to_le_bytes());
                payload.extend_from_slice(&channel_id.to_le_bytes());
                payload.extend_from_slice(target);
                payload.push(extranonce_prefix.len() as u8);
                payload.extend_from_slice(extranonce_prefix);
                payload.extend_from_slice(&group_channel_id.to_le_bytes());
                
                self.create_sv2_frame(0, 0x11, payload)
            },
            
            ProtocolMessage::NewMiningJob { channel_id, job_id, future_job, version, merkle_path } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&channel_id.to_le_bytes());
                payload.extend_from_slice(&job_id.to_le_bytes());
                payload.push(if *future_job { 1 } else { 0 });
                payload.extend_from_slice(&version.to_le_bytes());
                
                // Encode merkle path
                payload.push(merkle_path.len() as u8);
                for hash in merkle_path {
                    payload.extend_from_slice(hash);
                }
                
                self.create_sv2_frame(0, 0x15, payload)
            },
            
            ProtocolMessage::SubmitSharesStandard { channel_id, sequence_number, job_id, 
                                                   nonce, ntime, version } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&channel_id.to_le_bytes());
                payload.extend_from_slice(&sequence_number.to_le_bytes());
                payload.extend_from_slice(&job_id.to_le_bytes());
                payload.extend_from_slice(&nonce.to_le_bytes());
                payload.extend_from_slice(&ntime.to_le_bytes());
                payload.extend_from_slice(&version.to_le_bytes());
                
                self.create_sv2_frame(0, 0x1a, payload)
            },
            
            ProtocolMessage::Error { code, message } => {
                let mut payload = Vec::new();
                payload.extend_from_slice(&code.to_le_bytes());
                self.encode_string(&mut payload, message);
                
                self.create_sv2_frame(0, 2, payload)
            },
            
            _ => Err(Error::Protocol(format!("Cannot serialize {:?} as SV2 message", message.message_type()))),
        }
    }

    /// Create SV2 frame with header
    fn create_sv2_frame(&self, extension_type: u8, msg_type: u8, payload: Vec<u8>) -> Result<Vec<u8>> {
        let mut frame = Vec::new();
        frame.push(extension_type);
        frame.push(msg_type);
        frame.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        frame.extend_from_slice(&payload);
        
        debug!("Created SV2 frame: {} bytes", frame.len());
        Ok(frame)
    }

    /// Encode string with length prefix
    fn encode_string(&self, buffer: &mut Vec<u8>, s: &str) {
        let bytes = s.as_bytes();
        buffer.push(bytes.len() as u8);
        buffer.extend_from_slice(bytes);
    }

    /// Decode string with length prefix
    fn decode_string(&self, data: &[u8], offset: &mut usize) -> Result<String> {
        if *offset >= data.len() {
            return Err(Error::Protocol("Insufficient data for string length".to_string()));
        }
        
        let len = data[*offset] as usize;
        *offset += 1;
        
        if *offset + len > data.len() {
            return Err(Error::Protocol("Insufficient data for string content".to_string()));
        }
        
        let s = String::from_utf8(data[*offset..*offset + len].to_vec())
            .map_err(|e| Error::Protocol(format!("Invalid UTF-8 in string: {}", e)))?;
        
        *offset += len;
        Ok(s)
    }

    /// Parse SetupConnection message
    async fn parse_setup_connection(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 12 {
            return Err(Error::Protocol("SetupConnection payload too short".to_string()));
        }
        
        let mut offset = 0;
        let protocol = u16::from_le_bytes([payload[offset], payload[offset + 1]]);
        offset += 2;
        let min_version = u16::from_le_bytes([payload[offset], payload[offset + 1]]);
        offset += 2;
        let max_version = u16::from_le_bytes([payload[offset], payload[offset + 1]]);
        offset += 2;
        let flags = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                       payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let endpoint_host = self.decode_string(payload, &mut offset)?;
        let endpoint_port = u16::from_le_bytes([payload[offset], payload[offset + 1]]);
        offset += 2;
        let vendor = self.decode_string(payload, &mut offset)?;
        let hardware_version = self.decode_string(payload, &mut offset)?;
        let firmware = self.decode_string(payload, &mut offset)?;
        let device_id = self.decode_string(payload, &mut offset)?;
        
        Ok(ProtocolMessage::SetupConnection {
            protocol,
            min_version,
            max_version,
            flags,
            endpoint_host,
            endpoint_port,
            vendor,
            hardware_version,
            firmware,
            device_id,
        })
    }

    /// Parse SetupConnectionSuccess message
    async fn parse_setup_connection_success(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 6 {
            return Err(Error::Protocol("SetupConnectionSuccess payload too short".to_string()));
        }
        
        let used_version = u16::from_le_bytes([payload[0], payload[1]]);
        let flags = u32::from_le_bytes([payload[2], payload[3], payload[4], payload[5]]);
        
        Ok(ProtocolMessage::SetupConnectionSuccess {
            used_version,
            flags,
        })
    }

    /// Parse SetupConnectionError message
    async fn parse_setup_connection_error(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 4 {
            return Err(Error::Protocol("SetupConnectionError payload too short".to_string()));
        }
        
        let code = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let mut offset = 4;
        let message = self.decode_string(payload, &mut offset)?;
        
        Ok(ProtocolMessage::Error { code, message })
    }

    /// Parse OpenStandardMiningChannel message
    async fn parse_open_standard_mining_channel(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 40 {
            return Err(Error::Protocol("OpenStandardMiningChannel payload too short".to_string()));
        }
        
        let mut offset = 0;
        let request_id = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                           payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let user_identity = self.decode_string(payload, &mut offset)?;
        
        let nominal_hash_rate = f32::from_le_bytes([payload[offset], payload[offset + 1], 
                                                   payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let mut max_target = [0u8; 32];
        max_target.copy_from_slice(&payload[offset..offset + 32]);
        
        Ok(ProtocolMessage::OpenStandardMiningChannel {
            request_id,
            user_identity,
            nominal_hash_rate,
            max_target,
        })
    }

    /// Parse OpenStandardMiningChannelError message
    async fn parse_open_standard_mining_channel_error(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 8 {
            return Err(Error::Protocol("OpenStandardMiningChannelError payload too short".to_string()));
        }
        
        let request_id = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let code = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let mut offset = 8;
        let message = self.decode_string(payload, &mut offset)?;
        
        debug!("OpenStandardMiningChannelError: request_id={}, code={}, message={}", 
               request_id, code, message);
        
        Ok(ProtocolMessage::Error { code, message })
    }

    /// Parse OpenStandardMiningChannelSuccess message
    async fn parse_open_standard_mining_channel_success(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 45 {
            return Err(Error::Protocol("OpenStandardMiningChannelSuccess payload too short".to_string()));
        }
        
        let mut offset = 0;
        let request_id = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                           payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let channel_id = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                           payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let mut target = [0u8; 32];
        target.copy_from_slice(&payload[offset..offset + 32]);
        offset += 32;
        
        let extranonce_len = payload[offset] as usize;
        offset += 1;
        let extranonce_prefix = payload[offset..offset + extranonce_len].to_vec();
        offset += extranonce_len;
        
        let group_channel_id = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                                 payload[offset + 2], payload[offset + 3]]);
        
        Ok(ProtocolMessage::OpenStandardMiningChannelSuccess {
            request_id,
            channel_id,
            target,
            extranonce_prefix,
            group_channel_id,
        })
    }

    /// Parse NewMiningJob message
    async fn parse_new_mining_job(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 14 {
            return Err(Error::Protocol("NewMiningJob payload too short".to_string()));
        }
        
        let mut offset = 0;
        let channel_id = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                           payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let job_id = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                       payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let future_job = payload[offset] != 0;
        offset += 1;
        
        let version = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                        payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let merkle_path_len = payload[offset] as usize;
        offset += 1;
        
        let mut merkle_path = Vec::new();
        for _ in 0..merkle_path_len {
            if offset + 32 > payload.len() {
                return Err(Error::Protocol("Insufficient data for merkle path".to_string()));
            }
            let mut hash = [0u8; 32];
            hash.copy_from_slice(&payload[offset..offset + 32]);
            merkle_path.push(hash);
            offset += 32;
        }
        
        Ok(ProtocolMessage::NewMiningJob {
            channel_id,
            job_id,
            future_job,
            version,
            merkle_path,
        })
    }

    /// Parse SubmitSharesSuccess message
    async fn parse_submit_shares_success(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 8 {
            return Err(Error::Protocol("SubmitSharesSuccess payload too short".to_string()));
        }
        
        let channel_id = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let sequence_number = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        
        debug!("SubmitSharesSuccess: channel_id={}, sequence_number={}", channel_id, sequence_number);
        
        // Return a generic success message - in practice might have more specific response
        Ok(ProtocolMessage::Pong)
    }

    /// Parse SubmitSharesError message
    async fn parse_submit_shares_error(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 12 {
            return Err(Error::Protocol("SubmitSharesError payload too short".to_string()));
        }
        
        let channel_id = u32::from_le_bytes([payload[0], payload[1], payload[2], payload[3]]);
        let sequence_number = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
        let code = u32::from_le_bytes([payload[8], payload[9], payload[10], payload[11]]);
        let mut offset = 12;
        let message = self.decode_string(payload, &mut offset)?;
        
        debug!("SubmitSharesError: channel_id={}, sequence_number={}, code={}, message={}", 
               channel_id, sequence_number, code, message);
        
        Ok(ProtocolMessage::Error { code, message })
    }

    /// Parse SubmitSharesStandard message
    async fn parse_submit_shares_standard(&self, payload: &[u8]) -> Result<ProtocolMessage> {
        if payload.len() < 24 {
            return Err(Error::Protocol("SubmitSharesStandard payload too short".to_string()));
        }
        
        let mut offset = 0;
        let channel_id = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                           payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let sequence_number = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                                payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let job_id = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                       payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let nonce = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                      payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let ntime = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                      payload[offset + 2], payload[offset + 3]]);
        offset += 4;
        
        let version = u32::from_le_bytes([payload[offset], payload[offset + 1], 
                                        payload[offset + 2], payload[offset + 3]]);
        
        Ok(ProtocolMessage::SubmitSharesStandard {
            channel_id,
            sequence_number,
            job_id,
            nonce,
            ntime,
            version,
        })
    }

    /// Parse AllocateMiningJobToken message (Job Negotiation Protocol)
    async fn parse_allocate_mining_job_token(&self, _payload: &[u8]) -> Result<ProtocolMessage> {
        // Simplified implementation - in practice would parse actual fields
        Ok(ProtocolMessage::Ping) // Placeholder
    }

    /// Parse AllocateMiningJobTokenSuccess message
    async fn parse_allocate_mining_job_token_success(&self, _payload: &[u8]) -> Result<ProtocolMessage> {
        // Simplified implementation - in practice would parse actual fields
        Ok(ProtocolMessage::Pong) // Placeholder
    }

    /// Parse DeclareMiningJob message
    async fn parse_declare_mining_job(&self, _payload: &[u8]) -> Result<ProtocolMessage> {
        // Simplified implementation - in practice would parse actual fields
        Ok(ProtocolMessage::Ping) // Placeholder
    }

    /// Parse DeclareMiningJobSuccess message
    async fn parse_declare_mining_job_success(&self, _payload: &[u8]) -> Result<ProtocolMessage> {
        // Simplified implementation - in practice would parse actual fields
        Ok(ProtocolMessage::Pong) // Placeholder
    }

    /// Get connection state for an address
    pub async fn get_connection_state(&self, addr: SocketAddr) -> Option<Sv2ConnectionState> {
        let states = self.connection_states.read().await;
        states.get(&addr).cloned()
    }

    /// Update connection state for an address
    pub async fn update_connection_state<F>(&self, addr: SocketAddr, updater: F) -> Result<()>
    where
        F: FnOnce(&mut Sv2ConnectionState),
    {
        let mut states = self.connection_states.write().await;
        let state = states.entry(addr).or_default();
        updater(state);
        Ok(())
    }

    /// Remove connection state for an address
    pub async fn remove_connection_state(&self, addr: SocketAddr) {
        let mut states = self.connection_states.write().await;
        states.remove(&addr);
    }

    /// Handle SV2 setup connection process
    pub async fn handle_setup_connection(&self, addr: SocketAddr, protocol: u16, min_version: u16, 
                                       max_version: u16, flags: u32, device_id: String) -> Result<ProtocolMessage> {
        // Validate protocol version
        if protocol != 2 {
            return Ok(ProtocolMessage::Error {
                code: 1,
                message: "Unsupported protocol version".to_string(),
            });
        }

        // Choose version within supported range
        let used_version = std::cmp::min(max_version, 2);
        if used_version < min_version {
            return Ok(ProtocolMessage::Error {
                code: 1,
                message: "No compatible protocol version".to_string(),
            });
        }

        self.update_connection_state(addr, |state| {
            state.setup_complete = true;
            state.version = used_version;
            state.flags = flags;
            state.device_id = device_id;
        }).await?;

        Ok(ProtocolMessage::SetupConnectionSuccess {
            used_version,
            flags,
        })
    }

    /// Handle SV2 channel opening
    pub async fn handle_open_mining_channel(&self, addr: SocketAddr, request_id: u32, 
                                          user_identity: String, nominal_hash_rate: f32, 
                                          max_target: [u8; 32]) -> Result<ProtocolMessage> {
        let channel_id = (addr.port() as u32) << 16 | (request_id & 0xFFFF);
        let group_channel_id = 0; // Simplified - would be assigned by pool
        let target = max_target; // Simplified - would be calculated based on difficulty
        let extranonce_prefix = vec![0x01, 0x02, 0x03, 0x04]; // Simplified

        self.update_connection_state(addr, |state| {
            state.channel_id = Some(channel_id);
            state.group_channel_id = Some(group_channel_id);
            state.target = Some(target);
            state.extranonce_prefix = extranonce_prefix.clone();
        }).await?;

        debug!("Opened mining channel {} for {} with hashrate {}", 
               channel_id, user_identity, nominal_hash_rate);

        Ok(ProtocolMessage::OpenStandardMiningChannelSuccess {
            request_id,
            channel_id,
            target,
            extranonce_prefix,
            group_channel_id,
        })
    }

    /// Handle SV2 share submission
    pub async fn handle_submit_shares(&self, addr: SocketAddr, channel_id: u32, 
                                    sequence_number: u32, job_id: u32, nonce: u32, 
                                    _ntime: u32, _version: u32) -> Result<bool> {
        let state = self.get_connection_state(addr).await
            .ok_or_else(|| Error::Protocol("Connection not found".to_string()))?;

        if !state.setup_complete {
            return Err(Error::Protocol("Connection setup not complete".to_string()));
        }

        if state.channel_id != Some(channel_id) {
            return Err(Error::Protocol("Invalid channel ID".to_string()));
        }

        // Update sequence number
        self.update_connection_state(addr, |state| {
            state.sequence_number = sequence_number;
        }).await?;

        debug!("Received share submission: channel={}, job={}, nonce={:08x}", 
               channel_id, job_id, nonce);

        // Simplified validation - in practice would validate against current job
        Ok(true)
    }

    /// Create a new mining job notification
    pub fn create_mining_job(&self, channel_id: u32, job_id: u32, version: u32, 
                           merkle_path: Vec<[u8; 32]>, future_job: bool) -> ProtocolMessage {
        ProtocolMessage::NewMiningJob {
            channel_id,
            job_id,
            future_job,
            version,
            merkle_path,
        }
    }

    /// Detect if incoming data is SV2 protocol
    pub fn detect_sv2_protocol(&self, data: &[u8]) -> bool {
        // SV2 uses binary format with specific header structure
        if data.len() >= 6 {
            let extension_type = data[0];
            let msg_type = data[1];
            let msg_length = u32::from_le_bytes([data[2], data[3], data[4], data[5]]);
            
            // Check for reasonable message length
            if msg_length > 0 && msg_length < 1024 * 1024 { // Max 1MB message
                // Check for known extension types and message types
                match (extension_type, msg_type) {
                    (0, 0..=2) => return true,      // Common messages
                    (0, 0x10..=0x1f) => return true, // Standard mining messages
                    (1, 0x50..=0x5f) => return true, // Job negotiation messages
                    _ => {}
                }
            }
        }
        false
    }
}

#[async_trait]
impl ProtocolHandler for Sv2ProtocolHandler {
    /// Detect protocol version from initial connection
    async fn detect_protocol(&self, _addr: SocketAddr) -> Result<Protocol> {
        // For SV2 handler, we always return SV2
        Ok(Protocol::Sv2)
    }

    /// Handle protocol-specific message parsing
    async fn parse_message(&self, data: &[u8], protocol: Protocol) -> Result<ProtocolMessage> {
        match protocol {
            Protocol::Sv2 => {
                self.parse_sv2_message(data).await
            },
            Protocol::Sv1 => {
                Err(Error::Protocol("SV2 handler cannot parse SV1 messages".to_string()))
            }
        }
    }

    /// Serialize message for transmission
    async fn serialize_message(&self, message: ProtocolMessage, protocol: Protocol) -> Result<Vec<u8>> {
        match protocol {
            Protocol::Sv2 => {
                self.serialize_sv2_message(&message).await
            },
            Protocol::Sv1 => {
                Err(Error::Protocol("SV2 handler cannot serialize SV1 messages".to_string()))
            }
        }
    }

    /// Translate between SV1 and SV2 protocols (not implemented in SV2 handler)
    async fn translate_message(&self, _message: ProtocolMessage, _target_protocol: Protocol) -> Result<ProtocolMessage> {
        Err(Error::Protocol("Translation not implemented in SV2 handler".to_string()))
    }

    /// Validate protocol message
    fn validate_message(&self, message: &ProtocolMessage) -> Result<()> {
        match message {
            ProtocolMessage::SetupConnection { protocol, min_version, max_version, .. } => {
                if *protocol != 2 {
                    return Err(Error::Protocol("Invalid protocol version".to_string()));
                }
                if min_version > max_version {
                    return Err(Error::Protocol("Min version cannot be greater than max version".to_string()));
                }
                Ok(())
            },
            ProtocolMessage::OpenStandardMiningChannel { nominal_hash_rate, .. } => {
                if *nominal_hash_rate <= 0.0 {
                    return Err(Error::Protocol("Nominal hash rate must be positive".to_string()));
                }
                Ok(())
            },
            ProtocolMessage::SubmitSharesStandard { channel_id, job_id, .. } => {
                if *channel_id == 0 {
                    return Err(Error::Protocol("Channel ID cannot be zero".to_string()));
                }
                if *job_id == 0 {
                    return Err(Error::Protocol("Job ID cannot be zero".to_string()));
                }
                Ok(())
            },
            _ => {
                // For SV1 messages or other types, we don't validate in SV2 handler
                Ok(())
            }
        }
    }
}
        let mut payload = Vec::new();
        payload.extend_from_slice(&2u16.to_le_bytes()); // protocol
        payload.extend_from_slice(&2u16.to_le_bytes()); // min_version
        payload.extend_from_slice(&2u16.to_le_bytes()); // max_version
        payload.extend_from_slice(&0u32.to_le_bytes()); // flags
        
        // Add strings
        handler.encode_string(&mut payload, "localhost");
        payload.extend_from_slice(&4444u16.to_le_bytes()); // port
        handler.encode_string(&mut payload, "test_vendor");
        handler.encode_string(&mut payload, "1.0");
        handler.encode_string(&mut payload, "1.0.0");
        handler.encode_string(&mut payload, "device123");
        
        let frame = handler.create_sv2_frame(0, 0, payload).unwrap();
        let message = handler.parse_sv2_message(&frame).await.unwrap();
        
        match message {
            ProtocolMessage::SetupConnection { protocol, device_id, .. } => {
                assert_eq!(protocol, 2);
                assert_eq!(device_id, "device123");
            },
            _ => panic!("Expected SetupConnection message"),
        }
    }

    #[tokio::test]
    async fn test_sv2_setup_connection_handling() {
        let handler = Sv2ProtocolHandler::new();
        let addr = "127.0.0.1:3333".parse().unwrap();
        
        let response = handler.handle_setup_connection(
            addr, 2, 2, 2, 0, "test_device".to_string()
        ).await.unwrap();
        
        match response {
            ProtocolMessage::SetupConnectionSuccess { used_version, .. } => {
                assert_eq!(used_version, 2);
            },
            _ => panic!("Expected SetupConnectionSuccess"),
        }
        
        let state = handler.get_connection_state(addr).await.unwrap();
        assert!(state.setup_complete);
        assert_eq!(state.device_id, "test_device");
    }

    #[tokio::test]
    async fn test_sv2_mining_channel_handling() {
        let handler = Sv2ProtocolHandler::new();
        let addr = "127.0.0.1:3333".parse().unwrap();
        
        // First setup connection
        handler.handle_setup_connection(addr, 2, 2, 2, 0, "test".to_string()).await.unwrap();
        
        let max_target = [0xFFu8; 32];
        let response = handler.handle_open_mining_channel(
            addr, 1, "worker1".to_string(), 1000.0, max_target
        ).await.unwrap();
        
        match response {
            ProtocolMessage::OpenStandardMiningChannelSuccess { request_id, channel_id, .. } => {
                assert_eq!(request_id, 1);
                assert_ne!(channel_id, 0);
            },
            _ => panic!("Expected OpenStandardMiningChannelSuccess"),
        }
        
        let state = handler.get_connection_state(addr).await.unwrap();
        assert!(state.channel_id.is_some());
    }

    #[tokio::test]
    async fn test_sv2_share_submission() {
        let handler = Sv2ProtocolHandler::new();
        let addr = "127.0.0.1:3333".parse().unwrap();
        
        // Setup connection and channel
        handler.handle_setup_connection(addr, 2, 2, 2, 0, "test".to_string()).await.unwrap();
        let max_target = [0xFFu8; 32];
        let response = handler.handle_open_mining_channel(
            addr, 1, "worker1".to_string(), 1000.0, max_target
        ).await.unwrap();
        
        let channel_id = match response {
            ProtocolMessage::OpenStandardMiningChannelSuccess { channel_id, .. } => channel_id,
            _ => panic!("Expected OpenStandardMiningChannelSuccess"),
        };
        
        // Submit share
        let result = handler.handle_submit_shares(
            addr, channel_id, 1, 123, 0x12345678, 0x504e86b9, 0x20000000
        ).await.unwrap();
        
        assert!(result);
    }

    #[tokio::test]
    async fn test_sv2_protocol_detection() {
        let handler = Sv2ProtocolHandler::new();
        
        // Test SV2 detection
        let sv2_data = [0x00, 0x01, 0x06, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00];
        assert!(handler.detect_sv2_protocol(&sv2_data));
        
        // Test non-SV2 data
        let non_sv2_data = b"GET / HTTP/1.1\r\n";
        assert!(!handler.detect_sv2_protocol(non_sv2_data));
        
        // Test too short data
        let short_data = [0x00, 0x01];
        assert!(!handler.detect_sv2_protocol(&short_data));
    }

    #[tokio::test]
    async fn test_sv2_message_serialization() {
        let handler = Sv2ProtocolHandler::new();
        
        let setup_msg = ProtocolMessage::SetupConnection {
            protocol: 2,
            min_version: 2,
            max_version: 2,
            flags: 0,
            endpoint_host: "localhost".to_string(),
            endpoint_port: 4444,
            vendor: "test".to_string(),
            hardware_version: "1.0".to_string(),
            firmware: "1.0.0".to_string(),
            device_id: "device123".to_string(),
        };
        
        let serialized = handler.serialize_sv2_message(&setup_msg).await.unwrap();
        
        // Should start with SV2 header
        assert_eq!(serialized[0], 0); // extension_type
        assert_eq!(serialized[1], 0); // msg_type
        assert!(serialized.len() > 6); // Has header + payload
        
        // Should be able to parse back
        let parsed = handler.parse_sv2_message(&serialized).await.unwrap();
        match parsed {
            ProtocolMessage::SetupConnection { protocol, device_id, .. } => {
                assert_eq!(protocol, 2);
                assert_eq!(device_id, "device123");
            },
            _ => panic!("Expected SetupConnection message"),
        }
    }

    #[tokio::test]
    async fn test_sv2_message_validation() {
        let handler = Sv2ProtocolHandler::new();
        
        // Valid setup connection
        let valid_setup = ProtocolMessage::SetupConnection {
            protocol: 2,
            min_version: 1,
            max_version: 2,
            flags: 0,
            endpoint_host: "localhost".to_string(),
            endpoint_port: 4444,
            vendor: "test".to_string(),
            hardware_version: "1.0".to_string(),
            firmware: "1.0.0".to_string(),
            device_id: "device123".to_string(),
        };
        assert!(handler.validate_message(&valid_setup).is_ok());
        
        // Invalid setup connection - wrong protocol
        let invalid_setup = ProtocolMessage::SetupConnection {
            protocol: 1,
            min_version: 1,
            max_version: 2,
            flags: 0,
            endpoint_host: "localhost".to_string(),
            endpoint_port: 4444,
            vendor: "test".to_string(),
            hardware_version: "1.0".to_string(),
            firmware: "1.0.0".to_string(),
            device_id: "device123".to_string(),
        };
        assert!(handler.validate_message(&invalid_setup).is_err());
        
        // Invalid setup connection - min > max version
        let invalid_version = ProtocolMessage::SetupConnection {
            protocol: 2,
            min_version: 3,
            max_version: 2,
            flags: 0,
            endpoint_host: "localhost".to_string(),
            endpoint_port: 4444,
            vendor: "test".to_string(),
            hardware_version: "1.0".to_string(),
            firmware: "1.0.0".to_string(),
            device_id: "device123".to_string(),
        };
        assert!(handler.validate_message(&invalid_version).is_err());
    }

    #[tokio::test]
    async fn test_sv2_connection_state_management() {
        let handler = Sv2ProtocolHandler::new();
        let addr = "127.0.0.1:3333".parse().unwrap();
        
        // Initially no state
        assert!(handler.get_connection_state(addr).await.is_none());
        
        // Update state
        handler.update_connection_state(addr, |state| {
            state.setup_complete = true;
            state.channel_id = Some(123);
        }).await.unwrap();
        
        let state = handler.get_connection_state(addr).await.unwrap();
        assert!(state.setup_complete);
        assert_eq!(state.channel_id, Some(123));
        
        // Remove state
        handler.remove_connection_state(addr).await;
        assert!(handler.get_connection_state(addr).await.is_none());
    }

    #[tokio::test]
    async fn test_sv2_job_creation() {
        let handler = Sv2ProtocolHandler::new();
        
        let merkle_path = vec![
            [0x01; 32],
            [0x02; 32],
        ];
        
        let job = handler.create_mining_job(123, 456, 0x20000000, merkle_path.clone(), false);
        
        match job {
            ProtocolMessage::NewMiningJob { channel_id, job_id, future_job, version, merkle_path: path } => {
                assert_eq!(channel_id, 123);
                assert_eq!(job_id, 456);
                assert!(!future_job);
                assert_eq!(version, 0x20000000);
                assert_eq!(path.len(), 2);
            },
            _ => panic!("Expected NewMiningJob message"),
        }
    }/// Protocol
 translator that handles bidirectional translation between SV1 and SV2
pub struct ProtocolTranslator {
    sv1_handler: Sv1ProtocolHandler,
    sv2_handler: Sv2ProtocolHandler,
    detector: ProtocolDetector,
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
    pub extranonce2_size: u8,
    pub difficulty: f64,
    pub last_job_clean: bool,
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
            extranonce2_size: 4,
            difficulty: 1.0,
            last_job_clean: false,
            sequence_counter: 0,
        }
    }
}

impl ProtocolTranslator {
    pub fn new() -> Self {
        Self {
            sv1_handler: Sv1ProtocolHandler::new(),
            sv2_handler: Sv2ProtocolHandler::new(),
            detector: ProtocolDetector::new(),
            translation_state: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Detect protocol from incoming data and set up translation context
    pub async fn detect_and_setup_translation(&self, addr: SocketAddr, data: &[u8]) -> Result<Protocol> {
        let detected_protocol = self.detector.detect_protocol_from_data(data).await?;
        
        // Set up translation state
        let mut states = self.translation_state.write().await;
        let state = states.entry(addr).or_default();
        state.detected_protocol = detected_protocol;
        
        // Default translation: SV1 clients get translated to SV2 upstream, SV2 clients stay SV2
        state.target_protocol = match detected_protocol {
            Protocol::Sv1 => Protocol::Sv2,
            Protocol::Sv2 => Protocol::Sv2,
        };

        debug!("Detected protocol {:?} for {}, target protocol {:?}", 
               detected_protocol, addr, state.target_protocol);

        Ok(detected_protocol)
    }

    /// Translate message from detected protocol to target protocol
    pub async fn translate_message(&self, addr: SocketAddr, message: ProtocolMessage) -> Result<ProtocolMessage> {
        let state = {
            let states = self.translation_state.read().await;
            states.get(&addr).cloned().unwrap_or_default()
        };

        match (state.detected_protocol, state.target_protocol) {
            (Protocol::Sv1, Protocol::Sv2) => self.translate_sv1_to_sv2(addr, message).await,
            (Protocol::Sv2, Protocol::Sv1) => self.translate_sv2_to_sv1(addr, message).await,
            (Protocol::Sv1, Protocol::Sv1) | (Protocol::Sv2, Protocol::Sv2) => Ok(message), // No translation needed
        }
    }

    /// Translate SV1 message to SV2 equivalent
    async fn translate_sv1_to_sv2(&self, addr: SocketAddr, message: ProtocolMessage) -> Result<ProtocolMessage> {
        match message {
            ProtocolMessage::Subscribe { user_agent, session_id } => {
                // SV1 subscribe -> SV2 SetupConnection
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

            ProtocolMessage::Authorize { username, password: _ } => {
                // SV1 authorize -> SV2 OpenStandardMiningChannel
                let request_id = addr.port() as u32;
                let max_target = [0xFFu8; 32]; // Maximum target (minimum difficulty)
                
                self.update_translation_state(addr, |state| {
                    state.extranonce1 = format!("{:08x}", addr.port());
                }).await?;

                Ok(ProtocolMessage::OpenStandardMiningChannel {
                    request_id,
                    user_identity: username,
                    nominal_hash_rate: 1000.0, // Default hashrate
                    max_target,
                })
            },

            ProtocolMessage::Submit { username: _, job_id, extranonce2, ntime, nonce } => {
                // SV1 submit -> SV2 SubmitSharesStandard
                let state = self.get_translation_state(addr).await.unwrap_or_default();
                
                let channel_id = state.channel_id.unwrap_or(1);
                let sv2_job_id = state.job_id_mapping.get(&job_id)
                    .copied()
                    .unwrap_or_else(|| job_id.parse().unwrap_or(1));
                
                // Convert hex strings to u32
                let nonce_u32 = u32::from_str_radix(&nonce, 16)
                    .map_err(|e| Error::Protocol(format!("Invalid nonce hex: {}", e)))?;
                let ntime_u32 = u32::from_str_radix(&ntime, 16)
                    .map_err(|e| Error::Protocol(format!("Invalid ntime hex: {}", e)))?;

                let sequence_number = state.sequence_counter;
                self.update_translation_state(addr, |state| {
                    state.sequence_counter += 1;
                }).await?;

                Ok(ProtocolMessage::SubmitSharesStandard {
                    channel_id,
                    sequence_number,
                    job_id: sv2_job_id,
                    nonce: nonce_u32,
                    ntime: ntime_u32,
                    version: 0x20000000, // Default version
                })
            },

            _ => {
                warn!("Cannot translate SV1 message {:?} to SV2", message.message_type());
                Err(Error::Protocol(format!("Unsupported SV1->SV2 translation for {}", message.message_type())))
            }
        }
    }

    /// Translate SV2 message to SV1 equivalent
    async fn translate_sv2_to_sv1(&self, addr: SocketAddr, message: ProtocolMessage) -> Result<ProtocolMessage> {
        match message {
            ProtocolMessage::SetupConnectionSuccess { used_version: _, flags: _ } => {
                // SV2 SetupConnectionSuccess -> SV1 Subscribe response (handled at higher level)
                Ok(ProtocolMessage::Subscribe {
                    user_agent: "sv2_client".to_string(),
                    session_id: Some(format!("sv2_{}", addr.port())),
                })
            },

            ProtocolMessage::OpenStandardMiningChannelSuccess { 
                request_id: _, 
                channel_id, 
                target: _, 
                extranonce_prefix, 
                group_channel_id: _ 
            } => {
                // SV2 OpenStandardMiningChannelSuccess -> SV1 Authorize response
                self.update_translation_state(addr, |state| {
                    state.channel_id = Some(channel_id);
                    state.extranonce1 = hex::encode(&extranonce_prefix);
                }).await?;

                Ok(ProtocolMessage::Authorize {
                    username: "translated_user".to_string(),
                    password: "".to_string(),
                })
            },

            ProtocolMessage::NewMiningJob { 
                channel_id: _, 
                job_id, 
                future_job: _, 
                version, 
                merkle_path 
            } => {
                // SV2 NewMiningJob -> SV1 Notify
                let sv1_job_id = format!("{:08x}", job_id);
                let state = self.get_translation_state(addr).await.unwrap_or_default();
                
                // Update job mapping
                self.update_translation_state(addr, |state| {
                    state.job_id_mapping.insert(sv1_job_id.clone(), job_id);
                    state.reverse_job_mapping.insert(job_id, sv1_job_id.clone());
                }).await?;

                // Convert merkle path to hex strings
                let merkle_branch: Vec<String> = merkle_path.iter()
                    .map(|hash| hex::encode(hash))
                    .collect();

                Ok(ProtocolMessage::Notify {
                    job_id: sv1_job_id,
                    prevhash: "0000000000000000000000000000000000000000000000000000000000000000".to_string(), // Placeholder
                    coinb1: "01000000010000000000000000000000000000000000000000000000000000000000000000ffffffff".to_string(),
                    coinb2: format!("ffffffff0100f2052a01000000434104{}", state.extranonce1),
                    merkle_branch,
                    version: format!("{:08x}", version),
                    nbits: "1d00ffff".to_string(), // Default difficulty bits
                    ntime: format!("{:08x}", chrono::Utc::now().timestamp() as u32),
                    clean_jobs: state.last_job_clean,
                })
            },

            _ => {
                warn!("Cannot translate SV2 message {:?} to SV1", message.message_type());
                Err(Error::Protocol(format!("Unsupported SV2->SV1 translation for {}", message.message_type())))
            }
        }
    }

    /// Handle protocol-specific parsing with automatic translation
    pub async fn parse_and_translate(&self, addr: SocketAddr, data: &[u8]) -> Result<ProtocolMessage> {
        // First detect protocol if not already known
        let detected_protocol = {
            let states = self.translation_state.read().await;
            states.get(&addr)
                .map(|s| s.detected_protocol)
                .unwrap_or_else(|| {
                    // Auto-detect from data
                    if self.sv1_handler.detect_sv1_protocol(data) {
                        Protocol::Sv1
                    } else if self.sv2_handler.detect_sv2_protocol(data) {
                        Protocol::Sv2
                    } else {
                        Protocol::Sv1 // Default fallback
                    }
                })
        };

        // Parse message using appropriate handler
        let message = match detected_protocol {
            Protocol::Sv1 => {
                self.sv1_handler.parse_message(data, Protocol::Sv1).await?
            },
            Protocol::Sv2 => {
                self.sv2_handler.parse_message(data, Protocol::Sv2).await?
            }
        };

        // Translate if needed
        self.translate_message(addr, message).await
    }

    /// Serialize message with automatic protocol detection and translation
    pub async fn translate_and_serialize(&self, addr: SocketAddr, message: ProtocolMessage, 
                                       target_protocol: Protocol) -> Result<Vec<u8>> {
        // Translate message to target protocol if needed
        let translated_message = if message.protocol() != target_protocol {
            self.translate_message(addr, message).await?
        } else {
            message
        };

        // Serialize using appropriate handler
        match target_protocol {
            Protocol::Sv1 => {
                self.sv1_handler.serialize_message(translated_message, Protocol::Sv1).await
            },
            Protocol::Sv2 => {
                self.sv2_handler.serialize_message(translated_message, Protocol::Sv2).await
            }
        }
    }

    /// Get translation state for an address
    async fn get_translation_state(&self, addr: SocketAddr) -> Option<TranslationState> {
        let states = self.translation_state.read().await;
        states.get(&addr).cloned()
    }

    /// Update translation state for an address
    async fn update_translation_state<F>(&self, addr: SocketAddr, updater: F) -> Result<()>
    where
        F: FnOnce(&mut TranslationState),
    {
        let mut states = self.translation_state.write().await;
        let state = states.entry(addr).or_default();
        updater(state);
        Ok(())
    }

    /// Remove translation state for an address
    pub async fn remove_translation_state(&self, addr: SocketAddr) {
        let mut states = self.translation_state.write().await;
        states.remove(&addr);
    }

    /// Set target protocol for translation
    pub async fn set_target_protocol(&self, addr: SocketAddr, target_protocol: Protocol) -> Result<()> {
        self.update_translation_state(addr, |state| {
            state.target_protocol = target_protocol;
        }).await
    }

    /// Check if translation is needed for a connection
    pub async fn needs_translation(&self, addr: SocketAddr) -> bool {
        if let Some(state) = self.get_translation_state(addr).await {
            state.detected_protocol != state.target_protocol
        } else {
            false
        }
    }

    /// Get supported protocol capabilities for fallback
    pub fn get_fallback_capabilities(&self, protocol: Protocol) -> Vec<String> {
        match protocol {
            Protocol::Sv1 => vec![
                "mining.subscribe".to_string(),
                "mining.authorize".to_string(),
                "mining.submit".to_string(),
                "mining.notify".to_string(),
                "mining.set_difficulty".to_string(),
            ],
            Protocol::Sv2 => vec![
                "SetupConnection".to_string(),
                "OpenStandardMiningChannel".to_string(),
                "NewMiningJob".to_string(),
                "SubmitSharesStandard".to_string(),
            ],
        }
    }

    /// Handle graceful fallback when translation fails
    pub async fn handle_translation_fallback(&self, addr: SocketAddr, 
                                           original_message: ProtocolMessage, 
                                           error: Error) -> Result<ProtocolMessage> {
        warn!("Translation failed for {}: {}. Attempting fallback.", addr, error);

        // Try to provide a graceful error response in the original protocol
        match original_message.protocol() {
            Protocol::Sv1 => {
                Ok(ProtocolMessage::Error {
                    code: 20,
                    message: "Feature not supported in translation mode".to_string(),
                })
            },
            Protocol::Sv2 => {
                Ok(ProtocolMessage::Error {
                    code: 1,
                    message: "Translation error - feature not available".to_string(),
                })
            }
        }
    }

    /// Validate that a message can be translated
    pub fn can_translate(&self, message: &ProtocolMessage, from: Protocol, to: Protocol) -> bool {
        match (from, to) {
            (Protocol::Sv1, Protocol::Sv2) => {
                matches!(message, 
                    ProtocolMessage::Subscribe { .. } |
                    ProtocolMessage::Authorize { .. } |
                    ProtocolMessage::Submit { .. }
                )
            },
            (Protocol::Sv2, Protocol::Sv1) => {
                matches!(message,
                    ProtocolMessage::SetupConnectionSuccess { .. } |
                    ProtocolMessage::OpenStandardMiningChannelSuccess { .. } |
                    ProtocolMessage::NewMiningJob { .. }
                )
            },
            _ => true, // Same protocol, no translation needed
        }
    }

    /// Get translation statistics
    pub async fn get_translation_stats(&self, addr: SocketAddr) -> Option<TranslationStats> {
        let state = self.get_translation_state(addr).await?;
        Some(TranslationStats {
            detected_protocol: state.detected_protocol,
            target_protocol: state.target_protocol,
            job_mappings: state.job_id_mapping.len(),
            sequence_counter: state.sequence_counter,
            has_channel: state.channel_id.is_some(),
        })
    }
}

/// Statistics for protocol translation
#[derive(Debug, Clone)]
pub struct TranslationStats {
    pub detected_protocol: Protocol,
    pub target_protocol: Protocol,
    pub job_mappings: usize,
    pub sequence_counter: u32,
    pub has_channel: bool,
}

#[async_trait]
impl ProtocolHandler for ProtocolTranslator {
    /// Detect protocol version from initial connection
    async fn detect_protocol(&self, addr: SocketAddr) -> Result<Protocol> {
        // Use the internal detector
        self.detector.detect_protocol(addr).await
    }

    /// Handle protocol-specific message parsing with translation
    async fn parse_message(&self, data: &[u8], protocol: Protocol) -> Result<ProtocolMessage> {
        // For the translator, we ignore the protocol hint and auto-detect
        let addr = "127.0.0.1:0".parse().unwrap(); // Placeholder - in real use would come from connection context
        self.parse_and_translate(addr, data).await
    }

    /// Serialize message for transmission with translation
    async fn serialize_message(&self, message: ProtocolMessage, protocol: Protocol) -> Result<Vec<u8>> {
        let addr = "127.0.0.1:0".parse().unwrap(); // Placeholder
        self.translate_and_serialize(addr, message, protocol).await
    }

    /// Translate between SV1 and SV2 protocols
    async fn translate_message(&self, message: ProtocolMessage, target_protocol: Protocol) -> Result<ProtocolMessage> {
        let addr = "127.0.0.1:0".parse().unwrap(); // Placeholder
        
        // Set up temporary translation state
        self.update_translation_state(addr, |state| {
            state.detected_protocol = message.protocol();
            state.target_protocol = target_protocol;
        }).await?;

        self.translate_message(addr, message).await
    }

    /// Validate protocol message
    fn validate_message(&self, message: &ProtocolMessage) -> Result<()> {
        // Use the appropriate handler for validation
        match message.protocol() {
            Protocol::Sv1 => self.sv1_handler.validate_message(message),
            Protocol::Sv2 => self.sv2_handler.validate_message(message),
        }
    }
}    #[t
okio::test]
    async fn test_protocol_translation_sv1_to_sv2() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        // Test SV1 Subscribe -> SV2 SetupConnection
        let subscribe = ProtocolMessage::Subscribe {
            user_agent: "test_miner".to_string(),
            session_id: Some("session123".to_string()),
        };

        translator.update_translation_state(addr, |state| {
            state.detected_protocol = Protocol::Sv1;
            state.target_protocol = Protocol::Sv2;
        }).await.unwrap();

        let translated = translator.translate_sv1_to_sv2(addr, subscribe).await.unwrap();
        match translated {
            ProtocolMessage::SetupConnection { vendor, device_id, .. } => {
                assert_eq!(vendor, "test_miner");
                assert_eq!(device_id, "session123");
            },
            _ => panic!("Expected SetupConnection message"),
        }

        // Test SV1 Authorize -> SV2 OpenStandardMiningChannel
        let authorize = ProtocolMessage::Authorize {
            username: "worker1".to_string(),
            password: "password".to_string(),
        };

        let translated = translator.translate_sv1_to_sv2(addr, authorize).await.unwrap();
        match translated {
            ProtocolMessage::OpenStandardMiningChannel { user_identity, .. } => {
                assert_eq!(user_identity, "worker1");
            },
            _ => panic!("Expected OpenStandardMiningChannel message"),
        }

        // Test SV1 Submit -> SV2 SubmitSharesStandard
        translator.update_translation_state(addr, |state| {
            state.channel_id = Some(123);
            state.job_id_mapping.insert("job456".to_string(), 456);
        }).await.unwrap();

        let submit = ProtocolMessage::Submit {
            username: "worker1".to_string(),
            job_id: "job456".to_string(),
            extranonce2: "12345678".to_string(),
            ntime: "504e86b9".to_string(),
            nonce: "abcdef01".to_string(),
        };

        let translated = translator.translate_sv1_to_sv2(addr, submit).await.unwrap();
        match translated {
            ProtocolMessage::SubmitSharesStandard { channel_id, job_id, nonce, .. } => {
                assert_eq!(channel_id, 123);
                assert_eq!(job_id, 456);
                assert_eq!(nonce, 0xabcdef01);
            },
            _ => panic!("Expected SubmitSharesStandard message"),
        }
    }

    #[tokio::test]
    async fn test_protocol_translation_sv2_to_sv1() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        translator.update_translation_state(addr, |state| {
            state.detected_protocol = Protocol::Sv2;
            state.target_protocol = Protocol::Sv1;
        }).await.unwrap();

        // Test SV2 SetupConnectionSuccess -> SV1 Subscribe response
        let setup_success = ProtocolMessage::SetupConnectionSuccess {
            used_version: 2,
            flags: 0,
        };

        let translated = translator.translate_sv2_to_sv1(addr, setup_success).await.unwrap();
        match translated {
            ProtocolMessage::Subscribe { user_agent, .. } => {
                assert_eq!(user_agent, "sv2_client");
            },
            _ => panic!("Expected Subscribe message"),
        }

        // Test SV2 OpenStandardMiningChannelSuccess -> SV1 Authorize response
        let channel_success = ProtocolMessage::OpenStandardMiningChannelSuccess {
            request_id: 1,
            channel_id: 123,
            target: [0xFFu8; 32],
            extranonce_prefix: vec![0x01, 0x02, 0x03, 0x04],
            group_channel_id: 0,
        };

        let translated = translator.translate_sv2_to_sv1(addr, channel_success).await.unwrap();
        match translated {
            ProtocolMessage::Authorize { username, .. } => {
                assert_eq!(username, "translated_user");
            },
            _ => panic!("Expected Authorize message"),
        }

        // Test SV2 NewMiningJob -> SV1 Notify
        let new_job = ProtocolMessage::NewMiningJob {
            channel_id: 123,
            job_id: 789,
            future_job: false,
            version: 0x20000000,
            merkle_path: vec![[0xAAu8; 32], [0xBBu8; 32]],
        };

        let translated = translator.translate_sv2_to_sv1(addr, new_job).await.unwrap();
        match translated {
            ProtocolMessage::Notify { job_id, merkle_branch, .. } => {
                assert_eq!(job_id, "00000315"); // 789 in hex
                assert_eq!(merkle_branch.len(), 2);
            },
            _ => panic!("Expected Notify message"),
        }
    }

    #[tokio::test]
    async fn test_protocol_detection_and_translation() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        // Test SV1 detection
        let sv1_data = r#"{"id": 1, "method": "mining.subscribe", "params": ["test"]}"#;
        let protocol = translator.detect_and_setup_translation(addr, sv1_data.as_bytes()).await.unwrap();
        assert_eq!(protocol, Protocol::Sv1);

        let state = translator.get_translation_state(addr).await.unwrap();
        assert_eq!(state.detected_protocol, Protocol::Sv1);
        assert_eq!(state.target_protocol, Protocol::Sv2);

        // Test SV2 detection
        let sv2_data = [0x00, 0x01, 0x06, 0x00, 0x00, 0x00, 0x02, 0x00, 0x00, 0x00, 0x00, 0x00];
        let protocol = translator.detect_and_setup_translation(addr, &sv2_data).await.unwrap();
        assert_eq!(protocol, Protocol::Sv2);

        let state = translator.get_translation_state(addr).await.unwrap();
        assert_eq!(state.detected_protocol, Protocol::Sv2);
        assert_eq!(state.target_protocol, Protocol::Sv2);
    }

    #[tokio::test]
    async fn test_translation_capabilities() {
        let translator = ProtocolTranslator::new();

        // Test SV1 capabilities
        let sv1_caps = translator.get_fallback_capabilities(Protocol::Sv1);
        assert!(sv1_caps.contains(&"mining.subscribe".to_string()));
        assert!(sv1_caps.contains(&"mining.authorize".to_string()));
        assert!(sv1_caps.contains(&"mining.submit".to_string()));

        // Test SV2 capabilities
        let sv2_caps = translator.get_fallback_capabilities(Protocol::Sv2);
        assert!(sv2_caps.contains(&"SetupConnection".to_string()));
        assert!(sv2_caps.contains(&"OpenStandardMiningChannel".to_string()));

        // Test translation validation
        let subscribe = ProtocolMessage::Subscribe {
            user_agent: "test".to_string(),
            session_id: None,
        };
        assert!(translator.can_translate(&subscribe, Protocol::Sv1, Protocol::Sv2));

        let setup = ProtocolMessage::SetupConnection {
            protocol: 2,
            min_version: 2,
            max_version: 2,
            flags: 0,
            endpoint_host: "localhost".to_string(),
            endpoint_port: 4444,
            vendor: "test".to_string(),
            hardware_version: "1.0".to_string(),
            firmware: "1.0.0".to_string(),
            device_id: "test".to_string(),
        };
        assert!(!translator.can_translate(&setup, Protocol::Sv2, Protocol::Sv1));
    }

    #[tokio::test]
    async fn test_translation_fallback() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        let unsupported_msg = ProtocolMessage::Ping;
        let error = Error::Protocol("Unsupported translation".to_string());

        let fallback = translator.handle_translation_fallback(addr, unsupported_msg, error).await.unwrap();
        match fallback {
            ProtocolMessage::Error { code, message } => {
                assert_eq!(code, 20);
                assert!(message.contains("not supported"));
            },
            _ => panic!("Expected Error message"),
        }
    }

    #[tokio::test]
    async fn test_translation_state_management() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        // Initially no state
        assert!(translator.get_translation_state(addr).await.is_none());
        assert!(!translator.needs_translation(addr).await);

        // Set up translation state
        translator.update_translation_state(addr, |state| {
            state.detected_protocol = Protocol::Sv1;
            state.target_protocol = Protocol::Sv2;
            state.channel_id = Some(123);
            state.job_id_mapping.insert("job1".to_string(), 1);
        }).await.unwrap();

        assert!(translator.needs_translation(addr).await);

        let stats = translator.get_translation_stats(addr).await.unwrap();
        assert_eq!(stats.detected_protocol, Protocol::Sv1);
        assert_eq!(stats.target_protocol, Protocol::Sv2);
        assert_eq!(stats.job_mappings, 1);
        assert!(stats.has_channel);

        // Remove state
        translator.remove_translation_state(addr).await;
        assert!(translator.get_translation_state(addr).await.is_none());
    }

    #[tokio::test]
    async fn test_job_id_mapping() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        // Set up initial state
        translator.update_translation_state(addr, |state| {
            state.detected_protocol = Protocol::Sv1;
            state.target_protocol = Protocol::Sv2;
            state.channel_id = Some(123);
        }).await.unwrap();

        // Translate SV1 submit with job mapping
        translator.update_translation_state(addr, |state| {
            state.job_id_mapping.insert("abc123".to_string(), 456);
        }).await.unwrap();

        let submit = ProtocolMessage::Submit {
            username: "worker1".to_string(),
            job_id: "abc123".to_string(),
            extranonce2: "12345678".to_string(),
            ntime: "504e86b9".to_string(),
            nonce: "abcdef01".to_string(),
        };

        let translated = translator.translate_sv1_to_sv2(addr, submit).await.unwrap();
        match translated {
            ProtocolMessage::SubmitSharesStandard { job_id, .. } => {
                assert_eq!(job_id, 456);
            },
            _ => panic!("Expected SubmitSharesStandard message"),
        }

        // Test reverse mapping with SV2 -> SV1 job translation
        let new_job = ProtocolMessage::NewMiningJob {
            channel_id: 123,
            job_id: 789,
            future_job: false,
            version: 0x20000000,
            merkle_path: vec![],
        };

        let translated = translator.translate_sv2_to_sv1(addr, new_job).await.unwrap();
        match translated {
            ProtocolMessage::Notify { job_id, .. } => {
                // Should create reverse mapping
                let state = translator.get_translation_state(addr).await.unwrap();
                assert!(state.reverse_job_mapping.contains_key(&789));
                assert_eq!(state.reverse_job_mapping[&789], job_id);
            },
            _ => panic!("Expected Notify message"),
        }
    }

    #[tokio::test]
    async fn test_sequence_number_handling() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        translator.update_translation_state(addr, |state| {
            state.detected_protocol = Protocol::Sv1;
            state.target_protocol = Protocol::Sv2;
            state.channel_id = Some(123);
            state.sequence_counter = 0;
        }).await.unwrap();

        // Submit multiple shares and check sequence numbers
        for i in 0..3 {
            let submit = ProtocolMessage::Submit {
                username: "worker1".to_string(),
                job_id: "job1".to_string(),
                extranonce2: "12345678".to_string(),
                ntime: "504e86b9".to_string(),
                nonce: format!("{:08x}", i),
            };

            let translated = translator.translate_sv1_to_sv2(addr, submit).await.unwrap();
            match translated {
                ProtocolMessage::SubmitSharesStandard { sequence_number, .. } => {
                    assert_eq!(sequence_number, i);
                },
                _ => panic!("Expected SubmitSharesStandard message"),
            }
        }

        // Verify final sequence counter
        let state = translator.get_translation_state(addr).await.unwrap();
        assert_eq!(state.sequence_counter, 3);
    }

    #[tokio::test]
    async fn test_translation_error_handling() {
        let translator = ProtocolTranslator::new();
        let addr = "127.0.0.1:3333".parse().unwrap();

        // Test invalid hex in SV1 submit
        translator.update_translation_state(addr, |state| {
            state.detected_protocol = Protocol::Sv1;
            state.target_protocol = Protocol::Sv2;
            state.channel_id = Some(123);
        }).await.unwrap();

        let invalid_submit = ProtocolMessage::Submit {
            username: "worker1".to_string(),
            job_id: "job1".to_string(),
            extranonce2: "12345678".to_string(),
            ntime: "invalid_hex".to_string(),
            nonce: "abcdef01".to_string(),
        };

        let result = translator.translate_sv1_to_sv2(addr, invalid_submit).await;
        assert!(result.is_err());

        // Test unsupported message translation
        let ping = ProtocolMessage::Ping;
        let result = translator.translate_sv1_to_sv2(addr, ping).await;
        assert!(result.is_err());
    }
}