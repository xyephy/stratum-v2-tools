// WebSocket connection management for real-time updates

class WebSocketManager {
    constructor(url = 'ws://localhost:8080/ws') {
        this.url = url;
        this.ws = null;
        this.reconnectAttempts = 0;
        this.maxReconnectAttempts = 10;
        this.reconnectDelay = 1000; // Start with 1 second
        this.maxReconnectDelay = 30000; // Max 30 seconds
        this.isConnecting = false;
        this.subscriptions = new Set(['*']); // Subscribe to all events by default
        this.messageHandlers = new Map();
        this.connectionStateCallbacks = [];
        
        this.connect();
    }

    connect() {
        if (this.isConnecting || (this.ws && this.ws.readyState === WebSocket.CONNECTING)) {
            return;
        }

        this.isConnecting = true;
        this.updateConnectionStatus('connecting');

        try {
            this.ws = new WebSocket(this.url);
            this.setupEventHandlers();
        } catch (error) {
            console.error('Failed to create WebSocket connection:', error);
            this.handleConnectionError();
        }
    }

    setupEventHandlers() {
        this.ws.onopen = () => {
            console.log('WebSocket connected');
            this.isConnecting = false;
            this.reconnectAttempts = 0;
            this.reconnectDelay = 1000;
            this.updateConnectionStatus('connected');
            
            // Subscribe to events
            this.subscribe(Array.from(this.subscriptions));
        };

        this.ws.onmessage = (event) => {
            try {
                const message = JSON.parse(event.data);
                this.handleMessage(message);
            } catch (error) {
                console.error('Failed to parse WebSocket message:', error);
            }
        };

        this.ws.onclose = (event) => {
            console.log('WebSocket disconnected:', event.code, event.reason);
            this.isConnecting = false;
            this.updateConnectionStatus('disconnected');
            
            if (!event.wasClean && this.reconnectAttempts < this.maxReconnectAttempts) {
                this.scheduleReconnect();
            }
        };

        this.ws.onerror = (error) => {
            console.error('WebSocket error:', error);
            this.handleConnectionError();
        };
    }

    handleConnectionError() {
        this.isConnecting = false;
        this.updateConnectionStatus('disconnected');
        
        if (this.reconnectAttempts < this.maxReconnectAttempts) {
            this.scheduleReconnect();
        }
    }

    scheduleReconnect() {
        this.reconnectAttempts++;
        const delay = Math.min(this.reconnectDelay * Math.pow(2, this.reconnectAttempts - 1), this.maxReconnectDelay);
        
        console.log(`Reconnecting in ${delay}ms (attempt ${this.reconnectAttempts}/${this.maxReconnectAttempts})`);
        
        setTimeout(() => {
            this.connect();
        }, delay);
    }

    updateConnectionStatus(status) {
        const indicator = document.getElementById('connection-indicator');
        const text = document.getElementById('connection-text');
        
        if (indicator) {
            indicator.className = `status-indicator ${status}`;
        }
        
        if (text) {
            switch (status) {
                case 'connected':
                    text.textContent = 'Connected';
                    break;
                case 'connecting':
                    text.textContent = 'Connecting...';
                    break;
                case 'disconnected':
                    text.textContent = 'Disconnected';
                    break;
            }
        }

        // Notify callbacks
        this.connectionStateCallbacks.forEach(callback => {
            try {
                callback(status);
            } catch (error) {
                console.error('Error in connection state callback:', error);
            }
        });
    }

    handleMessage(message) {
        console.log('Received WebSocket message:', message);

        // Handle specific message types
        switch (message.type) {
            case 'Status':
                this.handleStatusUpdate(message.data);
                break;
            case 'ConnectionAdded':
                this.handleConnectionAdded(message.data);
                break;
            case 'ConnectionUpdated':
                this.handleConnectionUpdated(message.data);
                break;
            case 'ConnectionRemoved':
                this.handleConnectionRemoved(message.data);
                break;
            case 'ShareSubmitted':
                this.handleShareSubmitted(message.data);
                break;
            case 'MetricsUpdate':
                this.handleMetricsUpdate(message.data);
                break;
            case 'MiningStatsUpdate':
                this.handleMiningStatsUpdate(message.data);
                break;
            case 'AlertCreated':
                this.handleAlertCreated(message.data);
                break;
            case 'AlertResolved':
                this.handleAlertResolved(message.data);
                break;
            case 'Heartbeat':
                this.handleHeartbeat(message.data);
                break;
            case 'Error':
                this.handleError(message.data);
                break;
            case 'Subscribed':
                this.handleSubscribed(message.data);
                break;
        }

        // Call registered handlers
        const handlers = this.messageHandlers.get(message.type) || [];
        handlers.forEach(handler => {
            try {
                handler(message.data);
            } catch (error) {
                console.error(`Error in message handler for ${message.type}:`, error);
            }
        });
    }

    handleStatusUpdate(status) {
        // Update status display
        this.updateElement('uptime', this.formatDuration(status.uptime));
        this.updateElement('connections', status.connections);
        this.updateElement('difficulty', status.current_difficulty.toFixed(2));
        this.updateElement('blocks-found', status.blocks_found);
        this.updateElement('hashrate', this.formatHashrate(status.hashrate));
        this.updateElement('total-shares', status.total_shares);
        this.updateElement('valid-shares', status.valid_shares);
        this.updateElement('invalid-shares', status.total_shares - status.valid_shares);

        // Calculate acceptance rate
        const acceptanceRate = status.total_shares > 0 
            ? ((status.valid_shares / status.total_shares) * 100).toFixed(1)
            : '0.0';
        this.updateElement('acceptance-rate', `${acceptanceRate}%`);

        // Update charts if available
        if (window.chartManager) {
            window.chartManager.updateHashrateChart(status.hashrate);
            window.chartManager.updateAcceptanceChart(parseFloat(acceptanceRate));
        }
    }

    handleConnectionAdded(connection) {
        console.log('New connection added:', connection);
        if (window.connectionManager) {
            window.connectionManager.addConnection(connection);
        }
    }

    handleConnectionUpdated(connection) {
        console.log('Connection updated:', connection);
        if (window.connectionManager) {
            window.connectionManager.updateConnection(connection);
        }
    }

    handleConnectionRemoved(data) {
        console.log('Connection removed:', data.id);
        if (window.connectionManager) {
            window.connectionManager.removeConnection(data.id);
        }
    }

    handleShareSubmitted(share) {
        console.log('New share submitted:', share);
        if (window.shareManager) {
            window.shareManager.addShare(share);
        }
    }

    handleMetricsUpdate(metrics) {
        console.log('Metrics updated:', metrics);
        if (window.chartManager) {
            window.chartManager.updateMetrics(metrics);
        }
    }

    handleMiningStatsUpdate(stats) {
        console.log('Mining stats updated:', stats);
        this.updateElement('efficiency', `${(stats.efficiency * 100).toFixed(1)}%`);
        this.updateElement('shares-per-minute', stats.shares_per_minute.toFixed(1));
    }

    handleAlertCreated(alert) {
        console.log('New alert:', alert);
        if (window.alertManager) {
            window.alertManager.addAlert(alert);
        }
    }

    handleAlertResolved(data) {
        console.log('Alert resolved:', data.id);
        if (window.alertManager) {
            window.alertManager.resolveAlert(data.id);
        }
    }

    handleHeartbeat(data) {
        // Update last heartbeat time
        this.lastHeartbeat = new Date(data.timestamp);
    }

    handleError(data) {
        console.error('WebSocket error message:', data.message);
        // Could show a toast notification here
    }

    handleSubscribed(data) {
        console.log('Subscribed to events:', data.subscriptions);
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

    // Public API methods
    subscribe(events) {
        if (!Array.isArray(events)) {
            events = [events];
        }

        events.forEach(event => this.subscriptions.add(event));

        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.send({
                action: 'Subscribe',
                events: events
            });
        }
    }

    unsubscribe(events) {
        if (!Array.isArray(events)) {
            events = [events];
        }

        events.forEach(event => this.subscriptions.delete(event));

        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.send({
                action: 'Unsubscribe',
                events: events
            });
        }
    }

    onMessage(messageType, handler) {
        if (!this.messageHandlers.has(messageType)) {
            this.messageHandlers.set(messageType, []);
        }
        this.messageHandlers.get(messageType).push(handler);
    }

    offMessage(messageType, handler) {
        const handlers = this.messageHandlers.get(messageType);
        if (handlers) {
            const index = handlers.indexOf(handler);
            if (index > -1) {
                handlers.splice(index, 1);
            }
        }
    }

    onConnectionStateChange(callback) {
        this.connectionStateCallbacks.push(callback);
    }

    send(data) {
        if (this.ws && this.ws.readyState === WebSocket.OPEN) {
            this.ws.send(JSON.stringify(data));
        } else {
            console.warn('WebSocket not connected, cannot send message:', data);
        }
    }

    ping() {
        this.send({ action: 'Ping' });
    }

    getStatus() {
        this.send({ action: 'GetStatus' });
    }

    disconnect() {
        if (this.ws) {
            this.ws.close(1000, 'Client disconnect');
        }
    }

    isConnected() {
        return this.ws && this.ws.readyState === WebSocket.OPEN;
    }
}

// Global WebSocket manager instance
window.wsManager = null;

// Initialize WebSocket connection when DOM is loaded
document.addEventListener('DOMContentLoaded', () => {
    window.wsManager = new WebSocketManager();
    
    // Set up periodic ping to keep connection alive
    setInterval(() => {
        if (window.wsManager && window.wsManager.isConnected()) {
            window.wsManager.ping();
        }
    }, 30000); // Ping every 30 seconds
});

// Export for use in other modules
if (typeof module !== 'undefined' && module.exports) {
    module.exports = WebSocketManager;
}