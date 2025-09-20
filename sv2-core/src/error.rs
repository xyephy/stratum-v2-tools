use thiserror::Error;

/// Core error types for the sv2d system
#[derive(Error, Debug)]
pub enum Error {
    #[error("Configuration error: {0}")]
    Config(String),

    #[error("Protocol error: {0}")]
    Protocol(String),

    #[error("Connection error: {0}")]
    Connection(String),

    #[error("Network error: {0}")]
    Network(String),

    #[error("Mining error: {0}")]
    Mining(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Database migration error: {0}")]
    Migration(#[from] sqlx::migrate::MigrateError),

    #[error("Bitcoin RPC error: {0}")]
    BitcoinRpc(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Invalid share: {0}")]
    InvalidShare(String),

    #[error("Share validation error: {0}")]
    ShareValidation(crate::share_validator::ShareValidationError),

    #[error("UUID error: {0}")]
    Uuid(#[from] uuid::Error),

    #[error("Address parse error: {0}")]
    AddressParse(#[from] std::net::AddrParseError),

    #[error("Bitcoin hash error: {0}")]
    BitcoinHash(#[from] bitcoin::hashes::hex::Error),

    #[error("Bitcoin consensus error: {0}")]
    BitcoinConsensus(#[from] bitcoin::consensus::encode::Error),

    #[error("Template error: {0}")]
    Template(String),

    #[error("Authentication error: {0}")]
    Authentication(String),

    #[error("Authorization error: {0}")]
    Authorization(String),

    #[error("System error: {0}")]
    System(String),

    #[error("Internal error: {0}")]
    Internal(String),

    #[error("Metrics error: {0}")]
    Metrics(String),

    #[error("UTF-8 conversion error: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
}

/// Result type alias for convenience
pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    /// Check if error is recoverable
    pub fn is_recoverable(&self) -> bool {
        match self {
            Error::Connection(_) => true,
            Error::Network(_) => true,
            Error::BitcoinRpc(_) => true,
            Error::Database(_) => true,
            Error::Migration(_) => false,
            Error::Io(_) => true,
            _ => false,
        }
    }

    /// Get error category for metrics
    pub fn category(&self) -> &'static str {
        match self {
            Error::Config(_) => "config",
            Error::Protocol(_) => "protocol",
            Error::Connection(_) => "connection",
            Error::Network(_) => "network",
            Error::Mining(_) => "mining",
            Error::Database(_) => "database",
            Error::Migration(_) => "migration",
            Error::BitcoinRpc(_) => "bitcoin_rpc",
            Error::Io(_) => "io",
            Error::Serialization(_) => "serialization",
            Error::InvalidShare(_) => "invalid_share",
            Error::ShareValidation(_) => "share_validation",
            Error::Uuid(_) => "uuid",
            Error::AddressParse(_) => "address_parse",
            Error::BitcoinHash(_) => "bitcoin_hash",
            Error::BitcoinConsensus(_) => "bitcoin_consensus",
            Error::Template(_) => "template",
            Error::Authentication(_) => "authentication",
            Error::Authorization(_) => "authorization",
            Error::System(_) => "system",
            Error::Internal(_) => "internal",
            Error::Metrics(_) => "metrics",
            Error::Utf8(_) => "utf8",
        }
    }
}

impl From<prometheus::Error> for Error {
    fn from(err: prometheus::Error) -> Self {
        Error::Metrics(err.to_string())
    }
}impl 
Clone for Error {
    fn clone(&self) -> Self {
        match self {
            Error::Config(msg) => Error::Config(msg.clone()),
            Error::Protocol(msg) => Error::Protocol(msg.clone()),
            Error::Connection(msg) => Error::Connection(msg.clone()),
            Error::Network(msg) => Error::Network(msg.clone()),
            Error::Mining(msg) => Error::Mining(msg.clone()),
            Error::Database(_err) => Error::Database(sqlx::Error::PoolClosed), // Simplified clone
            Error::Migration(_) => Error::Migration(sqlx::migrate::MigrateError::VersionMissing(0)), // Simplified clone
            Error::BitcoinRpc(msg) => Error::BitcoinRpc(msg.clone()),
            Error::Io(_) => Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "IO Error")), // Simplified clone
            Error::Serialization(_) => Error::Serialization(serde_json::Error::io(std::io::Error::new(std::io::ErrorKind::Other, "Serialization Error"))), // Simplified clone
            Error::InvalidShare(msg) => Error::InvalidShare(msg.clone()),
            Error::ShareValidation(err) => Error::ShareValidation(err.clone()),
            Error::Uuid(err) => Error::Uuid(err.clone()),
            Error::AddressParse(err) => Error::AddressParse(err.clone()),
            Error::BitcoinHash(_) => Error::BitcoinHash(bitcoin::hashes::hex::Error::InvalidChar(b'x')), // Simplified clone
            Error::BitcoinConsensus(_) => Error::BitcoinConsensus(bitcoin::consensus::encode::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, "Bitcoin Consensus Error"))), // Simplified clone
            Error::Template(msg) => Error::Template(msg.clone()),
            Error::Authentication(msg) => Error::Authentication(msg.clone()),
            Error::Authorization(msg) => Error::Authorization(msg.clone()),
            Error::System(msg) => Error::System(msg.clone()),
            Error::Internal(msg) => Error::Internal(msg.clone()),
            Error::Metrics(msg) => Error::Metrics(msg.clone()),
            Error::Utf8(err) => Error::Utf8(err.clone()),
        }
    }
}