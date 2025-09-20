use std::sync::Arc;
use std::time::{Duration, Instant};
use std::collections::HashMap;
use tokio::sync::RwLock;
use prometheus::{
    Counter, Gauge, Histogram, IntCounter, IntGauge, Registry, Encoder, TextEncoder,
    HistogramOpts, Opts,
};
use serde::{Deserialize, Serialize};
use crate::error::{Error, Result};

/// Metrics configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetricsConfig {
    /// Enable metrics collection
    pub enabled: bool,
    /// Metrics collection interval in seconds
    pub collection_interval: u64,
    /// Prometheus metrics endpoint port
    pub prometheus_port: u16,
    /// Enable system resource monitoring
    pub system_monitoring: bool,
    /// Custom metrics labels
    pub labels: HashMap<String, String>,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            collection_interval: 10,
            prometheus_port: 9090,
            system_monitoring: true,
            labels: HashMap::new(),
        }
    }
}

/// Mining-specific metrics
#[derive(Debug, Clone)]
pub struct MiningMetrics {
    /// Total shares submitted
    pub shares_submitted: IntCounter,
    /// Valid shares accepted
    pub shares_accepted: IntCounter,
    /// Invalid shares rejected
    pub shares_rejected: IntCounter,
    /// Blocks found
    pub blocks_found: IntCounter,
    /// Current hashrate (H/s)
    pub hashrate: Gauge,
    /// Share acceptance rate (%)
    pub acceptance_rate: Gauge,
    /// Mining efficiency (%)
    pub efficiency: Gauge,
    /// Share difficulty histogram
    pub share_difficulty: Histogram,
    /// Share validation time
    pub share_validation_time: Histogram,
}

/// Connection metrics
#[derive(Debug, Clone)]
pub struct ConnectionMetrics {
    /// Active connections count
    pub active_connections: IntGauge,
    /// Total connections established
    pub total_connections: IntCounter,
    /// Connection errors
    pub connection_errors: IntCounter,
    /// Protocol distribution (SV1 vs SV2)
    pub sv1_connections: IntGauge,
    pub sv2_connections: IntGauge,
    /// Connection duration histogram
    pub connection_duration: Histogram,
    /// Messages sent/received
    pub messages_sent: IntCounter,
    pub messages_received: IntCounter,
}

/// System resource metrics
#[derive(Debug, Clone)]
pub struct SystemMetrics {
    /// CPU usage percentage
    pub cpu_usage: Gauge,
    /// Memory usage in bytes
    pub memory_usage: Gauge,
    /// Network bytes sent/received
    pub network_bytes_sent: IntCounter,
    pub network_bytes_received: IntCounter,
    /// Disk I/O operations
    pub disk_reads: IntCounter,
    pub disk_writes: IntCounter,
    /// Process uptime in seconds
    pub uptime: Gauge,
}

/// Business metrics for mining operations
#[derive(Debug, Clone)]
pub struct BusinessMetrics {
    /// Revenue metrics (if applicable)
    pub estimated_revenue: Gauge,
    /// Power consumption (watts)
    pub power_consumption: Gauge,
    /// Temperature monitoring
    pub temperature: Gauge,
    /// Pool fees paid
    pub pool_fees: Counter,
    /// Mining profitability
    pub profitability: Gauge,
}

/// Main metrics collector
#[derive(Debug)]
pub struct MetricsCollector {
    registry: Registry,
    config: MetricsConfig,
    mining: MiningMetrics,
    connections: ConnectionMetrics,
    system: SystemMetrics,
    business: BusinessMetrics,
    start_time: Instant,
    last_collection: Arc<RwLock<Instant>>,
}

impl MetricsCollector {
    /// Create a new metrics collector
    pub fn new(config: MetricsConfig) -> Result<Self> {
        let registry = Registry::new();
        
        // Create mining metrics
        let mining = MiningMetrics {
            shares_submitted: IntCounter::with_opts(
                Opts::new("sv2_shares_submitted_total", "Total shares submitted")
                    .const_labels(config.labels.clone())
            )?,
            shares_accepted: IntCounter::with_opts(
                Opts::new("sv2_shares_accepted_total", "Total shares accepted")
                    .const_labels(config.labels.clone())
            )?,
            shares_rejected: IntCounter::with_opts(
                Opts::new("sv2_shares_rejected_total", "Total shares rejected")
                    .const_labels(config.labels.clone())
            )?,
            blocks_found: IntCounter::with_opts(
                Opts::new("sv2_blocks_found_total", "Total blocks found")
                    .const_labels(config.labels.clone())
            )?,
            hashrate: Gauge::with_opts(
                Opts::new("sv2_hashrate", "Current hashrate in H/s")
                    .const_labels(config.labels.clone())
            )?,
            acceptance_rate: Gauge::with_opts(
                Opts::new("sv2_acceptance_rate", "Share acceptance rate percentage")
                    .const_labels(config.labels.clone())
            )?,
            efficiency: Gauge::with_opts(
                Opts::new("sv2_mining_efficiency", "Mining efficiency percentage")
                    .const_labels(config.labels.clone())
            )?,
            share_difficulty: Histogram::with_opts(
                HistogramOpts::new("sv2_share_difficulty", "Share difficulty distribution")
                    .const_labels(config.labels.clone())
                    .buckets(vec![1.0, 10.0, 100.0, 1000.0, 10000.0, 100000.0, 1000000.0])
            )?,
            share_validation_time: Histogram::with_opts(
                HistogramOpts::new("sv2_share_validation_seconds", "Share validation time")
                    .const_labels(config.labels.clone())
                    .buckets(vec![0.001, 0.005, 0.01, 0.05, 0.1, 0.5, 1.0])
            )?,
        };

        // Create connection metrics
        let connections = ConnectionMetrics {
            active_connections: IntGauge::with_opts(
                Opts::new("sv2_active_connections", "Number of active connections")
                    .const_labels(config.labels.clone())
            )?,
            total_connections: IntCounter::with_opts(
                Opts::new("sv2_connections_total", "Total connections established")
                    .const_labels(config.labels.clone())
            )?,
            connection_errors: IntCounter::with_opts(
                Opts::new("sv2_connection_errors_total", "Connection errors")
                    .const_labels(config.labels.clone())
            )?,
            sv1_connections: IntGauge::with_opts(
                Opts::new("sv2_sv1_connections", "Active SV1 connections")
                    .const_labels(config.labels.clone())
            )?,
            sv2_connections: IntGauge::with_opts(
                Opts::new("sv2_sv2_connections", "Active SV2 connections")
                    .const_labels(config.labels.clone())
            )?,
            connection_duration: Histogram::with_opts(
                HistogramOpts::new("sv2_connection_duration_seconds", "Connection duration")
                    .const_labels(config.labels.clone())
                    .buckets(vec![1.0, 10.0, 60.0, 300.0, 1800.0, 3600.0, 86400.0])
            )?,
            messages_sent: IntCounter::with_opts(
                Opts::new("sv2_messages_sent_total", "Messages sent")
                    .const_labels(config.labels.clone())
            )?,
            messages_received: IntCounter::with_opts(
                Opts::new("sv2_messages_received_total", "Messages received")
                    .const_labels(config.labels.clone())
            )?,
        };

        // Create system metrics
        let system = SystemMetrics {
            cpu_usage: Gauge::with_opts(
                Opts::new("sv2_cpu_usage_percent", "CPU usage percentage")
                    .const_labels(config.labels.clone())
            )?,
            memory_usage: Gauge::with_opts(
                Opts::new("sv2_memory_usage_bytes", "Memory usage in bytes")
                    .const_labels(config.labels.clone())
            )?,
            network_bytes_sent: IntCounter::with_opts(
                Opts::new("sv2_network_bytes_sent_total", "Network bytes sent")
                    .const_labels(config.labels.clone())
            )?,
            network_bytes_received: IntCounter::with_opts(
                Opts::new("sv2_network_bytes_received_total", "Network bytes received")
                    .const_labels(config.labels.clone())
            )?,
            disk_reads: IntCounter::with_opts(
                Opts::new("sv2_disk_reads_total", "Disk read operations")
                    .const_labels(config.labels.clone())
            )?,
            disk_writes: IntCounter::with_opts(
                Opts::new("sv2_disk_writes_total", "Disk write operations")
                    .const_labels(config.labels.clone())
            )?,
            uptime: Gauge::with_opts(
                Opts::new("sv2_uptime_seconds", "Process uptime in seconds")
                    .const_labels(config.labels.clone())
            )?,
        };

        // Create business metrics
        let business = BusinessMetrics {
            estimated_revenue: Gauge::with_opts(
                Opts::new("sv2_estimated_revenue", "Estimated revenue")
                    .const_labels(config.labels.clone())
            )?,
            power_consumption: Gauge::with_opts(
                Opts::new("sv2_power_consumption_watts", "Power consumption in watts")
                    .const_labels(config.labels.clone())
            )?,
            temperature: Gauge::with_opts(
                Opts::new("sv2_temperature_celsius", "Temperature in Celsius")
                    .const_labels(config.labels.clone())
            )?,
            pool_fees: Counter::with_opts(
                Opts::new("sv2_pool_fees_total", "Pool fees paid")
                    .const_labels(config.labels.clone())
            )?,
            profitability: Gauge::with_opts(
                Opts::new("sv2_profitability", "Mining profitability ratio")
                    .const_labels(config.labels.clone())
            )?,
        };

        // Register all metrics
        registry.register(Box::new(mining.shares_submitted.clone()))?;
        registry.register(Box::new(mining.shares_accepted.clone()))?;
        registry.register(Box::new(mining.shares_rejected.clone()))?;
        registry.register(Box::new(mining.blocks_found.clone()))?;
        registry.register(Box::new(mining.hashrate.clone()))?;
        registry.register(Box::new(mining.acceptance_rate.clone()))?;
        registry.register(Box::new(mining.efficiency.clone()))?;
        registry.register(Box::new(mining.share_difficulty.clone()))?;
        registry.register(Box::new(mining.share_validation_time.clone()))?;

        registry.register(Box::new(connections.active_connections.clone()))?;
        registry.register(Box::new(connections.total_connections.clone()))?;
        registry.register(Box::new(connections.connection_errors.clone()))?;
        registry.register(Box::new(connections.sv1_connections.clone()))?;
        registry.register(Box::new(connections.sv2_connections.clone()))?;
        registry.register(Box::new(connections.connection_duration.clone()))?;
        registry.register(Box::new(connections.messages_sent.clone()))?;
        registry.register(Box::new(connections.messages_received.clone()))?;

        registry.register(Box::new(system.cpu_usage.clone()))?;
        registry.register(Box::new(system.memory_usage.clone()))?;
        registry.register(Box::new(system.network_bytes_sent.clone()))?;
        registry.register(Box::new(system.network_bytes_received.clone()))?;
        registry.register(Box::new(system.disk_reads.clone()))?;
        registry.register(Box::new(system.disk_writes.clone()))?;
        registry.register(Box::new(system.uptime.clone()))?;

        registry.register(Box::new(business.estimated_revenue.clone()))?;
        registry.register(Box::new(business.power_consumption.clone()))?;
        registry.register(Box::new(business.temperature.clone()))?;
        registry.register(Box::new(business.pool_fees.clone()))?;
        registry.register(Box::new(business.profitability.clone()))?;

        let start_time = Instant::now();
        let last_collection = Arc::new(RwLock::new(start_time));

        Ok(Self {
            registry,
            config,
            mining,
            connections,
            system,
            business,
            start_time,
            last_collection,
        })
    }

    /// Get mining metrics
    pub fn mining(&self) -> &MiningMetrics {
        &self.mining
    }

    /// Get connection metrics
    pub fn connections(&self) -> &ConnectionMetrics {
        &self.connections
    }

    /// Get system metrics
    pub fn system(&self) -> &SystemMetrics {
        &self.system
    }

    /// Get business metrics
    pub fn business(&self) -> &BusinessMetrics {
        &self.business
    }

    /// Record a share submission
    pub fn record_share(&self, difficulty: f64, is_valid: bool, is_block: bool, validation_time: Duration) {
        self.mining.shares_submitted.inc();
        
        if is_valid {
            self.mining.shares_accepted.inc();
        } else {
            self.mining.shares_rejected.inc();
        }
        
        if is_block {
            self.mining.blocks_found.inc();
        }
        
        self.mining.share_difficulty.observe(difficulty);
        self.mining.share_validation_time.observe(validation_time.as_secs_f64());
        
        // Update acceptance rate
        let total = self.mining.shares_submitted.get() as f64;
        let accepted = self.mining.shares_accepted.get() as f64;
        if total > 0.0 {
            self.mining.acceptance_rate.set((accepted / total) * 100.0);
        }
    }

    /// Record connection event
    pub fn record_connection(&self, protocol: &str, is_new: bool) {
        if is_new {
            self.connections.total_connections.inc();
            self.connections.active_connections.inc();
        }
        
        match protocol {
            "sv1" => {
                if is_new {
                    self.connections.sv1_connections.inc();
                }
            }
            "sv2" => {
                if is_new {
                    self.connections.sv2_connections.inc();
                }
            }
            _ => {}
        }
    }

    /// Record connection close
    pub fn record_connection_close(&self, protocol: &str, duration: Duration) {
        self.connections.active_connections.dec();
        self.connections.connection_duration.observe(duration.as_secs_f64());
        
        match protocol {
            "sv1" => self.connections.sv1_connections.dec(),
            "sv2" => self.connections.sv2_connections.dec(),
            _ => {}
        }
    }

    /// Record message sent/received
    pub fn record_message(&self, sent: bool) {
        if sent {
            self.connections.messages_sent.inc();
        } else {
            self.connections.messages_received.inc();
        }
    }

    /// Update hashrate
    pub fn update_hashrate(&self, hashrate: f64) {
        self.mining.hashrate.set(hashrate);
    }

    /// Update system metrics
    pub async fn update_system_metrics(&self) -> Result<()> {
        if !self.config.system_monitoring {
            return Ok(());
        }

        // Update uptime
        let uptime = self.start_time.elapsed().as_secs_f64();
        self.system.uptime.set(uptime);

        // TODO: Implement actual system monitoring
        // For now, we'll use placeholder values
        // In a real implementation, you would use system monitoring libraries
        // like sysinfo or procfs to get actual system metrics
        
        Ok(())
    }

    /// Collect all metrics and update calculated values
    pub async fn collect_metrics(&self) -> Result<()> {
        let mut last_collection = self.last_collection.write().await;
        *last_collection = Instant::now();

        self.update_system_metrics().await?;
        
        Ok(())
    }

    /// Export metrics in Prometheus format
    pub fn export_prometheus(&self) -> Result<String> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer)?;
        Ok(String::from_utf8(buffer)?)
    }

    /// Get metrics summary
    pub fn get_summary(&self) -> MetricsSummary {
        MetricsSummary {
            shares_submitted: self.mining.shares_submitted.get(),
            shares_accepted: self.mining.shares_accepted.get(),
            shares_rejected: self.mining.shares_rejected.get(),
            blocks_found: self.mining.blocks_found.get(),
            hashrate: self.mining.hashrate.get(),
            acceptance_rate: self.mining.acceptance_rate.get(),
            active_connections: self.connections.active_connections.get(),
            total_connections: self.connections.total_connections.get(),
            sv1_connections: self.connections.sv1_connections.get(),
            sv2_connections: self.connections.sv2_connections.get(),
            uptime: self.system.uptime.get(),
        }
    }
}

/// Metrics summary for API responses
#[derive(Debug, Serialize, Deserialize)]
pub struct MetricsSummary {
    pub shares_submitted: u64,
    pub shares_accepted: u64,
    pub shares_rejected: u64,
    pub blocks_found: u64,
    pub hashrate: f64,
    pub acceptance_rate: f64,
    pub active_connections: i64,
    pub total_connections: u64,
    pub sv1_connections: i64,
    pub sv2_connections: i64,
    pub uptime: f64,
}

/// Metrics service for background collection
pub struct MetricsService {
    collector: Arc<MetricsCollector>,
    collection_interval: Duration,
}

impl MetricsService {
    pub fn new(collector: Arc<MetricsCollector>, collection_interval: Duration) -> Self {
        Self {
            collector,
            collection_interval,
        }
    }

    /// Start the metrics collection service
    pub async fn start(&self) -> Result<()> {
        let mut interval = tokio::time::interval(self.collection_interval);
        
        loop {
            interval.tick().await;
            
            if let Err(e) = self.collector.collect_metrics().await {
                tracing::error!("Failed to collect metrics: {}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[tokio::test]
    async fn test_metrics_collector_creation() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        // Test initial values
        assert_eq!(collector.mining.shares_submitted.get(), 0);
        assert_eq!(collector.connections.active_connections.get(), 0);
    }

    #[tokio::test]
    async fn test_share_recording() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        // Record a valid share
        collector.record_share(1000.0, true, false, Duration::from_millis(10));
        
        assert_eq!(collector.mining.shares_submitted.get(), 1);
        assert_eq!(collector.mining.shares_accepted.get(), 1);
        assert_eq!(collector.mining.shares_rejected.get(), 0);
        assert_eq!(collector.mining.acceptance_rate.get(), 100.0);
        
        // Record an invalid share
        collector.record_share(500.0, false, false, Duration::from_millis(5));
        
        assert_eq!(collector.mining.shares_submitted.get(), 2);
        assert_eq!(collector.mining.shares_accepted.get(), 1);
        assert_eq!(collector.mining.shares_rejected.get(), 1);
        assert_eq!(collector.mining.acceptance_rate.get(), 50.0);
    }

    #[tokio::test]
    async fn test_connection_recording() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        // Record new SV2 connection
        collector.record_connection("sv2", true);
        
        assert_eq!(collector.connections.active_connections.get(), 1);
        assert_eq!(collector.connections.total_connections.get(), 1);
        assert_eq!(collector.connections.sv2_connections.get(), 1);
        assert_eq!(collector.connections.sv1_connections.get(), 0);
        
        // Record new SV1 connection
        collector.record_connection("sv1", true);
        
        assert_eq!(collector.connections.active_connections.get(), 2);
        assert_eq!(collector.connections.total_connections.get(), 2);
        assert_eq!(collector.connections.sv2_connections.get(), 1);
        assert_eq!(collector.connections.sv1_connections.get(), 1);
        
        // Close SV2 connection
        collector.record_connection_close("sv2", Duration::from_secs(300));
        
        assert_eq!(collector.connections.active_connections.get(), 1);
        assert_eq!(collector.connections.sv2_connections.get(), 0);
        assert_eq!(collector.connections.sv1_connections.get(), 1);
    }

    #[tokio::test]
    async fn test_prometheus_export() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        // Record some data
        collector.record_share(1000.0, true, false, Duration::from_millis(10));
        collector.record_connection("sv2", true);
        
        let prometheus_output = collector.export_prometheus().unwrap();
        
        assert!(prometheus_output.contains("sv2_shares_submitted_total"));
        assert!(prometheus_output.contains("sv2_active_connections"));
    }

    #[tokio::test]
    async fn test_metrics_summary() {
        let config = MetricsConfig::default();
        let collector = MetricsCollector::new(config).unwrap();
        
        // Record some data
        collector.record_share(1000.0, true, false, Duration::from_millis(10));
        collector.record_connection("sv2", true);
        collector.update_hashrate(1000000.0);
        
        let summary = collector.get_summary();
        
        assert_eq!(summary.shares_submitted, 1);
        assert_eq!(summary.shares_accepted, 1);
        assert_eq!(summary.active_connections, 1);
        assert_eq!(summary.hashrate, 1000000.0);
    }
}