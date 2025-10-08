#!/bin/bash
# Working Demo Startup - DO NOT MODIFY

# 1. Bitcoin Core
~/Downloads/bitcoin-30.0rc2/bin/bitcoind -chain=regtest \
  -ipcbind=unix -rpcuser=sv2user -rpcpassword=sv2pass123 \
  -rpcport=18443 -daemon

sleep 5

# 2. Template Provider
cd ~/dawn/stratum-v2-tools/sv2-tp-1.0.2
./bin/sv2-tp -chain=regtest -debug=sv2 > /tmp/sv2-tp.log 2>&1 &
echo "sv2-tp PID: $!"

sleep 3

# 3. SRI Pool
cd ~/dawn/stratum-v2-tools/stratum-reference/roles
./target/debug/pool_sv2 -c ../../config/sri_pool_regtest.WORKING.toml > /tmp/pool.log 2>&1 &
echo "Pool PID: $!"

sleep 3

# 4. SRI Translator
./target/debug/translator_sv2 -c ../../config/translator_config.WORKING.toml > /tmp/translator.log 2>&1 &
echo "Translator PID: $!"

sleep 3

# 5. Verify
echo "Testing stack..."
cd ~/dawn/stratum-v2-tools
python3 end_to_end_test.py

echo ""
echo "Stack ready for Bitaxe on localhost:3333"