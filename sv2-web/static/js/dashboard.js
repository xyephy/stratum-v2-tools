// Main dashboard JavaScript - UI interactions and configuration management

class DashboardManager {
    constructor() {
        this.configModal = null;
        this.configEditor = null;
        this.currentConfig = null;
        
        this.initializeUI();
        this.setupEventListeners();
        this.loadInitialData();
    }

    initializeUI() {
        // Initialize modal elements
        this.configModal = document.getElementById('config-modal');
        this.configEditor = document.getElementById('config-editor');
        
        // Initialize tabs
        this.initializeTabs();
        
        // Set up periodic data refresh
        this.startPeriodicRefresh();
    }

    initializeTabs() {
        const tabButtons = document.querySelectorAll('.tab-button');
        const tabContents = document.querySelectorAll('.tab-content');

        tabButtons.forEach(button => {
            button.addEventListener('click', () => {
                const targetTab = button.getAttribute('data-tab');
                
                // Remove active class from all buttons and contents
                tabButtons.forEach(btn => btn.classList.remove('active'));
                tabContents.forEach(content => content.classList.remove('active'));
                
                // Add active class to clicked button and corresponding content
                button.classList.add('active');
                const targetContent = document.getElementById(`${targetTab}-tab`);
                if (targetContent) {
                    targetContent.classList.add('active');
                }
            });
        });
    }

    setupEventListeners() {
        // Configuration modal
        const configFab = document.getElementById('config-fab');
        const closeConfig = document.getElementById('close-config');
        const validateConfig = document.getElementById('validate-config');
        const saveConfig = document.getElementById('save-config');

        if (configFab) {
            configFab.addEventListener('click', () => this.openConfigModal());
        }

        if (closeConfig) {
            closeConfig.addEventListener('click', () => this.closeConfigModal());
        }

        if (validateConfig) {
            validateConfig.addEventListener('click', () => this.validateConfiguration());
        }

        if (saveConfig) {
            saveConfig.addEventListener('click', () => this.saveConfiguration());
        }

        // Close modal when clicking outside
        if (this.configModal) {
            this.configModal.addEventListener('click', (e) => {
                if (e.target === this.configModal) {
                    this.closeConfigModal();
                }
            });
        }

        // Keyboard shortcuts
        document.addEventListener('keydown', (e) => {
            if (e.key === 'Escape' && this.configModal.classList.contains('show')) {
                this.closeConfigModal();
            }
        });

        // WebSocket connection state changes
        if (window.wsManager) {
            window.wsManager.onConnectionStateChange((state) => {
                this.handleConnectionStateChange(state);
            });
        }
    }

    async loadInitialData() {
        try {
            // Load current configuration
            await this.loadConfiguration();
            
            // Load initial status
            await this.loadStatus();
            
            // Load connections
            await this.loadConnections();
            
            // Load recent shares
            await this.loadShares();
            
            // Load alerts
            await this.loadAlerts();
            
        } catch (error) {
            console.error('Failed to load initial data:', error);
            this.showError('Failed to load dashboard data');
        }
    }

    async loadConfiguration() {
        try {
            const response = await fetch('/api/v1/config');
            if (response.ok) {
                this.currentConfig = await response.json();
            }
        } catch (error) {
            console.error('Failed to load configuration:', error);
        }
    }

    async loadStatus() {
        try {
            const response = await fetch('/api/v1/status');
            if (response.ok) {
                const status = await response.json();
                this.updateStatusDisplay(status);
            }
        } catch (error) {
            console.error('Failed to load status:', error);
        }
    }

    async loadConnections() {
        try {
            const response = await fetch('/api/v1/connections');
            if (response.ok) {
                const connections = await response.json();
                connections.forEach(conn => {
                    if (window.connectionManager) {
                        window.connectionManager.addConnection(conn);
                    }
                });
            }
        } catch (error) {
            console.error('Failed to load connections:', error);
        }
    }

    async loadShares() {
        try {
            const response = await fetch('/api/v1/shares?limit=20');
            if (response.ok) {
                const shares = await response.json();
                shares.forEach(share => {
                    if (window.shareManager) {
                        window.shareManager.addShare(share);
                    }
                });
            }
        } catch (error) {
            console.error('Failed to load shares:', error);
        }
    }

    async loadAlerts() {
        try {
            const response = await fetch('/api/v1/alerts');
            if (response.ok) {
                const alerts = await response.json();
                alerts.forEach(alert => {
                    if (window.alertManager) {
                        window.alertManager.addAlert(alert);
                    }
                });
            }
        } catch (error) {
            console.error('Failed to load alerts:', error);
        }
    }

    updateStatusDisplay(status) {
        // Update status cards
        this.updateElement('uptime', this.formatDuration(status.uptime));
        this.updateElement('connections', status.connections);
        this.updateElement('difficulty', status.current_difficulty.toFixed(2));
        this.updateElement('blocks-found', status.blocks_found);
        
        // Update metrics
        this.updateElement('hashrate', this.formatHashrate(status.hashrate));
        this.updateElement('total-shares', status.total_shares);
        this.updateElement('valid-shares', status.valid_shares);
        this.updateElement('invalid-shares', status.total_shares - status.valid_shares);
        
        // Calculate and update acceptance rate
        const acceptanceRate = status.total_shares > 0 
            ? ((status.valid_shares / status.total_shares) * 100).toFixed(1)
            : '0.0';
        this.updateElement('acceptance-rate', `${acceptanceRate}%`);
    }

    startPeriodicRefresh() {
        // Refresh data every 30 seconds (WebSocket provides real-time updates)
        setInterval(async () => {
            if (!window.wsManager || !window.wsManager.isConnected()) {
                await this.loadStatus();
            }
        }, 30000);
    }

    // Configuration Modal Methods
    openConfigModal() {
        if (this.configModal && this.configEditor) {
            // Load current configuration into editor
            if (this.currentConfig) {
                this.configEditor.value = JSON.stringify(this.currentConfig, null, 2);
            }
            
            this.configModal.classList.add('show');
            this.configEditor.focus();
        }
    }

    closeConfigModal() {
        if (this.configModal) {
            this.configModal.classList.remove('show');
            this.clearConfigErrors();
        }
    }

    async validateConfiguration() {
        const configText = this.configEditor.value.trim();
        
        if (!configText) {
            this.showConfigError('Configuration cannot be empty');
            return;
        }

        try {
            // Parse JSON to validate syntax
            const config = JSON.parse(configText);
            
            // Send to server for validation
            const response = await fetch('/api/v1/config', {
                method: 'PUT',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({
                    config: config,
                    validate_only: true
                })
            });

            const result = await response.json();
            
            if (result.success) {
                this.showConfigSuccess('Configuration is valid');
            } else {
                this.showConfigError(result.message, result.validation_errors);
            }
            
        } catch (error) {
            if (error instanceof SyntaxError) {
                this.showConfigError('Invalid JSON syntax: ' + error.message);
            } else {
                this.showConfigError('Validation failed: ' + error.message);
            }
        }
    }

    async saveConfiguration() {
        const configText = this.configEditor.value.trim();
        
        if (!configText) {
            this.showConfigError('Configuration cannot be empty');
            return;
        }

        try {
            // Parse JSON to validate syntax
            const config = JSON.parse(configText);
            
            // Send to server
            const response = await fetch('/api/v1/config', {
                method: 'PUT',
                headers: {
                    'Content-Type': 'application/json'
                },
                body: JSON.stringify({
                    config: config,
                    validate_only: false
                })
            });

            const result = await response.json();
            
            if (result.success) {
                this.currentConfig = config;
                this.showConfigSuccess('Configuration saved successfully');
                
                // Close modal after a short delay
                setTimeout(() => {
                    this.closeConfigModal();
                }, 1500);
                
            } else {
                this.showConfigError(result.message, result.validation_errors);
            }
            
        } catch (error) {
            if (error instanceof SyntaxError) {
                this.showConfigError('Invalid JSON syntax: ' + error.message);
            } else {
                this.showConfigError('Save failed: ' + error.message);
            }
        }
    }

    showConfigError(message, details = null) {
        const errorContainer = document.getElementById('config-errors');
        if (errorContainer) {
            let errorHtml = `<strong>Error:</strong> ${message}`;
            
            if (details && Array.isArray(details)) {
                errorHtml += '<ul>';
                details.forEach(detail => {
                    errorHtml += `<li>${detail}</li>`;
                });
                errorHtml += '</ul>';
            }
            
            errorContainer.innerHTML = errorHtml;
            errorContainer.classList.add('show');
        }
    }

    showConfigSuccess(message) {
        const errorContainer = document.getElementById('config-errors');
        if (errorContainer) {
            errorContainer.innerHTML = `<strong style="color: var(--success-color);">Success:</strong> ${message}`;
            errorContainer.style.borderLeftColor = 'var(--success-color)';
            errorContainer.style.backgroundColor = 'rgba(39, 174, 96, 0.1)';
            errorContainer.classList.add('show');
            
            // Reset styles after a delay
            setTimeout(() => {
                errorContainer.style.borderLeftColor = '';
                errorContainer.style.backgroundColor = '';
            }, 3000);
        }
    }

    clearConfigErrors() {
        const errorContainer = document.getElementById('config-errors');
        if (errorContainer) {
            errorContainer.classList.remove('show');
            errorContainer.innerHTML = '';
        }
    }

    // Connection state handling
    handleConnectionStateChange(state) {
        console.log('Connection state changed:', state);
        
        // Update UI based on connection state
        const statusCards = document.querySelectorAll('.status-card, .metric-card');
        
        if (state === 'disconnected') {
            statusCards.forEach(card => {
                card.classList.add('loading');
            });
        } else if (state === 'connected') {
            statusCards.forEach(card => {
                card.classList.remove('loading');
            });
        }
    }

    // Utility methods
    updateElement(id, value) {
        const element = document.getElementById(id);
        if (element) {
            element.textContent = value;
        }
    }

    formatDuration(seconds) {
        const hours = Math.floor(seconds / 3600);
        const minutes = Math.floor((seconds % 3600) / 60);
        const secs = seconds % 60;
        
        if (hours > 0) {
            return `${hours}h ${minutes}m`;
        } else if (minutes > 0) {
            return `${minutes}m ${secs}s`;
        } else {
            return `${secs}s`;
        }
    }

    formatHashrate(hashrate) {
        if (hashrate >= 1e15) {
            return `${(hashrate / 1e15).toFixed(2)} PH/s`;
        } else if (hashrate >= 1e12) {
            return `${(hashrate / 1e12).toFixed(2)} TH/s`;
        } else if (hashrate >= 1e9) {
            return `${(hashrate / 1e9).toFixed(2)} GH/s`;
        } else if (hashrate >= 1e6) {
            return `${(hashrate / 1e6).toFixed(2)} MH/s`;
        } else if (hashrate >= 1e3) {
            return `${(hashrate / 1e3).toFixed(2)} KH/s`;
        } else {
            return `${hashrate.toFixed(2)} H/s`;
        }
    }

    showError(message) {
        // Create a temporary error notification
        const notification = document.createElement('div');
        notification.className = 'error-notification';
        notification.textContent = message;
        notification.style.cssText = `
            position: fixed;
            top: 20px;
            right: 20px;
            background: var(--danger-color);
            color: white;
            padding: 1rem;
            border-radius: var(--border-radius);
            box-shadow: var(--shadow-hover);
            z-index: 1000;
            animation: slideInRight 0.3s ease;
        `;
        
        document.body.appendChild(notification);
        
        // Remove after 5 seconds
        setTimeout(() => {
            notification.style.animation = 'slideOutRight 0.3s ease';
            setTimeout(() => {
                document.body.removeChild(notification);
            }, 300);
        }, 5000);
    }

    // Export dashboard data for debugging
    exportDashboardData() {
        const data = {
            config: this.currentConfig,
            charts: {},
            connections: window.connectionManager ? Array.from(window.connectionManager.connections.values()) : [],
            shares: window.shareManager ? window.shareManager.shares : [],
            alerts: window.alertManager ? Array.from(window.alertManager.alerts.values()) : []
        };

        // Export chart data
        if (window.chartManager) {
            data.charts.hashrate = window.chartManager.exportChartData('hashrate');
            data.charts.acceptance = window.chartManager.exportChartData('acceptance');
        }

        return data;
    }
}

// Global dashboard manager
window.dashboardManager = null;

// Initialize dashboard when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    window.dashboardManager = new DashboardManager();
    
    // Add CSS animations
    const style = document.createElement('style');
    style.textContent = `
        @keyframes slideInRight {
            from { transform: translateX(100%); opacity: 0; }
            to { transform: translateX(0); opacity: 1; }
        }
        
        @keyframes slideOutRight {
            from { transform: translateX(0); opacity: 1; }
            to { transform: translateX(100%); opacity: 0; }
        }
        
        .error-notification {
            animation: slideInRight 0.3s ease;
        }
    `;
    document.head.appendChild(style);
});

// Global utility functions
function refreshDashboard() {
    if (window.dashboardManager) {
        window.dashboardManager.loadInitialData();
    }
}

function exportData() {
    if (window.dashboardManager) {
        const data = window.dashboardManager.exportDashboardData();
        const blob = new Blob([JSON.stringify(data, null, 2)], { type: 'application/json' });
        const url = URL.createObjectURL(blob);
        
        const a = document.createElement('a');
        a.href = url;
        a.download = `sv2d-dashboard-${new Date().toISOString().split('T')[0]}.json`;
        document.body.appendChild(a);
        a.click();
        document.body.removeChild(a);
        URL.revokeObjectURL(url);
    }
}

// Keyboard shortcuts
document.addEventListener('keydown', (e) => {
    if (e.ctrlKey || e.metaKey) {
        switch (e.key) {
            case 'r':
                e.preventDefault();
                refreshDashboard();
                break;
            case 'e':
                e.preventDefault();
                exportData();
                break;
            case ',':
                e.preventDefault();
                if (window.dashboardManager) {
                    window.dashboardManager.openConfigModal();
                }
                break;
        }
    }
});