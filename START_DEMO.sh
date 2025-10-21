#!/bin/bash
# Working Stratum V2 Mining Demo
# Bitcoin Core v30.0 + sv2-tp v1.0.2 + SRI Pool + SRI Translator

set -e

echo "🚀 Starting Stratum V2 Mining Stack..."

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# Configuration
BITCOIN_BIN=~/Downloads/bitcoin-30.0/bin/bitcoin
BITCOIN_CLI=~/Downloads/bitcoin-30.0/bin/bitcoin-cli
BITCOIN_DATADIR=/tmp/bitcoin_regtest
SV2_TP=./sv2-tp-1.0.2/bin/sv2-tp
POOL_BIN=./stratum-reference/roles/target/debug/pool_sv2
TRANSLATOR_BIN=./stratum-reference/roles/target/debug/translator_sv2
POOL_CONFIG=./config/sri_pool_regtest.WORKING.toml
TRANSLATOR_CONFIG=./config/translator_config.WORKING.toml

# Clean slate
echo "🧹 Cleaning up old processes..."
pkill -f bitcoin-node || true
pkill -f sv2-tp || true
pkill -f pool_sv2 || true
pkill -f translator_sv2 || true
sleep 2

# Clean Bitcoin datadir
rm -rf $BITCOIN_DATADIR
mkdir -p $BITCOIN_DATADIR

echo -e "${YELLOW}Step 1/4: Starting Bitcoin Core v30.0...${NC}"
$BITCOIN_BIN -m node -chain=regtest -ipcbind=unix \
  -datadir=$BITCOIN_DATADIR \
  -rpcuser=test -rpcpassword=test \
  -rpcport=18443 -daemon

echo "Waiting for Bitcoin Core IPC socket..."
for i in {1..30}; do
  if [ -S "$BITCOIN_DATADIR/regtest/node.sock" ]; then
    echo -e "${GREEN}✅ Bitcoin Core IPC ready${NC}"
    break
  fi
  sleep 1
done

echo "Waiting for Bitcoin Core RPC..."
for i in {1..30}; do
  if $BITCOIN_CLI -datadir=$BITCOIN_DATADIR -rpcuser=test -rpcpassword=test -rpcport=18443 getblockchaininfo &>/dev/null; then
    echo -e "${GREEN}✅ Bitcoin Core RPC ready${NC}"
    break
  fi
  sleep 1
done

# Generate initial block to exit IBD
echo "Generating initial block to exit IBD..."
$BITCOIN_CLI -datadir=$BITCOIN_DATADIR -rpcuser=test -rpcpassword=test -rpcport=18443 \
  generatetoaddress 1 bcrt1qe8le5cgtujqrx9r85e8q4r6zjy4c227zhgtyea > /dev/null

echo -e "${YELLOW}Step 2/4: Starting sv2-tp v1.0.2...${NC}"
$SV2_TP -chain=regtest -datadir=$BITCOIN_DATADIR -sv2port=18447 -debug=sv2 > /tmp/sv2-tp.log 2>&1 &
SV2_TP_PID=$!

echo "Waiting for sv2-tp to start..."
for i in {1..30}; do
  if lsof -Pi :18447 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${GREEN}✅ sv2-tp ready on port 18447${NC}"
    break
  fi
  sleep 1
done

echo -e "${YELLOW}Step 3/4: Starting SRI Pool...${NC}"
$POOL_BIN --config $POOL_CONFIG > /tmp/pool.log 2>&1 &
POOL_PID=$!

echo "Waiting for Pool to start..."
for i in {1..20}; do
  if lsof -Pi :34254 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${GREEN}✅ Pool ready on port 34254${NC}"
    break
  fi
  sleep 1
done

echo -e "${YELLOW}Step 4/4: Starting SRI Translator...${NC}"
$TRANSLATOR_BIN --config $TRANSLATOR_CONFIG > /tmp/translator.log 2>&1 &
TRANSLATOR_PID=$!

echo "Waiting for Translator to start..."
for i in {1..20}; do
  if lsof -Pi :3333 -sTCP:LISTEN -t >/dev/null 2>&1; then
    echo -e "${GREEN}✅ Translator ready on port 3333${NC}"
    break
  fi
  sleep 1
done

echo ""
echo -e "${GREEN}🎉 Stratum V2 Mining Stack Running!${NC}"
echo ""
echo "📊 Components:"
echo "  • Bitcoin Core v30.0 - regtest"
echo "  • sv2-tp v1.0.2 - port 18447"
echo "  • SRI Pool - port 34254"
echo "  • SRI Translator - port 3333"
echo ""
echo "⛏️  Point your miner to: YOUR_IP:3333"
echo ""
echo "📝 Logs:"
echo "  • sv2-tp:     tail -f /tmp/sv2-tp.log"
echo "  • Pool:       tail -f /tmp/pool.log"
echo "  • Translator: tail -f /tmp/translator.log"
echo ""
echo "🛑 To stop: pkill -f bitcoin-node; pkill -f sv2-tp; pkill -f pool_sv2; pkill -f translator_sv2"
