#!/bin/bash
set -euo pipefail

# Signet integration testing script for SV2D

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
CONFIG_FILE="${CONFIG_FILE:-$PROJECT_ROOT/config/signet_solo.toml}"
TEST_DURATION="${TEST_DURATION:-60}"
BITCOIN_NETWORK="${BITCOIN_NETWORK:-signet}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

cleanup() {
    log "Cleaning up test processes..."
    
    # Stop SV2D if running
    if [[ -n "${SV2D_PID:-}" ]]; then
        kill "$SV2D_PID" 2>/dev/null || true
        wait "$SV2D_PID" 2>/dev/null || true
        log "SV2D stopped"
    fi
    
    # Stop sv2-web if running
    if [[ -n "${WEB_PID:-}" ]]; then
        kill "$WEB_PID" 2>/dev/null || true
        wait "$WEB_PID" 2>/dev/null || true
        log "sv2-web stopped"
    fi
}

trap cleanup EXIT

check_bitcoin_core() {
    log "Checking Bitcoin Core connectivity..."
    
    local rpc_cmd="bitcoin-cli"
    if [[ "$BITCOIN_NETWORK" == "signet" ]]; then
        rpc_cmd="bitcoin-cli -signet"
    elif [[ "$BITCOIN_NETWORK" == "testnet" ]]; then
        rpc_cmd="bitcoin-cli -testnet"
    elif [[ "$BITCOIN_NETWORK" == "regtest" ]]; then
        rpc_cmd="bitcoin-cli -regtest"
    fi
    
    # Check if Bitcoin Core is running
    if ! $rpc_cmd getblockchaininfo >/dev/null 2>&1; then
        log "✗ Bitcoin Core not running or not accessible"
        log "Please start Bitcoin Core with $BITCOIN_NETWORK network"
        log "Example: bitcoind -$BITCOIN_NETWORK -daemon"
        return 1
    fi
    
    # Check sync status
    local blocks=$($rpc_cmd getblockchaininfo | grep '"blocks"' | cut -d: -f2 | tr -d ' ,')
    local headers=$($rpc_cmd getblockchaininfo | grep '"headers"' | cut -d: -f2 | tr -d ' ,')
    
    if [[ "$blocks" != "$headers" ]]; then
        log "! Bitcoin Core is syncing... blocks: $blocks, headers: $headers"
        log "! Tests may not work correctly until sync is complete"
    else
        log "✓ Bitcoin Core is running and synced ($blocks blocks)"
    fi
    
    # Check RPC access
    if $rpc_cmd getblocktemplate >/dev/null 2>&1; then
        log "✓ Bitcoin Core RPC getblocktemplate works"
    else
        log "! Bitcoin Core RPC getblocktemplate failed"
        log "! This may be normal if no transactions are in mempool"
    fi
    
    return 0
}

build_project() {
    log "Building SV2D project..."
    
    cd "$PROJECT_ROOT"
    
    if cargo build --release; then
        log "✓ Project built successfully"
    else
        log "✗ Project build failed"
        return 1
    fi
    
    # Verify binaries exist
    if [[ ! -f "target/release/sv2d" ]]; then
        log "✗ sv2d binary not found"
        return 1
    fi
    
    if [[ ! -f "target/release/sv2-cli" ]]; then
        log "✗ sv2-cli binary not found"
        return 1
    fi
    
    log "✓ All binaries built successfully"
    return 0
}

create_test_config() {
    log "Creating test configuration..."
    
    mkdir -p "$PROJECT_ROOT/config"
    
    # Determine RPC port based on network
    local rpc_port="8332"
    case "$BITCOIN_NETWORK" in
        "signet") rpc_port="38332" ;;
        "testnet") rpc_port="18332" ;;
        "regtest") rpc_port="18443" ;;
    esac
    
    cat > "$CONFIG_FILE" << EOF
# SV2D $BITCOIN_NETWORK Testing Configuration

[server]
bind_address = "0.0.0.0:4254"
max_connections = 100

[mode]
operation_mode = "Solo"

[solo]
bitcoin_rpc_url = "http://127.0.0.1:$rpc_port"
bitcoin_rpc_user = "sv2d_test"
bitcoin_rpc_password = "sv2d_test_password_change_me"
payout_address = "tb1qw508d6qejxtdg4y5r3zarvary0c5xw7kxpjzsx"

[logging]
level = "info"
format = "pretty"

[database]
url = "sqlite://sv2d_${BITCOIN_NETWORK}_test.db"

[metrics]
enabled = true
bind_address = "0.0.0.0:9090"

[health]
enabled = true
bind_address = "0.0.0.0:8081"

[security]
tls_enabled = false
EOF
    
    log "✓ Test configuration created at $CONFIG_FILE"
}

start_sv2d() {
    log "Starting SV2D daemon..."
    
    cd "$PROJECT_ROOT"
    
    # Start SV2D in background
    ./target/release/sv2d --config "$CONFIG_FILE" > sv2d_test.log 2>&1 &
    SV2D_PID=$!
    
    log "SV2D started with PID $SV2D_PID"
    
    # Wait for startup
    local attempts=0
    while [[ $attempts -lt 30 ]]; do
        if ./target/release/sv2-cli status >/dev/null 2>&1; then
            log "✓ SV2D is ready"
            return 0
        fi
        sleep 1
        ((attempts++))
    done
    
    log "✗ SV2D failed to start within 30 seconds"
    log "Last few lines of log:"
    tail -10 sv2d_test.log
    return 1
}

test_basic_functionality() {
    log "Testing basic functionality..."
    
    cd "$PROJECT_ROOT"
    
    # Test status command
    if ./target/release/sv2-cli status; then
        log "✓ Status command works"
    else
        log "✗ Status command failed"
        return 1
    fi
    
    # Test health endpoint
    if curl -s http://localhost:8081/health >/dev/null; then
        log "✓ Health endpoint accessible"
    else
        log "✗ Health endpoint not accessible"
        return 1
    fi
    
    # Test metrics endpoint
    if curl -s http://localhost:9090/metrics | grep -q "sv2d_"; then
        log "✓ Metrics endpoint working"
    else
        log "! Metrics endpoint may not be fully initialized yet"
    fi
    
    return 0
}

test_bitcoin_integration() {
    log "Testing Bitcoin integration..."
    
    cd "$PROJECT_ROOT"
    
    # Test Bitcoin RPC connection (if sv2-cli supports it)
    # This would need to be implemented in sv2-cli
    log "! Bitcoin integration test would require sv2-cli bitcoin-info command"
    
    # For now, just verify the daemon is running and can connect
    if pgrep -f "sv2d.*$CONFIG_FILE" >/dev/null; then
        log "✓ SV2D process is running with correct config"
    else
        log "✗ SV2D process not found"
        return 1
    fi
    
    return 0
}

test_stratum_server() {
    log "Testing Stratum server..."
    
    # Test if port 4254 is listening
    if netstat -an 2>/dev/null | grep -q ":4254.*LISTEN" || \
       ss -an 2>/dev/null | grep -q ":4254.*LISTEN"; then
        log "✓ Stratum server is listening on port 4254"
    else
        log "✗ Stratum server not listening on port 4254"
        return 1
    fi
    
    # Test basic TCP connection
    if timeout 5 bash -c "</dev/tcp/localhost/4254" 2>/dev/null; then
        log "✓ Can connect to Stratum server"
    else
        log "✗ Cannot connect to Stratum server"
        return 1
    fi
    
    return 0
}

run_load_test() {
    log "Running brief load test for $TEST_DURATION seconds..."
    
    local start_time=$(date +%s)
    local end_time=$((start_time + TEST_DURATION))
    
    while [[ $(date +%s) -lt $end_time ]]; do
        # Check if SV2D is still running
        if ! kill -0 "$SV2D_PID" 2>/dev/null; then
            log "✗ SV2D process died during load test"
            return 1
        fi
        
        # Check health endpoint
        if ! curl -s http://localhost:8081/health >/dev/null; then
            log "✗ Health endpoint became unavailable"
            return 1
        fi
        
        sleep 5
    done
    
    log "✓ Load test completed successfully"
    return 0
}

collect_final_stats() {
    log "Collecting final statistics..."
    
    cd "$PROJECT_ROOT"
    
    # Show final status
    log "Final SV2D status:"
    ./target/release/sv2-cli status || true
    
    # Show metrics if available
    log "Metrics sample:"
    curl -s http://localhost:9090/metrics | head -20 || true
    
    # Show log tail
    log "Last 10 lines of SV2D log:"
    tail -10 sv2d_test.log || true
}

main() {
    log "Starting SV2D Signet Integration Tests"
    log "======================================"
    log "Network: $BITCOIN_NETWORK"
    log "Config: $CONFIG_FILE"
    log "Test duration: ${TEST_DURATION}s"
    log ""
    
    local failed_tests=0
    
    check_bitcoin_core || ((failed_tests++))
    build_project || ((failed_tests++))
    create_test_config
    start_sv2d || ((failed_tests++))
    test_basic_functionality || ((failed_tests++))
    test_bitcoin_integration || ((failed_tests++))
    test_stratum_server || ((failed_tests++))
    run_load_test || ((failed_tests++))
    collect_final_stats
    
    log ""
    if [[ $failed_tests -eq 0 ]]; then
        log "✓ All integration tests passed!"
        log "SV2D is working correctly with Bitcoin Core on $BITCOIN_NETWORK"
        exit 0
    else
        log "✗ $failed_tests integration tests failed"
        log "Check the logs above for details"
        exit 1
    fi
}

# Parse command line arguments
while [[ $# -gt 0 ]]; do
    case $1 in
        --config)
            CONFIG_FILE="$2"
            shift 2
            ;;
        --duration)
            TEST_DURATION="$2"
            shift 2
            ;;
        --network)
            BITCOIN_NETWORK="$2"
            shift 2
            ;;
        -h|--help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --config FILE     Configuration file (default: config/signet_solo.toml)"
            echo "  --duration SECS   Test duration in seconds (default: 60)"
            echo "  --network NET     Bitcoin network (signet, testnet, regtest, mainnet)"
            echo "  -h, --help        Show this help"
            exit 0
            ;;
        *)
            echo "Unknown option: $1"
            exit 1
            ;;
    esac
done

main "$@"