use crate::{Result, Error};
use crate::types::{Protocol, Share, WorkTemplate, Job, ShareSubmission};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Protocol message types for translation between SV1 and SV2
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ProtocolMessage {
    // SV1 Messages
    Sv1Subscribe { id: String, version: String },
    Sv1Authorize { username: String, password: String },
    Sv1Submit { worker: String, job_id: String, nonce: u32 },
    Sv1Notify { job_id: String, difficulty: f64 },

    // SV2 Messages
    Sv2SetupConnection,
    Sv2OpenChannel { channel_id: u32 },
    Sv2SubmitShares { channel_id: u32, shares: Vec<Share> },
    Sv2NewTemplate { template: WorkTemplate },

    // Generic
    Subscribe { id: String, version: String },
    Authorize { username: String, password: String },
    Submit { worker: String, job_id: String, nonce: u32 },
    Error { code: i32, message: String },
    Ok,
}

impl ProtocolMessage {
    pub fn message_type(&self) -> &'static str {
        match self {
            ProtocolMessage::Sv1Subscribe { .. } => "sv1.subscribe",
            ProtocolMessage::Sv1Authorize { .. } => "sv1.authorize",
            ProtocolMessage::Sv1Submit { .. } => "sv1.submit",
            ProtocolMessage::Sv1Notify { .. } => "sv1.notify",
            ProtocolMessage::Sv2SetupConnection => "sv2.setup_connection",
            ProtocolMessage::Sv2OpenChannel { .. } => "sv2.open_channel",
            ProtocolMessage::Sv2SubmitShares { .. } => "sv2.submit_shares",
            ProtocolMessage::Sv2NewTemplate { .. } => "sv2.new_template",
            ProtocolMessage::Subscribe { .. } => "subscribe",
            ProtocolMessage::Authorize { .. } => "authorize",
            ProtocolMessage::Submit { .. } => "submit",
            ProtocolMessage::Error { .. } => "error",
            ProtocolMessage::Ok => "ok",
        }
    }
}

/// Protocol translator for converting between SV1 and SV2
pub struct ProtocolTranslator {
    /// Current protocol mode
    mode: Protocol,
}

impl ProtocolTranslator {
    pub fn new(mode: Protocol) -> Self {
        Self { mode }
    }

    /// Translate a message from one protocol to another
    pub fn translate(&self, message: ProtocolMessage, target: Protocol) -> Result<ProtocolMessage> {
        match (self.mode, target) {
            (Protocol::Sv1, Protocol::Sv2) => self.sv1_to_sv2(message),
            (Protocol::Sv2, Protocol::Sv1) => self.sv2_to_sv1(message),
            _ => Ok(message), // Same protocol, no translation needed
        }
    }

    /// Convert SV1 message to SV2
    fn sv1_to_sv2(&self, message: ProtocolMessage) -> Result<ProtocolMessage> {
        match message {
            ProtocolMessage::Sv1Subscribe { .. } => {
                Ok(ProtocolMessage::Sv2SetupConnection)
            }
            ProtocolMessage::Sv1Submit { job_id, nonce, .. } => {
                // In a real implementation, we would look up the job and create proper shares
                Ok(ProtocolMessage::Sv2SubmitShares {
                    channel_id: 0,
                    shares: vec![],
                })
            }
            _ => Ok(message),
        }
    }

    /// Convert SV2 message to SV1
    fn sv2_to_sv1(&self, message: ProtocolMessage) -> Result<ProtocolMessage> {
        match message {
            ProtocolMessage::Sv2NewTemplate { template } => {
                Ok(ProtocolMessage::Sv1Notify {
                    job_id: template.id.to_string(),
                    difficulty: template.difficulty,
                })
            }
            _ => Ok(message),
        }
    }

    /// Get the current protocol mode
    pub fn mode(&self) -> Protocol {
        self.mode
    }

    /// Set the protocol mode
    pub fn set_mode(&mut self, mode: Protocol) {
        self.mode = mode;
    }
}

impl Default for ProtocolTranslator {
    fn default() -> Self {
        Self::new(Protocol::Sv2)
    }
}

/// Network protocol message (alias)
pub type NetworkProtocolMessage = ProtocolMessage;

/// Stratum message (alias)
pub type StratumMessage = ProtocolMessage;
