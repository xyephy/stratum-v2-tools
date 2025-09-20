// Chart management for real-time data visualization

class ChartManager {
    constructor() {
        this.charts = new Map();
        this.chartData = new Map();
        this.maxDataPoints = 50; // Keep last 50 data points
        this.updateInterval = 5000; // Update every 5 seconds
        
        this.initializeCharts();
    }

    initializeCharts() {
        // Initialize hashrate chart
        this.createHashrateChart();
        
        // Initialize acceptance rate chart
        this.createAcceptanceChart();
        
        // Set up periodic updates
        this.startPeriodicUpdates();
    }

    createHashrateChart() {
        const canvas = document.getElementById('hashrate-chart');
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        
        // Initialize data
        this.chartData.set('hashrate', {
            labels: [],
            data: [],
            timestamps: []
        });

        const chart = new Chart(ctx, {
            type: 'line',
            data: {
                labels: [],
                datasets: [{
                    label: 'Hashrate (TH/s)',
                    data: [],
                    borderColor: '#007acc',
                    backgroundColor: 'rgba(0, 122, 204, 0.1)',
                    borderWidth: 2,
                    fill: true,
                    tension: 0.4,
                    pointRadius: 2,
                    pointHoverRadius: 4
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                interaction: {
                    intersect: false,
                    mode: 'index'
                },
                plugins: {
                    legend: {
                        display: false
                    },
                    tooltip: {
                        backgroundColor: 'rgba(44, 62, 80, 0.95)',
                        titleColor: '#ffffff',
                        bodyColor: '#ffffff',
                        borderColor: '#007acc',
                        borderWidth: 1,
                        callbacks: {
                            label: (context) => {
                                return `Hashrate: ${context.parsed.y.toFixed(2)} TH/s`;
                            }
                        }
                    }
                },
                scales: {
                    x: {
                        display: true,
                        grid: {
                            color: 'rgba(189, 195, 199, 0.3)'
                        },
                        ticks: {
                            color: '#7f8c8d',
                            maxTicksLimit: 8
                        }
                    },
                    y: {
                        display: true,
                        beginAtZero: true,
                        grid: {
                            color: 'rgba(189, 195, 199, 0.3)'
                        },
                        ticks: {
                            color: '#7f8c8d',
                            callback: (value) => `${value} TH/s`
                        }
                    }
                },
                animation: {
                    duration: 750,
                    easing: 'easeInOutQuart'
                }
            }
        });

        this.charts.set('hashrate', chart);
    }

    createAcceptanceChart() {
        const canvas = document.getElementById('acceptance-chart');
        if (!canvas) return;

        const ctx = canvas.getContext('2d');
        
        // Initialize data
        this.chartData.set('acceptance', {
            labels: [],
            data: [],
            timestamps: []
        });

        const chart = new Chart(ctx, {
            type: 'line',
            data: {
                labels: [],
                datasets: [{
                    label: 'Acceptance Rate (%)',
                    data: [],
                    borderColor: '#27ae60',
                    backgroundColor: 'rgba(39, 174, 96, 0.1)',
                    borderWidth: 2,
                    fill: true,
                    tension: 0.4,
                    pointRadius: 2,
                    pointHoverRadius: 4
                }]
            },
            options: {
                responsive: true,
                maintainAspectRatio: false,
                interaction: {
                    intersect: false,
                    mode: 'index'
                },
                plugins: {
                    legend: {
                        display: false
                    },
                    tooltip: {
                        backgroundColor: 'rgba(44, 62, 80, 0.95)',
                        titleColor: '#ffffff',
                        bodyColor: '#ffffff',
                        borderColor: '#27ae60',
                        borderWidth: 1,
                        callbacks: {
                            label: (context) => {
                                return `Acceptance: ${context.parsed.y.toFixed(1)}%`;
                            }
                        }
                    }
                },
                scales: {
                    x: {
                        display: true,
                        grid: {
                            color: 'rgba(189, 195, 199, 0.3)'
                        },
                        ticks: {
                            color: '#7f8c8d',
                            maxTicksLimit: 8
                        }
                    },
                    y: {
                        display: true,
                        min: 0,
                        max: 100,
                        grid: {
                            color: 'rgba(189, 195, 199, 0.3)'
                        },
                        ticks: {
                            color: '#7f8c8d',
                            callback: (value) => `${value}%`
                        }
                    }
                },
                animation: {
                    duration: 750,
                    easing: 'easeInOutQuart'
                }
            }
        });

        this.charts.set('acceptance', chart);
    }

    updateHashrateChart(hashrate) {
        const chart = this.charts.get('hashrate');
        const data = this.chartData.get('hashrate');
        
        if (!chart || !data) return;

        const now = new Date();
        const timeLabel = now.toLocaleTimeString('en-US', { 
            hour12: false, 
            hour: '2-digit', 
            minute: '2-digit',
            second: '2-digit'
        });

        // Convert to TH/s
        const hashrateInTH = hashrate / 1e12;

        // Add new data point
        data.labels.push(timeLabel);
        data.data.push(hashrateInTH);
        data.timestamps.push(now);

        // Remove old data points if we exceed the limit
        if (data.labels.length > this.maxDataPoints) {
            data.labels.shift();
            data.data.shift();
            data.timestamps.shift();
        }

        // Update chart
        chart.data.labels = [...data.labels];
        chart.data.datasets[0].data = [...data.data];
        chart.update('none'); // No animation for real-time updates
    }

    updateAcceptanceChart(acceptanceRate) {
        const chart = this.charts.get('acceptance');
        const data = this.chartData.get('acceptance');
        
        if (!chart || !data) return;

        const now = new Date();
        const timeLabel = now.toLocaleTimeString('en-US', { 
            hour12: false, 
            hour: '2-digit', 
            minute: '2-digit',
            second: '2-digit'
        });

        // Add new data point
        data.labels.push(timeLabel);
        data.data.push(acceptanceRate);
        data.timestamps.push(now);

        // Remove old data points if we exceed the limit
        if (data.labels.length > this.maxDataPoints) {
            data.labels.shift();
            data.data.shift();
            data.timestamps.shift();
        }

        // Update chart
        chart.data.labels = [...data.labels];
        chart.data.datasets[0].data = [...data.data];
        chart.update('none'); // No animation for real-time updates
    }

    updateMetrics(metrics) {
        // Update any additional metrics charts here
        console.log('Updating metrics charts:', metrics);
    }

    startPeriodicUpdates() {
        // This could be used for periodic chart maintenance
        setInterval(() => {
            this.cleanupOldData();
        }, 60000); // Clean up every minute
    }

    cleanupOldData() {
        const cutoffTime = new Date(Date.now() - (60 * 60 * 1000)); // 1 hour ago

        this.chartData.forEach((data, chartName) => {
            let removedCount = 0;
            
            // Remove data points older than cutoff
            while (data.timestamps.length > 0 && data.timestamps[0] < cutoffTime) {
                data.labels.shift();
                data.data.shift();
                data.timestamps.shift();
                removedCount++;
            }

            if (removedCount > 0) {
                const chart = this.charts.get(chartName);
                if (chart) {
                    chart.data.labels = [...data.labels];
                    chart.data.datasets[0].data = [...data.data];
                    chart.update('none');
                }
            }
        });
    }

    // Utility methods for chart management
    pauseChart(chartName) {
        const chart = this.charts.get(chartName);
        if (chart) {
            chart.options.animation = false;
            chart.update();
        }
    }

    resumeChart(chartName) {
        const chart = this.charts.get(chartName);
        if (chart) {
            chart.options.animation = {
                duration: 750,
                easing: 'easeInOutQuart'
            };
            chart.update();
        }
    }

    clearChart(chartName) {
        const chart = this.charts.get(chartName);
        const data = this.chartData.get(chartName);
        
        if (chart && data) {
            data.labels = [];
            data.data = [];
            data.timestamps = [];
            
            chart.data.labels = [];
            chart.data.datasets[0].data = [];
            chart.update();
        }
    }

    resizeCharts() {
        this.charts.forEach(chart => {
            chart.resize();
        });
    }

    destroyCharts() {
        this.charts.forEach(chart => {
            chart.destroy();
        });
        this.charts.clear();
        this.chartData.clear();
    }

    // Export chart data for debugging or analysis
    exportChartData(chartName) {
        const data = this.chartData.get(chartName);
        if (data) {
            return {
                labels: [...data.labels],
                data: [...data.data],
                timestamps: [...data.timestamps]
            };
        }
        return null;
    }

    // Get chart statistics
    getChartStats(chartName) {
        const data = this.chartData.get(chartName);
        if (!data || data.data.length === 0) {
            return null;
        }

        const values = data.data;
        const min = Math.min(...values);
        const max = Math.max(...values);
        const avg = values.reduce((sum, val) => sum + val, 0) / values.length;
        const latest = values[values.length - 1];

        return {
            min,
            max,
            average: avg,
            latest,
            count: values.length
        };
    }
}

// Connection Manager for handling connection table updates
class ConnectionManager {
    constructor() {
        this.connections = new Map();
        this.tableBody = document.getElementById('connections-tbody');
    }

    addConnection(connection) {
        this.connections.set(connection.id, connection);
        this.updateTable();
    }

    updateConnection(connection) {
        this.connections.set(connection.id, connection);
        this.updateTable();
    }

    removeConnection(connectionId) {
        this.connections.delete(connectionId);
        this.updateTable();
    }

    updateTable() {
        if (!this.tableBody) return;

        if (this.connections.size === 0) {
            this.tableBody.innerHTML = '<tr><td colspan="7" class="no-data">No active connections</td></tr>';
            return;
        }

        const rows = Array.from(this.connections.values()).map(conn => {
            const connectedTime = new Date(conn.connected_at).toLocaleString();
            const lastActivity = new Date(conn.last_activity).toLocaleString();
            const protocolBadge = `<span class="status-badge ${conn.protocol.toLowerCase()}">${conn.protocol.toUpperCase()}</span>`;
            const stateBadge = `<span class="status-badge ${conn.state.toLowerCase()}">${conn.state}</span>`;
            
            return `
                <tr>
                    <td>${conn.id.substring(0, 8)}...</td>
                    <td>${conn.address}</td>
                    <td>${protocolBadge}</td>
                    <td>${stateBadge}</td>
                    <td>${connectedTime}</td>
                    <td>${lastActivity}</td>
                    <td>
                        <button class="btn btn-danger btn-sm" onclick="disconnectConnection('${conn.id}')">
                            Disconnect
                        </button>
                    </td>
                </tr>
            `;
        }).join('');

        this.tableBody.innerHTML = rows;
    }
}

// Share Manager for handling recent shares
class ShareManager {
    constructor() {
        this.shares = [];
        this.maxShares = 20; // Keep last 20 shares
        this.tableBody = document.getElementById('shares-tbody');
    }

    addShare(share) {
        this.shares.unshift(share); // Add to beginning
        
        // Remove old shares
        if (this.shares.length > this.maxShares) {
            this.shares = this.shares.slice(0, this.maxShares);
        }
        
        this.updateTable();
    }

    updateTable() {
        if (!this.tableBody) return;

        if (this.shares.length === 0) {
            this.tableBody.innerHTML = '<tr><td colspan="5" class="no-data">No recent shares</td></tr>';
            return;
        }

        const rows = this.shares.map(share => {
            const time = new Date(share.submitted_at).toLocaleTimeString();
            const validBadge = share.is_valid 
                ? '<span class="status-badge success">Valid</span>'
                : '<span class="status-badge danger">Invalid</span>';
            const blockBadge = share.is_block 
                ? '<span class="status-badge success">Block!</span>'
                : '-';
            
            return `
                <tr>
                    <td>${time}</td>
                    <td>${share.connection_id.substring(0, 8)}...</td>
                    <td>${share.difficulty.toFixed(2)}</td>
                    <td>${validBadge}</td>
                    <td>${blockBadge}</td>
                </tr>
            `;
        }).join('');

        this.tableBody.innerHTML = rows;
    }
}

// Alert Manager for handling system alerts
class AlertManager {
    constructor() {
        this.alerts = new Map();
        this.container = document.getElementById('alerts-container');
    }

    addAlert(alert) {
        this.alerts.set(alert.id, alert);
        this.updateDisplay();
    }

    resolveAlert(alertId) {
        this.alerts.delete(alertId);
        this.updateDisplay();
    }

    updateDisplay() {
        if (!this.container) return;

        if (this.alerts.size === 0) {
            this.container.innerHTML = '<div class="no-data">No alerts</div>';
            return;
        }

        const alertElements = Array.from(this.alerts.values()).map(alert => {
            const time = new Date(alert.created_at).toLocaleString();
            const severityClass = alert.severity.toLowerCase();
            
            return `
                <div class="alert-item ${severityClass}">
                    <div class="alert-header">
                        <span class="alert-title">${alert.title}</span>
                        <span class="alert-time">${time}</span>
                    </div>
                    <div class="alert-message">${alert.message}</div>
                </div>
            `;
        }).join('');

        this.container.innerHTML = alertElements;
    }
}

// Global instances
window.chartManager = null;
window.connectionManager = null;
window.shareManager = null;
window.alertManager = null;

// Initialize when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    window.chartManager = new ChartManager();
    window.connectionManager = new ConnectionManager();
    window.shareManager = new ShareManager();
    window.alertManager = new AlertManager();
    
    // Handle window resize
    window.addEventListener('resize', () => {
        if (window.chartManager) {
            window.chartManager.resizeCharts();
        }
    });
});

// Utility function for disconnecting connections
function disconnectConnection(connectionId) {
    if (confirm('Are you sure you want to disconnect this connection?')) {
        // This would make an API call to disconnect the connection
        fetch(`/api/v1/connections/${connectionId}`, {
            method: 'DELETE'
        })
        .then(response => {
            if (response.ok) {
                console.log('Connection disconnected successfully');
            } else {
                console.error('Failed to disconnect connection');
            }
        })
        .catch(error => {
            console.error('Error disconnecting connection:', error);
        });
    }
}