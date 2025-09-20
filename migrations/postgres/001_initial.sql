-- Initial database schema for PostgreSQL

-- Enable UUID extension
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- Connections table
CREATE TABLE IF NOT EXISTS connections (
    id UUID PRIMARY KEY,
    address TEXT NOT NULL,
    protocol TEXT NOT NULL,
    state TEXT NOT NULL,
    connected_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    last_activity TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    user_agent TEXT,
    version TEXT,
    subscribed_difficulty DOUBLE PRECISION,
    extranonce1 TEXT,
    extranonce2_size SMALLINT,
    total_shares BIGINT NOT NULL DEFAULT 0,
    valid_shares BIGINT NOT NULL DEFAULT 0,
    invalid_shares BIGINT NOT NULL DEFAULT 0,
    blocks_found BIGINT NOT NULL DEFAULT 0
);

-- Indexes for connection lookups
CREATE INDEX IF NOT EXISTS idx_connections_address ON connections(address);
CREATE INDEX IF NOT EXISTS idx_connections_state ON connections(state);
CREATE INDEX IF NOT EXISTS idx_connections_last_activity ON connections(last_activity);

-- Shares table
CREATE TABLE IF NOT EXISTS shares (
    id BIGSERIAL PRIMARY KEY,
    connection_id UUID NOT NULL,
    nonce BIGINT NOT NULL,
    timestamp BIGINT NOT NULL,
    difficulty DOUBLE PRECISION NOT NULL,
    is_valid BOOLEAN NOT NULL,
    block_hash TEXT,
    submitted_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (connection_id) REFERENCES connections(id) ON DELETE CASCADE
);

-- Indexes for share queries
CREATE INDEX IF NOT EXISTS idx_shares_connection_id ON shares(connection_id);
CREATE INDEX IF NOT EXISTS idx_shares_submitted_at ON shares(submitted_at);
CREATE INDEX IF NOT EXISTS idx_shares_is_valid ON shares(is_valid);
CREATE INDEX IF NOT EXISTS idx_shares_block_hash ON shares(block_hash) WHERE block_hash IS NOT NULL;

-- Work templates table
CREATE TABLE IF NOT EXISTS work_templates (
    id UUID PRIMARY KEY,
    previous_hash TEXT NOT NULL,
    coinbase_tx BYTEA NOT NULL,
    transactions BYTEA NOT NULL,
    difficulty DOUBLE PRECISION NOT NULL,
    timestamp BIGINT NOT NULL,
    expires_at TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for template expiration cleanup
CREATE INDEX IF NOT EXISTS idx_work_templates_expires_at ON work_templates(expires_at);
CREATE INDEX IF NOT EXISTS idx_work_templates_created_at ON work_templates(created_at);

-- Alerts table
CREATE TABLE IF NOT EXISTS alerts (
    id UUID PRIMARY KEY,
    level TEXT NOT NULL,
    title TEXT NOT NULL,
    message TEXT NOT NULL,
    component TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    resolved_at TIMESTAMPTZ,
    metadata JSONB -- JSON blob for additional data
);

-- Indexes for alert queries
CREATE INDEX IF NOT EXISTS idx_alerts_level ON alerts(level);
CREATE INDEX IF NOT EXISTS idx_alerts_component ON alerts(component);
CREATE INDEX IF NOT EXISTS idx_alerts_created_at ON alerts(created_at);
CREATE INDEX IF NOT EXISTS idx_alerts_resolved ON alerts(resolved_at);
CREATE INDEX IF NOT EXISTS idx_alerts_metadata ON alerts USING GIN(metadata);

-- Performance metrics table
CREATE TABLE IF NOT EXISTS performance_metrics (
    id BIGSERIAL PRIMARY KEY,
    cpu_usage DOUBLE PRECISION NOT NULL,
    memory_usage BIGINT NOT NULL,
    memory_total BIGINT NOT NULL,
    network_rx_bytes BIGINT NOT NULL,
    network_tx_bytes BIGINT NOT NULL,
    disk_usage BIGINT NOT NULL,
    disk_total BIGINT NOT NULL,
    open_connections BIGINT NOT NULL,
    database_connections INTEGER NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Index for metrics time series queries
CREATE INDEX IF NOT EXISTS idx_performance_metrics_timestamp ON performance_metrics(timestamp);

-- Configuration history table
CREATE TABLE IF NOT EXISTS config_history (
    id BIGSERIAL PRIMARY KEY,
    config_data TEXT NOT NULL,
    applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_by TEXT NOT NULL
);

-- Index for config history queries
CREATE INDEX IF NOT EXISTS idx_config_history_applied_at ON config_history(applied_at);

-- Workers table (for pool mode)
CREATE TABLE IF NOT EXISTS workers (
    id BIGSERIAL PRIMARY KEY,
    name TEXT NOT NULL,
    connection_id UUID NOT NULL,
    difficulty DOUBLE PRECISION NOT NULL,
    last_share TIMESTAMPTZ,
    total_shares BIGINT NOT NULL DEFAULT 0,
    valid_shares BIGINT NOT NULL DEFAULT 0,
    invalid_shares BIGINT NOT NULL DEFAULT 0,
    hashrate DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    efficiency DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
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
    template_id UUID NOT NULL,
    version INTEGER NOT NULL,
    previous_hash TEXT NOT NULL,
    merkle_root TEXT NOT NULL,
    timestamp BIGINT NOT NULL,
    bits INTEGER NOT NULL,
    target TEXT NOT NULL,
    clean_jobs BOOLEAN NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    expires_at TIMESTAMPTZ NOT NULL,
    FOREIGN KEY (template_id) REFERENCES work_templates(id) ON DELETE CASCADE
);

-- Indexes for job queries
CREATE INDEX IF NOT EXISTS idx_jobs_template_id ON jobs(template_id);
CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);
CREATE INDEX IF NOT EXISTS idx_jobs_expires_at ON jobs(expires_at);

-- Upstream pool status table (for proxy/client modes)
CREATE TABLE IF NOT EXISTS upstream_status (
    id BIGSERIAL PRIMARY KEY,
    url TEXT NOT NULL UNIQUE,
    connected BOOLEAN NOT NULL DEFAULT FALSE,
    last_connected TIMESTAMPTZ,
    connection_attempts INTEGER NOT NULL DEFAULT 0,
    last_error TEXT,
    latency_ms INTEGER,
    shares_submitted BIGINT NOT NULL DEFAULT 0,
    shares_accepted BIGINT NOT NULL DEFAULT 0,
    shares_rejected BIGINT NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Indexes for upstream status queries
CREATE INDEX IF NOT EXISTS idx_upstream_status_url ON upstream_status(url);
CREATE INDEX IF NOT EXISTS idx_upstream_status_connected ON upstream_status(connected);