# Signet Testing Guide

This guide walks through testing the SV2D Stratum V2 Toolkit with a Bitcoin Core node on signet.

## Prerequisites

- Bitcoin Core 24.0+ (with signet support)
- Rust toolchain 1.70+
- 4GB RAM minimum
- 20GB disk space for signet blockchain

## Setup Bitcoin Core for Signet

### 1. Install Bitcoin Core

**macOS (Homebrew):**
```bash
brew install bitcoin
```

**Linux (Ubuntu/Debian):**
```bash
wget https://bitcoincore.org/bin/bitcoin-core-25.1/bitcoin-25.1-x86_64-linux-gnu.tar.gz
tar -xzf bitcoin-25.1-x86_64-linux-gnu.tar.gz
sudo cp bitcoin-25.1/bin/* /usr/local/bin/
```

### 2. Configure Bitcoin Core for Signet

Create Bitcoin configuration file:

**macOS:**
```bash
mkdir -p ~/Library/Application\ Support/Bitcoin
cat > ~/Library/Application\ Support/Bitcoin/bitcoin.conf << EOF
# Signet configuration for SV2D testing
signet=1
server=1
rpcuser=sv2d_test
rpcpassword=sv2d_test_password_change_me
rpcallowip=127.0.0.1
rpcbind=127.0.0.1:38332

# Enable getblocktemplate
blockfilterindex=1
txindex=1

# Logging
debug=rpc
debug=net

# Mining
generate=0
EOF
```

**Linux:**
```bash
mkdir -p ~/.bitcoin
cat > ~/.bitcoin/bitcoin.conf << EOF
# Signet configuration for SV2D testing
signet=1
server=1
rpcuser=sv2d_test
rpcpassword=sv2d_test_password_change_me
rpcallowip=127.0.0.1
rpcbind=127.0.0.1:38332

# Enable getblocktemplate
blockfilterindex=1
txindex=1

# Logging
debug=rpc
debug=net

# Mining
generate=0
EOF
```

### 3. Start Bitcoin Core

```bash
bitcoind -daemon
```

Wait for initial sync (this may take 30-60 minutes for signet):
```bash
bitcoin-cli -signet getblockchaininfo
```

## Build and Configure SV2D

### 1. Build the Project

```bash
cargo build --release
```

### 2. Create Signet Configuration

```bash
mkdir -p config
cat > config/signet_solo.toml << EOF
# SV2D Signet Solo Mining Configuration

[server]
bind_address = "0.0.0.0:4254"
max_connections = 100

[mode]
operation_mode = "Solo"

[solo]
bitcoin_rpc_url = "http://127.0.0.1:38332"
bitcoin_rpc_user = "sv2d_test"
bitcoin_rpc_password = "sv2d_test_password_change_me"
payout_address = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"  # Replace with your signet address

[logging]
level = "debug"
format = "pretty"

[database]
url = "sqlite://sv2d_signet.db"

[metrics]
enabled = true
bind_address = "0.0.0.0:9090"

[health]
enabled = true
bind_address = "0.0.0.0:8081"
EOF
```

### 3. Get a Signet Address and Coins

Generate a signet address:
```bash
bitcoin-cli -signet getnewaddress "sv2d_test" "bech32"
```

Get signet coins from faucet:
- Visit: https://signet.bc-2.jp/
- Or use: https://alt.signetfaucet.com/
- Send coins to your generated address

Update the `payout_address` in the config with your address.

## Testing Scenarios

### Test 1: Basic Connectivity

1. **Start SV2D:**
   ```bash
   ./target/release/sv2d --config config/signet_solo.toml
   ```

2. **Check status:**
   ```bash
   ./target/release/sv2-cli status
   ```

3. **Verify Bitcoin RPC connection:**
   ```bash
   ./target/release/sv2-cli bitcoin-info
   ```

### Test 2: Solo Mining Setup

1. **Check mining readiness:**
   ```bash
   ./target/release/sv2-cli mining-status
   ```

2. **Start mining (if you have a miner):**
   ```bash
   # Connect your Stratum V1 miner to localhost:4254
   # Or use cpuminer for testing:
   cpuminer -a sha256d -o stratum+tcp://localhost:4254 -u test -p test
   ```

### Test 3: Work Template Generation

1. **Monitor work templates:**
   ```bash
   ./target/release/sv2-cli monitor --templates
   ```

2. **Check template details:**
   ```bash
   ./target/release/sv2-cli template-info
   ```

### Test 4: Share Validation

1. **Monitor shares:**
   ```bash
   ./target/release/sv2-cli monitor --shares
   ```

2. **Check share statistics:**
   ```bash
   ./target/release/sv2-cli stats
   ```

## Automated Testing Script

Create an automated test script:

```bash
cat > test_signet.sh << 'EOF'
#!/bin/bash
set -euo pipefail

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*"
}

# Check Bitcoin Core is running
if ! bitcoin-cli -signet getblockchaininfo >/dev/null 2>&1; then
    log "ERROR: Bitcoin Core not running or not synced"
    exit 1
fi

log "Bitcoin Core is running and synced"

# Build SV2D
log "Building SV2D..."
cargo build --release

# Start SV2D in background
log "Starting SV2D..."
./target/release/sv2d --config config/signet_solo.toml &
SV2D_PID=$!

# Wait for startup
sleep 5

# Test basic connectivity
log "Testing basic connectivity..."
if ./target/release/sv2-cli status; then
    log "✓ SV2D status check passed"
else
    log "✗ SV2D status check failed"
    kill $SV2D_PID 2>/dev/null || true
    exit 1
fi

# Test Bitcoin RPC connection
log "Testing Bitcoin RPC connection..."
if ./target/release/sv2-cli bitcoin-info; then
    log "✓ Bitcoin RPC connection test passed"
else
    log "✗ Bitcoin RPC connection test failed"
    kill $SV2D_PID 2>/dev/null || true
    exit 1
fi

# Test work template generation
log "Testing work template generation..."
if ./target/release/sv2-cli template-info; then
    log "✓ Work template generation test passed"
else
    log "✗ Work template generation test failed"
    kill $SV2D_PID 2>/dev/null || true
    exit 1
fi

# Run for 30 seconds to collect metrics
log "Running for 30 seconds to collect metrics..."
sleep 30

# Check final stats
log "Final statistics:"
./target/release/sv2-cli stats

# Cleanup
log "Stopping SV2D..."
kill $SV2D_PID 2>/dev/null || true
wait $SV2D_PID 2>/dev/null || true

log "✓ All tests completed successfully!"
EOF

chmod +x test_signet.sh
```

## Monitoring and Debugging

### Real-time Monitoring

1. **SV2D logs:**
   ```bash
   tail -f sv2d.log
   ```

2. **Bitcoin Core logs:**
   ```bash
   tail -f ~/.bitcoin/signet/debug.log
   ```

3. **Metrics:**
   ```bash
   curl http://localhost:9090/metrics
   ```

4. **Health check:**
   ```bash
   curl http://localhost:8081/health
   ```

### Web Dashboard

If sv2-web is running:
```bash
./target/release/sv2-web --config config/signet_solo.toml &
```

Access dashboard at: http://localhost:8080

## Troubleshooting

### Common Issues

1. **Bitcoin Core not synced:**
   ```bash
   bitcoin-cli -signet getblockchaininfo | grep blocks
   ```

2. **RPC connection failed:**
   - Check bitcoin.conf credentials
   - Verify Bitcoin Core is running with RPC enabled
   - Test RPC manually: `bitcoin-cli -signet getblockchaininfo`

3. **No work templates:**
   - Ensure Bitcoin Core has recent blocks
   - Check if mempool has transactions
   - Verify payout address is valid

4. **Permission denied:**
   ```bash
   chmod +x target/release/sv2d target/release/sv2-cli
   ```

### Debug Commands

```bash
# Check Bitcoin Core status
bitcoin-cli -signet getblockchaininfo
bitcoin-cli -signet getmempoolinfo
bitcoin-cli -signet getblocktemplate

# Check SV2D status
./target/release/sv2-cli status --verbose
./target/release/sv2-cli debug-info

# Network connectivity
netstat -an | grep 4254
netstat -an | grep 38332
```

## Expected Results

After successful testing, you should see:

1. ✅ SV2D connects to Bitcoin Core RPC
2. ✅ Work templates are generated from signet blocks
3. ✅ Stratum V2 server accepts connections on port 4254
4. ✅ Share validation works correctly
5. ✅ Metrics are collected and exposed
6. ✅ Web dashboard shows real-time data

## Next Steps

Once basic functionality is verified:

1. Test with real mining hardware (Bitaxe, etc.)
2. Test protocol translation (SV1 ↔ SV2)
3. Test proxy mode with upstream pools
4. Performance testing with multiple connections
5. Failover and recovery testing

## Safety Notes

- Signet coins have no real value
- Use only for testing purposes
- Don't use mainnet for initial testing
- Keep RPC credentials secure
- Monitor resource usage during testing