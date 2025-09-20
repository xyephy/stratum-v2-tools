# sv2-web - Stratum V2 Web Dashboard and API

A comprehensive web dashboard and REST API for the sv2d Stratum V2 mining daemon, providing real-time monitoring, configuration management, and WebSocket-based live updates.

## Features

### 🌐 Web Dashboard
- **Real-time monitoring** with live charts and statistics
- **Connection management** with detailed miner information
- **Share tracking** and acceptance rate monitoring
- **System health** and performance metrics
- **Alert management** with configurable thresholds
- **Configuration interface** for daemon settings

### 🔌 REST API
- **Comprehensive endpoints** for all daemon operations
- **JSON responses** with detailed error handling
- **Pagination support** for large datasets
- **Input validation** and sanitization
- **CORS support** for cross-origin requests

### ⚡ WebSocket Interface
- **Real-time updates** for live monitoring
- **Event subscriptions** for targeted notifications
- **Bidirectional communication** with the daemon
- **Connection management** with automatic reconnection
- **Heartbeat monitoring** for connection health

## API Endpoints

### System Status
- `GET /api/v1/health` - Health check and system information
- `GET /api/v1/status` - Daemon status and mining statistics
- `GET /api/v1/mining/stats` - Detailed mining performance metrics

### Connection Management
- `GET /api/v1/connections` - List active connections
- `GET /api/v1/connections/{id}` - Get specific connection details

### Share Management
- `GET /api/v1/shares` - List recent shares with filtering
- `GET /api/v1/shares/stats` - Share statistics and acceptance rates

### Work Template Management
- `GET /api/v1/templates` - List work templates
- `GET /api/v1/templates/{id}` - Get specific template
- `POST /api/v1/templates/custom` - Submit custom work template

### Monitoring and Alerts
- `GET /api/v1/metrics` - Performance metrics
- `GET /api/v1/alerts` - System alerts and notifications

### Configuration Management
- `GET /api/v1/config` - Get current configuration
- `PUT /api/v1/config` - Update configuration with validation

## WebSocket Events

### Event Types
- `status` - Daemon status updates
- `connection` - Connection events (added, updated, removed)
- `share` - Share submission events
- `metrics` - Performance metric updates
- `mining_stats` - Mining statistics updates
- `alert` - Alert creation and resolution
- `heartbeat` - Connection keepalive

### Subscription Management
```json
{
  "action": "Subscribe",
  "events": ["status", "connection", "share"]
}
```

```json
{
  "action": "Unsubscribe", 
  "events": ["metrics"]
}
```

## Usage

### Starting the Web Server

```bash
# Run the web dashboard
cargo run --bin sv2-web

# Or with custom configuration
DATABASE_URL=sqlite://custom.db cargo run --bin sv2-web
```

The dashboard will be available at `http://localhost:8080`

### API Examples

#### Get System Status
```bash
curl http://localhost:8080/api/v1/status
```

#### List Active Connections
```bash
curl http://localhost:8080/api/v1/connections
```

#### Get Share Statistics
```bash
curl http://localhost:8080/api/v1/shares/stats
```

#### Submit Custom Template
```bash
curl -X POST http://localhost:8080/api/v1/templates/custom \
  -H "Content-Type: application/json" \
  -d '{
    "transactions": ["..."],
    "difficulty": 1.0
  }'
```

#### Update Configuration
```bash
curl -X PUT http://localhost:8080/api/v1/config \
  -H "Content-Type: application/json" \
  -d '{
    "config": {...},
    "validate_only": false
  }'
```

### WebSocket Connection

```javascript
const ws = new WebSocket('ws://localhost:8080/ws');

// Subscribe to events
ws.send(JSON.stringify({
  action: "Subscribe",
  events: ["status", "connection", "share"]
}));

// Handle incoming messages
ws.onmessage = function(event) {
  const data = JSON.parse(event.data);
  console.log('Received:', data.type, data.data);
};
```

### Running the API Demo

```bash
# Start the sv2-web server first
cargo run --bin sv2-web

# In another terminal, run the demo
cargo run --example api_demo
```

## Configuration

The web server can be configured through environment variables:

- `DATABASE_URL` - Database connection string (default: `sqlite://sv2d.db`)
- `BIND_ADDRESS` - Server bind address (default: `127.0.0.1:8080`)
- `LOG_LEVEL` - Logging level (default: `info`)

## Testing

### Unit Tests
```bash
cargo test --lib
```

### Integration Tests
```bash
# API integration tests
cargo test --test api_integration_tests

# WebSocket integration tests  
cargo test --test websocket_integration_tests
```

### All Tests
```bash
cargo test
```

## Architecture

### Components
- **handlers.rs** - REST API endpoint handlers
- **websocket.rs** - WebSocket server and event broadcasting
- **main.rs** - Server initialization and routing

### Key Features
- **Async/await** throughout for high performance
- **Error handling** with proper HTTP status codes
- **Input validation** and sanitization
- **CORS support** for web applications
- **Structured logging** with correlation IDs
- **Graceful shutdown** handling

### Dependencies
- **axum** - Modern web framework
- **tokio** - Async runtime
- **serde** - Serialization/deserialization
- **sqlx** - Database connectivity
- **tower-http** - HTTP middleware
- **tokio-tungstenite** - WebSocket support

## Security Considerations

- Input validation on all endpoints
- SQL injection prevention through parameterized queries
- Rate limiting support (configurable)
- CORS policy configuration
- Sensitive data redaction in logs
- Optional TLS/SSL support

## Performance

- **Concurrent connections** - Handles 1000+ simultaneous connections
- **Memory efficient** - Uses streaming for large datasets
- **Database pooling** - Optimized database connection management
- **Caching** - Strategic caching for frequently accessed data
- **Compression** - Response compression for bandwidth efficiency

## Monitoring

The web interface provides comprehensive monitoring capabilities:

- **Real-time dashboards** with live updating charts
- **Historical data** with configurable time ranges
- **Alert thresholds** with customizable notifications
- **Performance metrics** with Prometheus compatibility
- **Health checks** for system components

## Development

### Adding New Endpoints

1. Add handler function in `handlers.rs`
2. Define request/response types with proper validation
3. Add route in `main.rs`
4. Write integration tests
5. Update API documentation

### Adding WebSocket Events

1. Add event type to `WebSocketMessage` enum
2. Implement broadcasting method in `WebSocketBroadcaster`
3. Add event handling in daemon components
4. Write tests for new event types
5. Update client documentation

## License

This project is part of the sv2d Stratum V2 toolkit and follows the same licensing terms.