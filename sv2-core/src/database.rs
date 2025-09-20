use crate::{Result, Error, ConnectionInfo, Share, WorkTemplate, PerformanceMetrics};
use crate::types::Alert;
use crate::recovery::{DatabaseRecovery, RecoveryConfig};
use sqlx::{Pool, Sqlite, Postgres, Row};
use uuid::Uuid;
use std::sync::Arc;
use tokio::sync::Mutex;

/// Database connection pool enum supporting both SQLite and PostgreSQL
#[derive(Debug, Clone)]
pub enum DatabasePool {
    Sqlite(Pool<Sqlite>),
    Postgres(Pool<Postgres>),
}

/// Database operations trait
#[async_trait::async_trait]
pub trait DatabaseOps: Send + Sync {
    async fn create_connection(&self, conn_info: &ConnectionInfo) -> Result<()>;
    async fn update_connection(&self, conn_info: &ConnectionInfo) -> Result<()>;
    async fn get_connection(&self, id: Uuid) -> Result<Option<ConnectionInfo>>;
    async fn list_connections(&self, limit: Option<u32>) -> Result<Vec<ConnectionInfo>>;
    async fn delete_connection(&self, id: Uuid) -> Result<()>;
    
    async fn create_share(&self, share: &Share) -> Result<()>;
    async fn get_shares(&self, connection_id: Option<Uuid>, limit: Option<u32>) -> Result<Vec<Share>>;
    async fn get_share_stats(&self, connection_id: Option<Uuid>) -> Result<ShareStats>;
    
    async fn create_work_template(&self, template: &WorkTemplate) -> Result<()>;
    async fn get_work_template(&self, id: Uuid) -> Result<Option<WorkTemplate>>;
    async fn list_work_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>>;
    async fn delete_expired_templates(&self) -> Result<u64>;
    
    async fn create_alert(&self, alert: &Alert) -> Result<()>;
    async fn update_alert(&self, alert: &Alert) -> Result<()>;
    async fn get_alerts(&self, resolved: Option<bool>, limit: Option<u32>) -> Result<Vec<Alert>>;
    
    async fn store_performance_metrics(&self, metrics: &PerformanceMetrics) -> Result<()>;
    async fn get_performance_metrics(&self, limit: Option<u32>) -> Result<Vec<PerformanceMetrics>>;
    
    async fn store_config_history(&self, config_data: &str, applied_by: &str) -> Result<()>;
    async fn get_config_history(&self, limit: Option<u32>) -> Result<Vec<ConfigHistoryEntry>>;
    
    // Additional methods needed by solo mode handler
    async fn store_connection(&self, conn: &crate::Connection) -> Result<()>;
    async fn store_share(&self, share: &Share) -> Result<()>;
    async fn store_work_template(&self, template: &WorkTemplate) -> Result<()>;
    async fn update_connection_status(&self, connection_id: Uuid, status: crate::types::ConnectionState) -> Result<()>;
    
    // Methods needed by API server
    async fn get_connection_info(&self, connection_id: Uuid) -> Result<Option<ConnectionInfo>>;
    async fn get_connections(&self, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<ConnectionInfo>>;
    async fn get_work_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>>;
}

/// Share statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ShareStats {
    pub total_shares: u64,
    pub valid_shares: u64,
    pub invalid_shares: u64,
    pub blocks_found: u64,
    pub acceptance_rate: f64,
    pub first_share: Option<chrono::DateTime<chrono::Utc>>,
    pub last_share: Option<chrono::DateTime<chrono::Utc>>,
}

/// Configuration history entry
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ConfigHistoryEntry {
    pub id: i64,
    pub config_data: String,
    pub applied_at: chrono::DateTime<chrono::Utc>,
    pub applied_by: String,
}

/// Database statistics
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct DatabaseStats {
    pub total_connections: u64,
    pub total_shares: u64,
    pub database_size: u64,
}

impl DatabasePool {
    /// Create a new database pool from URL
    pub async fn new(database_url: &str, _max_connections: u32) -> Result<Self> {
        if database_url.starts_with("sqlite:") {
            let pool = sqlx::SqlitePool::connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(database_url.strip_prefix("sqlite://").unwrap_or("sv2d.db"))
                    .create_if_missing(true)
            ).await?;
            
            Ok(DatabasePool::Sqlite(pool))
        } else if database_url.starts_with("postgres:") {
            let pool = sqlx::PgPool::connect(database_url).await?;
            Ok(DatabasePool::Postgres(pool))
        } else {
            Err(Error::Config("Unsupported database URL scheme".to_string()))
        }
    }

    /// Run database migrations
    pub async fn migrate(&self) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::migrate!("./migrations/sqlite").run(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::migrate!("./migrations/postgres").run(pool).await?;
            }
        }
        Ok(())
    }

    /// Check if database is healthy
    pub async fn health_check(&self) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query("SELECT 1").execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query("SELECT 1").execute(pool).await?;
            }
        }
        Ok(())
    }

    /// Get database statistics
    pub async fn get_stats(&self) -> Result<DatabaseStats> {
        match self {
            DatabasePool::Sqlite(pool) => {
                let connections_row = sqlx::query("SELECT COUNT(*) as count FROM connections")
                    .fetch_one(pool)
                    .await?;
                let connections: i64 = connections_row.get("count");
                
                let shares_row = sqlx::query("SELECT COUNT(*) as count FROM shares")
                    .fetch_one(pool)
                    .await?;
                let shares: i64 = shares_row.get("count");
                
                Ok(DatabaseStats {
                    total_connections: connections as u64,
                    total_shares: shares as u64,
                    database_size: 0, // SQLite doesn't easily provide this
                })
            }
            DatabasePool::Postgres(pool) => {
                let connections_row = sqlx::query("SELECT COUNT(*) as count FROM connections")
                    .fetch_one(pool)
                    .await?;
                let connections: i64 = connections_row.get("count");
                
                let shares_row = sqlx::query("SELECT COUNT(*) as count FROM shares")
                    .fetch_one(pool)
                    .await?;
                let shares: i64 = shares_row.get("count");
                
                Ok(DatabaseStats {
                    total_connections: connections as u64,
                    total_shares: shares as u64,
                    database_size: 0, // Would need additional query for PostgreSQL
                })
            }
        }
    }
}

#[async_trait::async_trait]
impl DatabaseOps for DatabasePool {
    async fn create_connection(&self, conn_info: &ConnectionInfo) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO connections (
                        id, address, protocol, state, connected_at, last_activity,
                        user_agent, version, subscribed_difficulty, extranonce1, extranonce2_size,
                        total_shares, valid_shares, invalid_shares, blocks_found
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#
                )
                .bind(conn_info.id.to_string())
                .bind(conn_info.address.to_string())
                .bind(format!("{:?}", conn_info.protocol).to_lowercase())
                .bind(format!("{:?}", conn_info.state))
                .bind(conn_info.connected_at)
                .bind(conn_info.last_activity)
                .bind(&conn_info.user_agent)
                .bind(&conn_info.version)
                .bind(conn_info.subscribed_difficulty)
                .bind(&conn_info.extranonce1)
                .bind(conn_info.extranonce2_size.map(|s| s as i32))
                .bind(conn_info.total_shares as i64)
                .bind(conn_info.valid_shares as i64)
                .bind(conn_info.invalid_shares as i64)
                .bind(conn_info.blocks_found as i64)
                .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO connections (
                        id, address, protocol, state, connected_at, last_activity,
                        user_agent, version, subscribed_difficulty, extranonce1, extranonce2_size,
                        total_shares, valid_shares, invalid_shares, blocks_found
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                    "#
                )
                .bind(conn_info.id)
                .bind(conn_info.address.to_string())
                .bind(format!("{:?}", conn_info.protocol).to_lowercase())
                .bind(format!("{:?}", conn_info.state))
                .bind(conn_info.connected_at)
                .bind(conn_info.last_activity)
                .bind(&conn_info.user_agent)
                .bind(&conn_info.version)
                .bind(conn_info.subscribed_difficulty)
                .bind(&conn_info.extranonce1)
                .bind(conn_info.extranonce2_size.map(|s| s as i16))
                .bind(conn_info.total_shares as i64)
                .bind(conn_info.valid_shares as i64)
                .bind(conn_info.invalid_shares as i64)
                .bind(conn_info.blocks_found as i64)
                .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn update_connection(&self, conn_info: &ConnectionInfo) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    UPDATE connections SET
                        address = ?, protocol = ?, state = ?, last_activity = ?,
                        user_agent = ?, version = ?, subscribed_difficulty = ?,
                        extranonce1 = ?, extranonce2_size = ?, total_shares = ?,
                        valid_shares = ?, invalid_shares = ?, blocks_found = ?
                    WHERE id = ?
                    "#
                )
                .bind(conn_info.address.to_string())
                .bind(format!("{:?}", conn_info.protocol).to_lowercase())
                .bind(format!("{:?}", conn_info.state))
                .bind(conn_info.last_activity)
                .bind(&conn_info.user_agent)
                .bind(&conn_info.version)
                .bind(conn_info.subscribed_difficulty)
                .bind(&conn_info.extranonce1)
                .bind(conn_info.extranonce2_size.map(|s| s as i32))
                .bind(conn_info.total_shares as i64)
                .bind(conn_info.valid_shares as i64)
                .bind(conn_info.invalid_shares as i64)
                .bind(conn_info.blocks_found as i64)
                .bind(conn_info.id.to_string())
                .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    UPDATE connections SET
                        address = $1, protocol = $2, state = $3, last_activity = $4,
                        user_agent = $5, version = $6, subscribed_difficulty = $7,
                        extranonce1 = $8, extranonce2_size = $9, total_shares = $10,
                        valid_shares = $11, invalid_shares = $12, blocks_found = $13
                    WHERE id = $14
                    "#
                )
                .bind(conn_info.address.to_string())
                .bind(format!("{:?}", conn_info.protocol).to_lowercase())
                .bind(format!("{:?}", conn_info.state))
                .bind(conn_info.last_activity)
                .bind(&conn_info.user_agent)
                .bind(&conn_info.version)
                .bind(conn_info.subscribed_difficulty)
                .bind(&conn_info.extranonce1)
                .bind(conn_info.extranonce2_size.map(|s| s as i16))
                .bind(conn_info.total_shares as i64)
                .bind(conn_info.valid_shares as i64)
                .bind(conn_info.invalid_shares as i64)
                .bind(conn_info.blocks_found as i64)
                .bind(conn_info.id)
                .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn get_connection(&self, id: Uuid) -> Result<Option<ConnectionInfo>> {
        match self {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query(
                    "SELECT * FROM connections WHERE id = ?"
                )
                .bind(id.to_string())
                .fetch_optional(pool).await?;

                if let Some(row) = row {
                    Ok(Some(ConnectionInfo {
                        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                        address: row.get::<String, _>("address").parse().map_err(Error::AddressParse)?,
                        protocol: match row.get::<String, _>("protocol").as_str() {
                            "sv1" => crate::types::Protocol::Sv1,
                            "sv2" => crate::types::Protocol::Sv2,
                            _ => return Err(Error::Config("Invalid protocol in database".to_string())),
                        },
                        state: serde_json::from_str(&format!("\"{}\"", row.get::<String, _>("state")))?,
                        connected_at: row.get("connected_at"),
                        last_activity: row.get("last_activity"),
                        user_agent: row.get("user_agent"),
                        version: row.get("version"),
                        subscribed_difficulty: row.get("subscribed_difficulty"),
                        extranonce1: row.get("extranonce1"),
                        extranonce2_size: row.get::<Option<i32>, _>("extranonce2_size").map(|s| s as u8),
                        authorized_workers: Vec::new(), // Not stored in DB for simplicity
                        total_shares: row.get::<i64, _>("total_shares") as u64,
                        valid_shares: row.get::<i64, _>("valid_shares") as u64,
                        invalid_shares: row.get::<i64, _>("invalid_shares") as u64,
                        blocks_found: row.get::<i64, _>("blocks_found") as u64,
                    }))
                } else {
                    Ok(None)
                }
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query(
                    "SELECT * FROM connections WHERE id = $1"
                )
                .bind(id)
                .fetch_optional(pool).await?;

                if let Some(row) = row {
                    Ok(Some(ConnectionInfo {
                        id: row.get("id"),
                        address: row.get::<String, _>("address").parse().map_err(Error::AddressParse)?,
                        protocol: match row.get::<String, _>("protocol").as_str() {
                            "sv1" => crate::types::Protocol::Sv1,
                            "sv2" => crate::types::Protocol::Sv2,
                            _ => return Err(Error::Config("Invalid protocol in database".to_string())),
                        },
                        state: serde_json::from_str(&format!("\"{}\"", row.get::<String, _>("state")))?,
                        connected_at: row.get("connected_at"),
                        last_activity: row.get("last_activity"),
                        user_agent: row.get("user_agent"),
                        version: row.get("version"),
                        subscribed_difficulty: row.get("subscribed_difficulty"),
                        extranonce1: row.get("extranonce1"),
                        extranonce2_size: row.get::<Option<i16>, _>("extranonce2_size").map(|s| s as u8),
                        authorized_workers: Vec::new(), // Not stored in DB for simplicity
                        total_shares: row.get::<i64, _>("total_shares") as u64,
                        valid_shares: row.get::<i64, _>("valid_shares") as u64,
                        invalid_shares: row.get::<i64, _>("invalid_shares") as u64,
                        blocks_found: row.get::<i64, _>("blocks_found") as u64,
                    }))
                } else {
                    Ok(None)
                }
            }
        }
    }

    async fn list_connections(&self, limit: Option<u32>) -> Result<Vec<ConnectionInfo>> {
        let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
        
        match self {
            DatabasePool::Sqlite(pool) => {
                let query = format!("SELECT * FROM connections ORDER BY connected_at DESC {}", limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut connections = Vec::new();
                for row in rows {
                    if let Ok(address) = row.get::<String, _>("address").parse() {
                        connections.push(ConnectionInfo {
                            id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                            address,
                            protocol: match row.get::<String, _>("protocol").as_str() {
                                "sv1" => crate::types::Protocol::Sv1,
                                "sv2" => crate::types::Protocol::Sv2,
                                _ => continue,
                            },
                            state: serde_json::from_str(&format!("\"{}\"", row.get::<String, _>("state"))).unwrap_or(crate::types::ConnectionState::Disconnected),
                            connected_at: row.get("connected_at"),
                            last_activity: row.get("last_activity"),
                            user_agent: row.get("user_agent"),
                            version: row.get("version"),
                            subscribed_difficulty: row.get("subscribed_difficulty"),
                            extranonce1: row.get("extranonce1"),
                            extranonce2_size: row.get::<Option<i32>, _>("extranonce2_size").map(|s| s as u8),
                            authorized_workers: Vec::new(),
                            total_shares: row.get::<i64, _>("total_shares") as u64,
                            valid_shares: row.get::<i64, _>("valid_shares") as u64,
                            invalid_shares: row.get::<i64, _>("invalid_shares") as u64,
                            blocks_found: row.get::<i64, _>("blocks_found") as u64,
                        });
                    }
                }
                Ok(connections)
            }
            DatabasePool::Postgres(pool) => {
                let query = format!("SELECT * FROM connections ORDER BY connected_at DESC {}", limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut connections = Vec::new();
                for row in rows {
                    if let Ok(address) = row.get::<String, _>("address").parse() {
                        connections.push(ConnectionInfo {
                            id: row.get("id"),
                            address,
                            protocol: match row.get::<String, _>("protocol").as_str() {
                                "sv1" => crate::types::Protocol::Sv1,
                                "sv2" => crate::types::Protocol::Sv2,
                                _ => continue,
                            },
                            state: serde_json::from_str(&format!("\"{}\"", row.get::<String, _>("state"))).unwrap_or(crate::types::ConnectionState::Disconnected),
                            connected_at: row.get("connected_at"),
                            last_activity: row.get("last_activity"),
                            user_agent: row.get("user_agent"),
                            version: row.get("version"),
                            subscribed_difficulty: row.get("subscribed_difficulty"),
                            extranonce1: row.get("extranonce1"),
                            extranonce2_size: row.get::<Option<i16>, _>("extranonce2_size").map(|s| s as u8),
                            authorized_workers: Vec::new(),
                            total_shares: row.get::<i64, _>("total_shares") as u64,
                            valid_shares: row.get::<i64, _>("valid_shares") as u64,
                            invalid_shares: row.get::<i64, _>("invalid_shares") as u64,
                            blocks_found: row.get::<i64, _>("blocks_found") as u64,
                        });
                    }
                }
                Ok(connections)
            }
        }
    }

    async fn delete_connection(&self, id: Uuid) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query("DELETE FROM connections WHERE id = ?")
                    .bind(id.to_string())
                    .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query("DELETE FROM connections WHERE id = $1")
                    .bind(id)
                    .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn create_share(&self, share: &Share) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO shares (connection_id, nonce, timestamp, difficulty, is_valid, block_hash, submitted_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                    "#
                )
                .bind(share.connection_id.to_string())
                .bind(share.nonce as i64)
                .bind(share.timestamp as i64)
                .bind(share.difficulty)
                .bind(share.is_valid)
                .bind(share.block_hash.map(|h| h.to_string()))
                .bind(share.submitted_at)
                .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO shares (connection_id, nonce, timestamp, difficulty, is_valid, block_hash, submitted_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    "#
                )
                .bind(share.connection_id)
                .bind(share.nonce as i64)
                .bind(share.timestamp as i64)
                .bind(share.difficulty)
                .bind(share.is_valid)
                .bind(share.block_hash.map(|h| h.to_string()))
                .bind(share.submitted_at)
                .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn get_shares(&self, connection_id: Option<Uuid>, limit: Option<u32>) -> Result<Vec<Share>> {
        let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
        
        match self {
            DatabasePool::Sqlite(pool) => {
                let (query, bind_connection_id) = if let Some(conn_id) = connection_id {
                    (format!("SELECT * FROM shares WHERE connection_id = ? ORDER BY submitted_at DESC {}", limit_clause), Some(conn_id.to_string()))
                } else {
                    (format!("SELECT * FROM shares ORDER BY submitted_at DESC {}", limit_clause), None)
                };
                
                let mut query_builder = sqlx::query(&query);
                if let Some(conn_id) = bind_connection_id {
                    query_builder = query_builder.bind(conn_id);
                }
                
                let rows = query_builder.fetch_all(pool).await?;
                
                let mut shares = Vec::new();
                for row in rows {
                    shares.push(Share {
                        connection_id: Uuid::parse_str(&row.get::<String, _>("connection_id"))?,
                        nonce: row.get::<i64, _>("nonce") as u32,
                        timestamp: row.get::<i64, _>("timestamp") as u32,
                        difficulty: row.get("difficulty"),
                        is_valid: row.get("is_valid"),
                        block_hash: row.get::<Option<String>, _>("block_hash")
                            .map(|s| s.parse().map_err(Error::BitcoinHash))
                            .transpose()?,
                        submitted_at: row.get("submitted_at"),
                    });
                }
                Ok(shares)
            }
            DatabasePool::Postgres(pool) => {
                let (query, bind_connection_id) = if let Some(conn_id) = connection_id {
                    (format!("SELECT * FROM shares WHERE connection_id = $1 ORDER BY submitted_at DESC {}", limit_clause), Some(conn_id))
                } else {
                    (format!("SELECT * FROM shares ORDER BY submitted_at DESC {}", limit_clause), None)
                };
                
                let mut query_builder = sqlx::query(&query);
                if let Some(conn_id) = bind_connection_id {
                    query_builder = query_builder.bind(conn_id);
                }
                
                let rows = query_builder.fetch_all(pool).await?;
                
                let mut shares = Vec::new();
                for row in rows {
                    shares.push(Share {
                        connection_id: row.get("connection_id"),
                        nonce: row.get::<i64, _>("nonce") as u32,
                        timestamp: row.get::<i64, _>("timestamp") as u32,
                        difficulty: row.get("difficulty"),
                        is_valid: row.get("is_valid"),
                        block_hash: row.get::<Option<String>, _>("block_hash")
                            .map(|s| s.parse().map_err(Error::BitcoinHash))
                            .transpose()?,
                        submitted_at: row.get("submitted_at"),
                    });
                }
                Ok(shares)
            }
        }
    }

    async fn get_share_stats(&self, connection_id: Option<Uuid>) -> Result<ShareStats> {
        match self {
            DatabasePool::Sqlite(pool) => {
                let (query, bind_connection_id) = if let Some(conn_id) = connection_id {
                    (
                        r#"
                        SELECT 
                            COUNT(*) as total_shares,
                            SUM(CASE WHEN is_valid = 1 THEN 1 ELSE 0 END) as valid_shares,
                            SUM(CASE WHEN is_valid = 0 THEN 1 ELSE 0 END) as invalid_shares,
                            SUM(CASE WHEN block_hash IS NOT NULL THEN 1 ELSE 0 END) as blocks_found,
                            MIN(submitted_at) as first_share,
                            MAX(submitted_at) as last_share
                        FROM shares WHERE connection_id = ?
                        "#,
                        Some(conn_id.to_string())
                    )
                } else {
                    (
                        r#"
                        SELECT 
                            COUNT(*) as total_shares,
                            SUM(CASE WHEN is_valid = 1 THEN 1 ELSE 0 END) as valid_shares,
                            SUM(CASE WHEN is_valid = 0 THEN 1 ELSE 0 END) as invalid_shares,
                            SUM(CASE WHEN block_hash IS NOT NULL THEN 1 ELSE 0 END) as blocks_found,
                            MIN(submitted_at) as first_share,
                            MAX(submitted_at) as last_share
                        FROM shares
                        "#,
                        None
                    )
                };
                
                let mut query_builder = sqlx::query(&query);
                if let Some(conn_id) = bind_connection_id {
                    query_builder = query_builder.bind(conn_id);
                }
                
                let row = query_builder.fetch_one(pool).await?;
                
                let total_shares: i64 = row.get("total_shares");
                let valid_shares: i64 = row.get("valid_shares");
                let invalid_shares: i64 = row.get("invalid_shares");
                let blocks_found: i64 = row.get("blocks_found");
                
                let acceptance_rate = if total_shares > 0 {
                    (valid_shares as f64 / total_shares as f64) * 100.0
                } else {
                    0.0
                };
                
                Ok(ShareStats {
                    total_shares: total_shares as u64,
                    valid_shares: valid_shares as u64,
                    invalid_shares: invalid_shares as u64,
                    blocks_found: blocks_found as u64,
                    acceptance_rate,
                    first_share: row.get("first_share"),
                    last_share: row.get("last_share"),
                })
            }
            DatabasePool::Postgres(pool) => {
                let (query, bind_connection_id) = if let Some(conn_id) = connection_id {
                    (
                        r#"
                        SELECT 
                            COUNT(*) as total_shares,
                            SUM(CASE WHEN is_valid = true THEN 1 ELSE 0 END) as valid_shares,
                            SUM(CASE WHEN is_valid = false THEN 1 ELSE 0 END) as invalid_shares,
                            SUM(CASE WHEN block_hash IS NOT NULL THEN 1 ELSE 0 END) as blocks_found,
                            MIN(submitted_at) as first_share,
                            MAX(submitted_at) as last_share
                        FROM shares WHERE connection_id = $1
                        "#,
                        Some(conn_id)
                    )
                } else {
                    (
                        r#"
                        SELECT 
                            COUNT(*) as total_shares,
                            SUM(CASE WHEN is_valid = true THEN 1 ELSE 0 END) as valid_shares,
                            SUM(CASE WHEN is_valid = false THEN 1 ELSE 0 END) as invalid_shares,
                            SUM(CASE WHEN block_hash IS NOT NULL THEN 1 ELSE 0 END) as blocks_found,
                            MIN(submitted_at) as first_share,
                            MAX(submitted_at) as last_share
                        FROM shares
                        "#,
                        None
                    )
                };
                
                let mut query_builder = sqlx::query(&query);
                if let Some(conn_id) = bind_connection_id {
                    query_builder = query_builder.bind(conn_id);
                }
                
                let row = query_builder.fetch_one(pool).await?;
                
                let total_shares: i64 = row.get("total_shares");
                let valid_shares: i64 = row.get("valid_shares");
                let invalid_shares: i64 = row.get("invalid_shares");
                let blocks_found: i64 = row.get("blocks_found");
                
                let acceptance_rate = if total_shares > 0 {
                    (valid_shares as f64 / total_shares as f64) * 100.0
                } else {
                    0.0
                };
                
                Ok(ShareStats {
                    total_shares: total_shares as u64,
                    valid_shares: valid_shares as u64,
                    invalid_shares: invalid_shares as u64,
                    blocks_found: blocks_found as u64,
                    acceptance_rate,
                    first_share: row.get("first_share"),
                    last_share: row.get("last_share"),
                })
            }
        }
    }

    async fn create_work_template(&self, template: &WorkTemplate) -> Result<()> {
        let coinbase_bytes = bitcoin::consensus::encode::serialize(&template.coinbase_tx);
        let transactions_bytes = bitcoin::consensus::encode::serialize(&template.transactions);
        
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO work_templates (id, previous_hash, coinbase_tx, transactions, difficulty, timestamp, expires_at)
                    VALUES (?, ?, ?, ?, ?, ?, ?)
                    "#
                )
                .bind(template.id.to_string())
                .bind(template.previous_hash.to_string())
                .bind(coinbase_bytes)
                .bind(transactions_bytes)
                .bind(template.difficulty)
                .bind(template.timestamp as i64)
                .bind(template.expires_at)
                .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO work_templates (id, previous_hash, coinbase_tx, transactions, difficulty, timestamp, expires_at)
                    VALUES ($1, $2, $3, $4, $5, $6, $7)
                    "#
                )
                .bind(template.id)
                .bind(template.previous_hash.to_string())
                .bind(coinbase_bytes)
                .bind(transactions_bytes)
                .bind(template.difficulty)
                .bind(template.timestamp as i64)
                .bind(template.expires_at)
                .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn get_work_template(&self, id: Uuid) -> Result<Option<WorkTemplate>> {
        match self {
            DatabasePool::Sqlite(pool) => {
                let row = sqlx::query("SELECT * FROM work_templates WHERE id = ?")
                    .bind(id.to_string())
                    .fetch_optional(pool).await?;
                
                if let Some(row) = row {
                    let coinbase_bytes: Vec<u8> = row.get("coinbase_tx");
                    let transactions_bytes: Vec<u8> = row.get("transactions");
                    
                    Ok(Some(WorkTemplate {
                        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                        previous_hash: row.get::<String, _>("previous_hash").parse().map_err(Error::BitcoinHash)?,
                        coinbase_tx: bitcoin::consensus::encode::deserialize(&coinbase_bytes).map_err(Error::BitcoinConsensus)?,
                        transactions: bitcoin::consensus::encode::deserialize(&transactions_bytes).map_err(Error::BitcoinConsensus)?,
                        difficulty: row.get("difficulty"),
                        timestamp: row.get::<i64, _>("timestamp") as u32,
                        expires_at: row.get("expires_at"),
                    }))
                } else {
                    Ok(None)
                }
            }
            DatabasePool::Postgres(pool) => {
                let row = sqlx::query("SELECT * FROM work_templates WHERE id = $1")
                    .bind(id)
                    .fetch_optional(pool).await?;
                
                if let Some(row) = row {
                    let coinbase_bytes: Vec<u8> = row.get("coinbase_tx");
                    let transactions_bytes: Vec<u8> = row.get("transactions");
                    
                    Ok(Some(WorkTemplate {
                        id: row.get("id"),
                        previous_hash: row.get::<String, _>("previous_hash").parse().map_err(Error::BitcoinHash)?,
                        coinbase_tx: bitcoin::consensus::encode::deserialize(&coinbase_bytes).map_err(Error::BitcoinConsensus)?,
                        transactions: bitcoin::consensus::encode::deserialize(&transactions_bytes).map_err(Error::BitcoinConsensus)?,
                        difficulty: row.get("difficulty"),
                        timestamp: row.get::<i64, _>("timestamp") as u32,
                        expires_at: row.get("expires_at"),
                    }))
                } else {
                    Ok(None)
                }
            }
        }
    }

    async fn list_work_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>> {
        let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
        
        match self {
            DatabasePool::Sqlite(pool) => {
                let query = format!("SELECT * FROM work_templates ORDER BY created_at DESC {}", limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut templates = Vec::new();
                for row in rows {
                    let coinbase_bytes: Vec<u8> = row.get("coinbase_tx");
                    let transactions_bytes: Vec<u8> = row.get("transactions");
                    
                    templates.push(WorkTemplate {
                        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                        previous_hash: row.get::<String, _>("previous_hash").parse().map_err(Error::BitcoinHash)?,
                        coinbase_tx: bitcoin::consensus::encode::deserialize(&coinbase_bytes).map_err(Error::BitcoinConsensus)?,
                        transactions: bitcoin::consensus::encode::deserialize(&transactions_bytes).map_err(Error::BitcoinConsensus)?,
                        difficulty: row.get("difficulty"),
                        timestamp: row.get::<i64, _>("timestamp") as u32,
                        expires_at: row.get("expires_at"),
                    });
                }
                Ok(templates)
            }
            DatabasePool::Postgres(pool) => {
                let query = format!("SELECT * FROM work_templates ORDER BY created_at DESC {}", limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut templates = Vec::new();
                for row in rows {
                    let coinbase_bytes: Vec<u8> = row.get("coinbase_tx");
                    let transactions_bytes: Vec<u8> = row.get("transactions");
                    
                    templates.push(WorkTemplate {
                        id: row.get("id"),
                        previous_hash: row.get::<String, _>("previous_hash").parse().map_err(Error::BitcoinHash)?,
                        coinbase_tx: bitcoin::consensus::encode::deserialize(&coinbase_bytes).map_err(Error::BitcoinConsensus)?,
                        transactions: bitcoin::consensus::encode::deserialize(&transactions_bytes).map_err(Error::BitcoinConsensus)?,
                        difficulty: row.get("difficulty"),
                        timestamp: row.get::<i64, _>("timestamp") as u32,
                        expires_at: row.get("expires_at"),
                    });
                }
                Ok(templates)
            }
        }
    }

    async fn delete_expired_templates(&self) -> Result<u64> {
        match self {
            DatabasePool::Sqlite(pool) => {
                let result = sqlx::query("DELETE FROM work_templates WHERE expires_at < datetime('now')")
                    .execute(pool).await?;
                Ok(result.rows_affected())
            }
            DatabasePool::Postgres(pool) => {
                let result = sqlx::query("DELETE FROM work_templates WHERE expires_at < NOW()")
                    .execute(pool).await?;
                Ok(result.rows_affected())
            }
        }
    }

    async fn create_alert(&self, alert: &Alert) -> Result<()> {
        let metadata_json = serde_json::to_string(&alert.metadata)?;
        
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO alerts (id, level, title, message, component, created_at, resolved_at, metadata)
                    VALUES (?, ?, ?, ?, ?, ?, ?, ?)
                    "#
                )
                .bind(alert.id.to_string())
                .bind(format!("{:?}", alert.level))
                .bind(&alert.title)
                .bind(&alert.message)
                .bind(&alert.component)
                .bind(alert.created_at)
                .bind(alert.resolved_at)
                .bind(metadata_json)
                .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO alerts (id, level, title, message, component, created_at, resolved_at, metadata)
                    VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                    "#
                )
                .bind(alert.id)
                .bind(format!("{:?}", alert.level))
                .bind(&alert.title)
                .bind(&alert.message)
                .bind(&alert.component)
                .bind(alert.created_at)
                .bind(alert.resolved_at)
                .bind(serde_json::Value::Object(alert.metadata.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect()))
                .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn update_alert(&self, alert: &Alert) -> Result<()> {
        let metadata_json = serde_json::to_string(&alert.metadata)?;
        
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    UPDATE alerts SET
                        level = ?, title = ?, message = ?, component = ?,
                        resolved_at = ?, metadata = ?
                    WHERE id = ?
                    "#
                )
                .bind(format!("{:?}", alert.level))
                .bind(&alert.title)
                .bind(&alert.message)
                .bind(&alert.component)
                .bind(alert.resolved_at)
                .bind(metadata_json)
                .bind(alert.id.to_string())
                .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    UPDATE alerts SET
                        level = $1, title = $2, message = $3, component = $4,
                        resolved_at = $5, metadata = $6
                    WHERE id = $7
                    "#
                )
                .bind(format!("{:?}", alert.level))
                .bind(&alert.title)
                .bind(&alert.message)
                .bind(&alert.component)
                .bind(alert.resolved_at)
                .bind(serde_json::Value::Object(alert.metadata.iter().map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone()))).collect()))
                .bind(alert.id)
                .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn get_alerts(&self, resolved: Option<bool>, limit: Option<u32>) -> Result<Vec<Alert>> {
        let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
        
        let where_clause = match resolved {
            Some(true) => "WHERE resolved_at IS NOT NULL",
            Some(false) => "WHERE resolved_at IS NULL",
            None => "",
        };
        
        match self {
            DatabasePool::Sqlite(pool) => {
                let query = format!("SELECT * FROM alerts {} ORDER BY created_at DESC {}", where_clause, limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut alerts = Vec::new();
                for row in rows {
                    let metadata_str: String = row.get("metadata");
                    let metadata: std::collections::HashMap<String, String> = serde_json::from_str(&metadata_str).unwrap_or_default();
                    
                    alerts.push(Alert {
                        id: Uuid::parse_str(&row.get::<String, _>("id"))?,
                        level: match row.get::<String, _>("level").as_str() {
                            "Info" => crate::types::AlertLevel::Info,
                            "Warning" => crate::types::AlertLevel::Warning,
                            "Error" => crate::types::AlertLevel::Error,
                            "Critical" => crate::types::AlertLevel::Critical,
                            _ => crate::types::AlertLevel::Info,
                        },
                        title: row.get("title"),
                        message: row.get("message"),
                        component: row.get("component"),
                        created_at: row.get("created_at"),
                        resolved_at: row.get("resolved_at"),
                        metadata,
                    });
                }
                Ok(alerts)
            }
            DatabasePool::Postgres(pool) => {
                let query = format!("SELECT * FROM alerts {} ORDER BY created_at DESC {}", where_clause, limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut alerts = Vec::new();
                for row in rows {
                    let metadata_json: serde_json::Value = row.get("metadata");
                    let metadata: std::collections::HashMap<String, String> = metadata_json
                        .as_object()
                        .map(|obj| obj.iter().map(|(k, v)| (k.clone(), v.as_str().unwrap_or("").to_string())).collect())
                        .unwrap_or_default();
                    
                    alerts.push(Alert {
                        id: row.get("id"),
                        level: match row.get::<String, _>("level").as_str() {
                            "Info" => crate::types::AlertLevel::Info,
                            "Warning" => crate::types::AlertLevel::Warning,
                            "Error" => crate::types::AlertLevel::Error,
                            "Critical" => crate::types::AlertLevel::Critical,
                            _ => crate::types::AlertLevel::Info,
                        },
                        title: row.get("title"),
                        message: row.get("message"),
                        component: row.get("component"),
                        created_at: row.get("created_at"),
                        resolved_at: row.get("resolved_at"),
                        metadata,
                    });
                }
                Ok(alerts)
            }
        }
    }

    async fn store_performance_metrics(&self, metrics: &PerformanceMetrics) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO performance_metrics (
                        cpu_usage, memory_usage, memory_total, network_rx_bytes, network_tx_bytes,
                        disk_usage, disk_total, open_connections, database_connections, timestamp
                    ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
                    "#
                )
                .bind(metrics.cpu_usage)
                .bind(metrics.memory_usage as i64)
                .bind(metrics.memory_total as i64)
                .bind(metrics.network_rx_bytes as i64)
                .bind(metrics.network_tx_bytes as i64)
                .bind(metrics.disk_usage as i64)
                .bind(metrics.disk_total as i64)
                .bind(metrics.open_connections as i64)
                .bind(metrics.database_connections as i32)
                .bind(metrics.timestamp)
                .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    r#"
                    INSERT INTO performance_metrics (
                        cpu_usage, memory_usage, memory_total, network_rx_bytes, network_tx_bytes,
                        disk_usage, disk_total, open_connections, database_connections, timestamp
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                    "#
                )
                .bind(metrics.cpu_usage)
                .bind(metrics.memory_usage as i64)
                .bind(metrics.memory_total as i64)
                .bind(metrics.network_rx_bytes as i64)
                .bind(metrics.network_tx_bytes as i64)
                .bind(metrics.disk_usage as i64)
                .bind(metrics.disk_total as i64)
                .bind(metrics.open_connections as i64)
                .bind(metrics.database_connections as i32)
                .bind(metrics.timestamp)
                .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn get_performance_metrics(&self, limit: Option<u32>) -> Result<Vec<PerformanceMetrics>> {
        let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
        
        match self {
            DatabasePool::Sqlite(pool) => {
                let query = format!("SELECT * FROM performance_metrics ORDER BY timestamp DESC {}", limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut metrics = Vec::new();
                for row in rows {
                    metrics.push(PerformanceMetrics {
                        cpu_usage: row.get("cpu_usage"),
                        memory_usage: row.get::<i64, _>("memory_usage") as u64,
                        memory_total: row.get::<i64, _>("memory_total") as u64,
                        network_rx_bytes: row.get::<i64, _>("network_rx_bytes") as u64,
                        network_tx_bytes: row.get::<i64, _>("network_tx_bytes") as u64,
                        disk_usage: row.get::<i64, _>("disk_usage") as u64,
                        disk_total: row.get::<i64, _>("disk_total") as u64,
                        open_connections: row.get::<i64, _>("open_connections") as u64,
                        database_connections: row.get::<i32, _>("database_connections") as u32,
                        timestamp: row.get("timestamp"),
                    });
                }
                Ok(metrics)
            }
            DatabasePool::Postgres(pool) => {
                let query = format!("SELECT * FROM performance_metrics ORDER BY timestamp DESC {}", limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut metrics = Vec::new();
                for row in rows {
                    metrics.push(PerformanceMetrics {
                        cpu_usage: row.get("cpu_usage"),
                        memory_usage: row.get::<i64, _>("memory_usage") as u64,
                        memory_total: row.get::<i64, _>("memory_total") as u64,
                        network_rx_bytes: row.get::<i64, _>("network_rx_bytes") as u64,
                        network_tx_bytes: row.get::<i64, _>("network_tx_bytes") as u64,
                        disk_usage: row.get::<i64, _>("disk_usage") as u64,
                        disk_total: row.get::<i64, _>("disk_total") as u64,
                        open_connections: row.get::<i64, _>("open_connections") as u64,
                        database_connections: row.get::<i32, _>("database_connections") as u32,
                        timestamp: row.get("timestamp"),
                    });
                }
                Ok(metrics)
            }
        }
    }

    async fn store_config_history(&self, config_data: &str, applied_by: &str) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query(
                    "INSERT INTO config_history (config_data, applied_by) VALUES (?, ?)"
                )
                .bind(config_data)
                .bind(applied_by)
                .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query(
                    "INSERT INTO config_history (config_data, applied_by) VALUES ($1, $2)"
                )
                .bind(config_data)
                .bind(applied_by)
                .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn get_config_history(&self, limit: Option<u32>) -> Result<Vec<ConfigHistoryEntry>> {
        let limit_clause = limit.map(|l| format!("LIMIT {}", l)).unwrap_or_default();
        
        match self {
            DatabasePool::Sqlite(pool) => {
                let query = format!("SELECT * FROM config_history ORDER BY applied_at DESC {}", limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut entries = Vec::new();
                for row in rows {
                    entries.push(ConfigHistoryEntry {
                        id: row.get::<i64, _>("id"),
                        config_data: row.get("config_data"),
                        applied_at: row.get("applied_at"),
                        applied_by: row.get("applied_by"),
                    });
                }
                Ok(entries)
            }
            DatabasePool::Postgres(pool) => {
                let query = format!("SELECT * FROM config_history ORDER BY applied_at DESC {}", limit_clause);
                let rows = sqlx::query(&query).fetch_all(pool).await?;
                
                let mut entries = Vec::new();
                for row in rows {
                    entries.push(ConfigHistoryEntry {
                        id: row.get::<i64, _>("id"),
                        config_data: row.get("config_data"),
                        applied_at: row.get("applied_at"),
                        applied_by: row.get("applied_by"),
                    });
                }
                Ok(entries)
            }
        }
    }
    
    // Additional methods needed by solo mode handler
    async fn store_connection(&self, conn: &crate::Connection) -> Result<()> {
        let conn_info = ConnectionInfo::from_connection(conn);
        self.create_connection(&conn_info).await
    }
    
    async fn store_share(&self, share: &Share) -> Result<()> {
        self.create_share(share).await
    }
    
    async fn store_work_template(&self, template: &WorkTemplate) -> Result<()> {
        self.create_work_template(template).await
    }
    
    async fn update_connection_status(&self, connection_id: Uuid, status: crate::types::ConnectionState) -> Result<()> {
        match self {
            DatabasePool::Sqlite(pool) => {
                sqlx::query("UPDATE connections SET state = ?, last_activity = ? WHERE id = ?")
                    .bind(format!("{:?}", status))
                    .bind(chrono::Utc::now())
                    .bind(connection_id.to_string())
                    .execute(pool).await?;
            }
            DatabasePool::Postgres(pool) => {
                sqlx::query("UPDATE connections SET state = $1, last_activity = $2 WHERE id = $3")
                    .bind(format!("{:?}", status))
                    .bind(chrono::Utc::now())
                    .bind(connection_id)
                    .execute(pool).await?;
            }
        }
        Ok(())
    }

    async fn get_connection_info(&self, connection_id: Uuid) -> Result<Option<ConnectionInfo>> {
        self.get_connection(connection_id).await
    }

    async fn get_connections(&self, limit: Option<u32>, _offset: Option<u32>) -> Result<Vec<ConnectionInfo>> {
        self.list_connections(limit).await
    }

    async fn get_work_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>> {
        self.list_work_templates(limit).await
    }
}

/// Mock database implementation for testing
#[cfg(any(test, feature = "test-utils"))]
pub struct MockDatabaseOps {
    connections: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<Uuid, ConnectionInfo>>>,
    shares: std::sync::Arc<tokio::sync::RwLock<Vec<Share>>>,
    templates: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<Uuid, WorkTemplate>>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl MockDatabaseOps {
    pub fn new() -> Self {
        Self {
            connections: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
            shares: std::sync::Arc::new(tokio::sync::RwLock::new(Vec::new())),
            templates: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
#[async_trait::async_trait]
impl DatabaseOps for MockDatabaseOps {
    async fn create_connection(&self, conn_info: &ConnectionInfo) -> Result<()> {
        let mut connections = self.connections.write().await;
        connections.insert(conn_info.id, conn_info.clone());
        Ok(())
    }

    async fn update_connection(&self, conn_info: &ConnectionInfo) -> Result<()> {
        let mut connections = self.connections.write().await;
        connections.insert(conn_info.id, conn_info.clone());
        Ok(())
    }

    async fn get_connection(&self, id: Uuid) -> Result<Option<ConnectionInfo>> {
        let connections = self.connections.read().await;
        Ok(connections.get(&id).cloned())
    }

    async fn list_connections(&self, limit: Option<u32>) -> Result<Vec<ConnectionInfo>> {
        let connections = self.connections.read().await;
        let mut result: Vec<_> = connections.values().cloned().collect();
        if let Some(limit) = limit {
            result.truncate(limit as usize);
        }
        Ok(result)
    }

    async fn delete_connection(&self, id: Uuid) -> Result<()> {
        let mut connections = self.connections.write().await;
        connections.remove(&id);
        Ok(())
    }

    async fn create_share(&self, share: &Share) -> Result<()> {
        let mut shares = self.shares.write().await;
        shares.push(share.clone());
        Ok(())
    }

    async fn get_shares(&self, connection_id: Option<Uuid>, limit: Option<u32>) -> Result<Vec<Share>> {
        let shares = self.shares.read().await;
        let mut result: Vec<_> = if let Some(conn_id) = connection_id {
            shares.iter().filter(|s| s.connection_id == conn_id).cloned().collect()
        } else {
            shares.clone()
        };
        if let Some(limit) = limit {
            result.truncate(limit as usize);
        }
        Ok(result)
    }

    async fn get_share_stats(&self, connection_id: Option<Uuid>) -> Result<ShareStats> {
        let shares = self.shares.read().await;
        let filtered_shares: Vec<_> = if let Some(conn_id) = connection_id {
            shares.iter().filter(|s| s.connection_id == conn_id).collect()
        } else {
            shares.iter().collect()
        };

        let total_shares = filtered_shares.len() as u64;
        let valid_shares = filtered_shares.iter().filter(|s| s.is_valid).count() as u64;
        let invalid_shares = total_shares - valid_shares;
        let blocks_found = filtered_shares.iter().filter(|s| s.block_hash.is_some()).count() as u64;
        let acceptance_rate = if total_shares > 0 {
            (valid_shares as f64 / total_shares as f64) * 100.0
        } else {
            0.0
        };

        let first_share = filtered_shares.iter().map(|s| s.submitted_at).min();
        let last_share = filtered_shares.iter().map(|s| s.submitted_at).max();

        Ok(ShareStats {
            total_shares,
            valid_shares,
            invalid_shares,
            blocks_found,
            acceptance_rate,
            first_share,
            last_share,
        })
    }

    async fn create_work_template(&self, template: &WorkTemplate) -> Result<()> {
        let mut templates = self.templates.write().await;
        templates.insert(template.id, template.clone());
        Ok(())
    }

    async fn get_work_template(&self, id: Uuid) -> Result<Option<WorkTemplate>> {
        let templates = self.templates.read().await;
        Ok(templates.get(&id).cloned())
    }

    async fn list_work_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>> {
        let templates = self.templates.read().await;
        let mut result: Vec<_> = templates.values().cloned().collect();
        if let Some(limit) = limit {
            result.truncate(limit as usize);
        }
        Ok(result)
    }

    async fn delete_expired_templates(&self) -> Result<u64> {
        let mut templates = self.templates.write().await;
        let now = chrono::Utc::now();
        let initial_count = templates.len();
        templates.retain(|_, template| template.expires_at > now);
        Ok((initial_count - templates.len()) as u64)
    }

    async fn create_alert(&self, _alert: &Alert) -> Result<()> {
        Ok(())
    }

    async fn update_alert(&self, _alert: &Alert) -> Result<()> {
        Ok(())
    }

    async fn get_alerts(&self, _resolved: Option<bool>, _limit: Option<u32>) -> Result<Vec<Alert>> {
        Ok(Vec::new())
    }

    async fn store_performance_metrics(&self, _metrics: &PerformanceMetrics) -> Result<()> {
        Ok(())
    }

    async fn get_performance_metrics(&self, _limit: Option<u32>) -> Result<Vec<PerformanceMetrics>> {
        Ok(Vec::new())
    }

    async fn store_config_history(&self, _config_data: &str, _applied_by: &str) -> Result<()> {
        Ok(())
    }

    async fn get_config_history(&self, _limit: Option<u32>) -> Result<Vec<ConfigHistoryEntry>> {
        Ok(Vec::new())
    }

    async fn store_connection(&self, conn: &crate::Connection) -> Result<()> {
        let conn_info = ConnectionInfo::from_connection(conn);
        self.create_connection(&conn_info).await
    }

    async fn store_share(&self, share: &Share) -> Result<()> {
        self.create_share(share).await
    }

    async fn store_work_template(&self, template: &WorkTemplate) -> Result<()> {
        self.create_work_template(template).await
    }

    async fn update_connection_status(&self, connection_id: Uuid, status: crate::types::ConnectionState) -> Result<()> {
        let mut connections = self.connections.write().await;
        if let Some(conn_info) = connections.get_mut(&connection_id) {
            conn_info.state = status;
        }
        Ok(())
    }

    async fn get_connection_info(&self, connection_id: Uuid) -> Result<Option<ConnectionInfo>> {
        self.get_connection(connection_id).await
    }

    async fn get_connections(&self, limit: Option<u32>, _offset: Option<u32>) -> Result<Vec<ConnectionInfo>> {
        self.list_connections(limit).await
    }

    async fn get_work_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>> {
        self.list_work_templates(limit).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;
    
    #[tokio::test]
    async fn test_sqlite_database_creation() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_url = format!("sqlite://{}", db_path.display());
        
        let pool = DatabasePool::new(&db_url, 5).await.unwrap();
        assert!(matches!(pool, DatabasePool::Sqlite(_)));
        
        // Test health check
        pool.health_check().await.unwrap();
    }
    
    #[tokio::test]
    async fn test_database_migration() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_url = format!("sqlite://{}", db_path.display());
        
        let pool = DatabasePool::new(&db_url, 5).await.unwrap();
        
        // Run migrations
        pool.migrate().await.unwrap();
        
        // Test that we can get stats after migration
        let stats = pool.get_stats().await.unwrap();
        assert_eq!(stats.total_connections, 0);
        assert_eq!(stats.total_shares, 0);
    }
    
    #[tokio::test]
    async fn test_database_ops_trait() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("test.db");
        let db_url = format!("sqlite://{}", db_path.display());
        
        let pool = DatabasePool::new(&db_url, 5).await.unwrap();
        pool.migrate().await.unwrap();
        
        // Test basic operations (simplified implementations)
        let connections = pool.list_connections(Some(10)).await.unwrap();
        assert_eq!(connections.len(), 0);
        
        let shares = pool.get_shares(None, Some(10)).await.unwrap();
        assert_eq!(shares.len(), 0);
        
        let stats = pool.get_share_stats(None).await.unwrap();
        assert_eq!(stats.total_shares, 0);
    }
}

/// Recovery-enabled database wrapper that provides automatic retry and failover
pub struct RecoveryDatabasePool {
    pool: DatabasePool,
    recovery: Arc<Mutex<DatabaseRecovery>>,
}

impl RecoveryDatabasePool {
    /// Create a new recovery-enabled database pool
    pub async fn new(database_url: &str, max_connections: u32, recovery_config: RecoveryConfig) -> Result<Self> {
        let pool = DatabasePool::new(database_url, max_connections).await?;
        let recovery = Arc::new(Mutex::new(DatabaseRecovery::new(recovery_config)));
        
        Ok(Self { pool, recovery })
    }

    /// Get the underlying database pool
    pub fn inner(&self) -> &DatabasePool {
        &self.pool
    }

    /// Check if database operations are available
    pub async fn is_available(&self) -> bool {
        let recovery = self.recovery.lock().await;
        recovery.is_database_available()
    }

    /// Get database failure count
    pub async fn failure_count(&self) -> u32 {
        let recovery = self.recovery.lock().await;
        recovery.database_failure_count()
    }
}

#[async_trait::async_trait]
impl DatabaseOps for RecoveryDatabasePool {
    async fn create_connection(&self, conn_info: &ConnectionInfo) -> Result<()> {
        self.pool.create_connection(conn_info).await
    }

    async fn update_connection(&self, conn_info: &ConnectionInfo) -> Result<()> {
        self.pool.update_connection(conn_info).await
    }

    async fn get_connection(&self, id: Uuid) -> Result<Option<ConnectionInfo>> {
        self.pool.get_connection(id).await
    }

    async fn list_connections(&self, limit: Option<u32>) -> Result<Vec<ConnectionInfo>> {
        self.pool.list_connections(limit).await
    }

    async fn delete_connection(&self, id: Uuid) -> Result<()> {
        self.pool.delete_connection(id).await
    }

    async fn create_share(&self, share: &Share) -> Result<()> {
        self.pool.create_share(share).await
    }

    async fn get_shares(&self, connection_id: Option<Uuid>, limit: Option<u32>) -> Result<Vec<Share>> {
        self.pool.get_shares(connection_id, limit).await
    }

    async fn get_share_stats(&self, connection_id: Option<Uuid>) -> Result<ShareStats> {
        self.pool.get_share_stats(connection_id).await
    }

    async fn create_work_template(&self, template: &WorkTemplate) -> Result<()> {
        self.pool.create_work_template(template).await
    }

    async fn get_work_template(&self, id: Uuid) -> Result<Option<WorkTemplate>> {
        self.pool.get_work_template(id).await
    }

    async fn list_work_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>> {
        self.pool.list_work_templates(limit).await
    }

    async fn delete_expired_templates(&self) -> Result<u64> {
        self.pool.delete_expired_templates().await
    }

    async fn create_alert(&self, alert: &Alert) -> Result<()> {
        self.pool.create_alert(alert).await
    }

    async fn update_alert(&self, alert: &Alert) -> Result<()> {
        self.pool.update_alert(alert).await
    }

    async fn get_alerts(&self, resolved: Option<bool>, limit: Option<u32>) -> Result<Vec<Alert>> {
        self.pool.get_alerts(resolved, limit).await
    }

    async fn store_performance_metrics(&self, metrics: &PerformanceMetrics) -> Result<()> {
        self.pool.store_performance_metrics(metrics).await
    }

    async fn get_performance_metrics(&self, limit: Option<u32>) -> Result<Vec<PerformanceMetrics>> {
        self.pool.get_performance_metrics(limit).await
    }

    async fn store_config_history(&self, config_data: &str, applied_by: &str) -> Result<()> {
        self.pool.store_config_history(config_data, applied_by).await
    }

    async fn get_config_history(&self, limit: Option<u32>) -> Result<Vec<ConfigHistoryEntry>> {
        self.pool.get_config_history(limit).await
    }

    async fn store_connection(&self, conn: &crate::Connection) -> Result<()> {
        self.pool.store_connection(conn).await
    }

    async fn store_share(&self, share: &Share) -> Result<()> {
        self.pool.store_share(share).await
    }

    async fn store_work_template(&self, template: &WorkTemplate) -> Result<()> {
        self.pool.store_work_template(template).await
    }

    async fn update_connection_status(&self, connection_id: Uuid, status: crate::types::ConnectionState) -> Result<()> {
        self.pool.update_connection_status(connection_id, status).await
    }

    async fn get_connection_info(&self, connection_id: Uuid) -> Result<Option<ConnectionInfo>> {
        self.pool.get_connection_info(connection_id).await
    }

    async fn get_connections(&self, limit: Option<u32>, offset: Option<u32>) -> Result<Vec<ConnectionInfo>> {
        self.pool.get_connections(limit, offset).await
    }

    async fn get_work_templates(&self, limit: Option<u32>) -> Result<Vec<WorkTemplate>> {
        self.pool.get_work_templates(limit).await
    }
}