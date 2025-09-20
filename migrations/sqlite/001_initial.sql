-- Initial database schema for SQLite

-- Connections table
CREATE TABLE IF NOT EXISTS connections (
    id TEXT PRIMARY KEY,
    address TEXT NOT NULL,
    protocol TEXT NOT NULL,
    state TEXT NOT NULL,
    connected_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    last_activity DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    user_agent TEXT,
    version TEXT,
    subscribed_difficulty REAL,
    extranonce1 TEXT,
    extranonce2_size INTEGER,
    total_shares INTEGER NOT NULL DEFAULT 0,
    valid_shares INTEGER NOT NULL DEFAULT 0,
    invalid_shares INTEGER NOT NULL DEFAULT 0,
    blocks_found INTEGER NOT NULL DEFAULT 0
);

-- Index for connection lookups
CREATE INDEX IF NOT EXISTS idx_connections_address ON connections(address);
CREATE INDEX IF NOT EXISTS idx_connections_state ON connections(state);
CREATE INDEX IF NOT EXISTS idx_connections_last_activity ON connections(last_activity);

-- Shares table
CREATE TABLE IF NOT EXISTS shares (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    connection_id TEXT NOT NULL,
    nonce INTEGER NOT NULL,
    timestamp INTEGER NOT NULL,
    difficulty REAL NOT NULL,
    is_valid BOOLEAN NOT NULL,
    block_hash TEXT,
    submitted_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (connection_id) REFERENCES connections(id) ON DELETE CASCADE
);

-- Indexes for share queries
CREATE INDEX IF NOT EXISTS idx_shares_connection_id ON shares(connection_id);
CREATE INDEX IF NOT EXISTS idx_shares_submitted_at ON shares(submitted_at);
CREATE INDEX IF NOT EXISTS idx_shares_is_valid ON shares(is_valid);
CREATE INDEX IF NOT EXISTS idx_shares_block_hash ON shares(block_hash) WHERE block_hash IS NOT NULL;

-- Work templates table
CREATE TABLE IF NOT EXISTS work_templates (
    id TEXT PRIMARY KEY,
    previous_hash TEXT NOT NULL,
    coinbase_tx BLOB NOT NULL,
    transactions BLOB NOT NULL,
    difficulty REAL NOT NULL,
    timestamp INTEGER NOT NULL,
    expires_at DATETIME NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for template expiration cleanup
CREATE INDEX IF NOT EXISTS idx_work_templates_expires_at ON work_templates(expires_at);
CREATE INDEX IF NOT EXISTS idx_work_templates_created_at ON work_templates(created_at);

-- Alerts table
CREATE TABLE IF NOT EXISTS alerts (
    id TEXT PRIMARY KEY,
    level TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    component TEXT NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    resolved_at DATETIME,
    metadata TEXT -- JSON blob for additional data
);

-- Indexes for alert queries
CREATE INDEX IF NOT EXISTS idx_alerts_level ON alerts(level);
CREATE INDEX IF NOT EXISTS idx_alerts_component ON alerts(component);
CREATE INDEX IF NOT EXISTS idx_alerts_created_at ON alerts(created_at);
CREATE INDEX IF NOT EXISTS idx_alerts_resolved ON alerts(resolved_at);

-- Performance metrics table
CREATE TABLE IF NOT EXISTS performance_metrics (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    cpu_usage REAL NOT NULL,
    memory_usage INTEGER NOT NULL,
    memory_total INTEGER NOT NULL,
    network_rx_bytes INTEGER NOT NULL,
    network_tx_bytes INTEGER NOT NULL,
    disk_usage INTEGER NOT NULL,
    disk_total INTEGER NOT NULL,
    open_connections INTEGER NOT NULL,
    database_connections INTEGER NOT NULL,
    timestamp DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for metrics time series queries
CREATE INDEX IF NOT EXISTS idx_performance_metrics_timestamp ON performance_metrics(timestamp);

-- Configuration history table
CREATE TABLE IF NOT EXISTS config_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    config_data TEXT NOT NULL,
    applied_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    applied_by TEXT NOT NULL
);

-- Index for config history queries
CREATE INDEX IF NOT EXISTS idx_config_history_applied_at ON config_history(applied_at);

-- Workers table (for pool mode)
CREATE TABLE IF NOT EXISTS workers (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    connection_id TEXT NOT NULL,
    difficulty REAL NOT NULL,
    last_share DATETIME,
    total_shares INTEGER NOT NULL DEFAULT 0,
    valid_shares INTEGER NOT NULL DEFAULT 0,
    invalid_shares INTEGER NOT NULL DEFAULT 0,
    hashrate REAL NOT NULL DEFAULT 0.0,
    efficiency REAL NOT NULL DEFAULT 0.0,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    FOREIGN KEY (connection_id) REFERENCES connections(id) ON DELETE CASCADE,
    UNIQUE(name, connection_id)
);

-- Indexes for worker queries
CREATE INDEX IF NOT EXISTS idx_workers_connection_id ON workers(connection_id);
CREATE INDEX IF NOT EXISTS idx_workers_name ON workers(name);
CREATE INDEX IF NOT EXISTS idx_workers_last_share ON workers(last_share);

-- Jobs table (for SV2 protocol)
CREATE TABLE IF NOT EXISTS jobs (
    id TEXT PRIMARY KEY,
    template_id TEXT NOT NULL,
    version INTEGER NOT NULL,
    previous_hash TEXT NOT NULL,
    merkle_root TEXT NOT NULL,
    timestamp INTEGER NOT NULL,
    bits INTEGER NOT NULL,
    target TEXT NOT NULL,
    clean_jobs BOOLEAN NOT NULL,
    created_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    expires_at DATETIME NOT NULL,
    FOREIGN KEY (template_id) REFERENCES work_templates(id) ON DELETE CASCADE
);

-- Indexes for job queries
CREATE INDEX IF NOT EXISTS idx_jobs_template_id ON jobs(template_id);
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);
CREATE INDEX IF NOT EXISTS idx_jobs_expires_at ON jobs(expires_at);

-- Upstream pool status table (for proxy/client modes)
CREATE TABLE IF NOT EXISTS upstream_status (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    url TEXT NOT NULL UNIQUE,
    connected BOOLEAN NOT NULL DEFAULT FALSE,
    last_connected DATETIME,
    connection_attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    latency_ms INTEGER,
    shares_submitted INTEGER NOT NULL DEFAULT 0,
    shares_accepted INTEGER NOT NULL DEFAULT 0,
    shares_rejected INTEGER NOT NULL DEFAULT 0,
    updated_at DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- Index for upstream status queries
CREATE INDEX IF NOT EXISTS idx_upstream_status_url ON upstream_status(url);
CREATE INDEX IF NOT EXISTS idx_upstream_status_connected ON upstream_status(connected);