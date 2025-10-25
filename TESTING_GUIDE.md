# Stratum V2 Mining Stack - Hands-On Testing Guide

## Overview

This guide will walk you through testing a complete Stratum V2 mining stack using Bitcoin Core v30.0, sv2-tp (Template Provider), and the Stratum Reference Implementation (SRI).

**What you'll test:**
- Bitcoin Core v30.0 in regtest mode
- sv2-tp v1.0.3 (Stratum V2 Template Provider) - **UPDATED**
- SRI Pool (Stratum V2 pool server)
- SRI Translator (SV1 → SV2 protocol bridge)
- Real mining hardware (Bitaxe, Apollo, or any Stratum V1 miner)

**Time required:** 15-30 minutes

---

## Prerequisites

### Hardware Requirements
- Computer running macOS, Linux, or Windows
- ASIC miner with Stratum V1 support (Bitaxe, Apollo, Antminer, etc.)
- Both devices on the same local network

### Software Requirements
- Bitcoin Core v30.0
- sv2-tp v1.0.3 (⚠️ NOT v1.0.2 - breaking changes!)
- Stratum Reference Implementation (SRI)

### Downloads

1. **Bitcoin Core v30.0**
   ```bash
   # macOS ARM64 (M1/M2/M3)
   wget https://bitcoincore.org/bin/bitcoin-core-30.0/bitcoin-30.0-arm64-apple-darwin.tar.gz
   tar -xzf bitcoin-30.0-arm64-apple-darwin.tar.gz -C ~/Downloads/

   # macOS x86_64 (Intel)
   wget https://bitcoincore.org/bin/bitcoin-core-30.0/bitcoin-30.0-x86_64-apple-darwin.tar.gz
   tar -xzf bitcoin-30.0-x86_64-apple-darwin.tar.gz -C ~/Downloads/

   # Linux x86_64
   wget https://bitcoincore.org/bin/bitcoin-core-30.0/bitcoin-30.0-x86_64-linux-gnu.tar.gz
   tar -xzf bitcoin-30.0-x86_64-linux-gnu.tar.gz -C ~/Downloads/
   ```

2. **sv2-tp v1.0.3** ⚠️ **BREAKING CHANGES - SEE NOTES BELOW**
   ```bash
   # macOS ARM64
   wget https://github.com/Sjors/sv2-tp/releases/download/v1.0.3/sv2-tp-1.0.3-aarch64-apple-darwin.tar.gz
   tar -xzf sv2-tp-1.0.3-aarch64-apple-darwin.tar.gz

   # macOS x86_64
   wget https://github.com/Sjors/sv2-tp/releases/download/v1.0.3/sv2-tp-1.0.3-x86_64-apple-darwin.tar.gz
   tar -xzf sv2-tp-1.0.3-x86_64-apple-darwin.tar.gz

   # Linux x86_64
   wget https://github.com/Sjors/sv2-tp/releases/download/v1.0.3/sv2-tp-1.0.3-x86_64-unknown-linux-gnu.tar.gz
   tar -xzf sv2-tp-1.0.3-x86_64-unknown-linux-gnu.tar.gz
   ```

   **⚠️ CRITICAL v1.0.3 CHANGES:**
   - Configuration file changed: now checks for `sv2-tp.conf` instead of `bitcoin.conf`
   - Configuration method changed: `-chain=regtest` flag **NO LONGER WORKS**
   - Must now use `bitcoind_url = "unix://..."` in config file (see configuration section below)

3. **Stratum Reference Implementation**
   ```bash
   git clone https://github.com/stratum-mining/stratum.git stratum-reference
   cd stratum-reference/roles
   cargo build --release
   cd ../..
   ```

---

## Quick Start with START_DEMO.sh

The easiest way to get started is using the automated startup script.

### Step 1: Download the Repository

```bash
git clone https://github.com/YOUR_USERNAME/stratum-v2-tools.git
cd stratum-v2-tools
```

### Step 2: Verify Prerequisites

Make sure all binaries are downloaded and in the correct locations:

```bash
# Check Bitcoin Core
~/Downloads/bitcoin-30.0/bin/bitcoind --version

# Check sv2-tp
./sv2-tp-1.0.3/bin/sv2-tp --help

# Check SRI components
./stratum-reference/roles/target/release/pool_sv2 --help
./stratum-reference/roles/target/release/translator_sv2 --help
```

### Step 3: Start the Mining Stack

```bash
chmod +x START_DEMO.sh
./START_DEMO.sh
```

You should see output like:

```
Starting Stratum V2 Mining Stack...
Cleaning up old processes...
Step 1/4: Starting Bitcoin Core v30.0...
[OK] Bitcoin Core IPC ready
[OK] Bitcoin Core RPC ready
Step 2/4: Starting sv2-tp v1.0.2...
[OK] sv2-tp ready on port 18447
Step 3/4: Starting SRI Pool...
[OK] Pool ready on port 34254
Step 4/4: Starting SRI Translator...
[OK] Translator ready on port 3333

Stratum V2 Mining Stack Running!

Components:
  • Bitcoin Core v30.0 - regtest
  • sv2-tp v1.0.3 - port 18447
  • SRI Pool - port 34254
  • SRI Translator - port 3333

Point your miner to: YOUR_IP:3333

Note: If you're NOT using START_DEMO.sh, see manual setup section for v1.0.3 config requirements
```

### Step 4: Find Your Server IP Address

```bash
# macOS
ipconfig getifaddr en0

# Linux
ip addr show | grep "inet " | grep -v 127.0.0.1
```

Note this IP address - you'll need it for your miner configuration.

---

## Manual Setup (Alternative to START_DEMO.sh)

If you prefer to start components manually or need to understand the v1.0.3 configuration:

### sv2-tp v1.0.3 Configuration File

**IMPORTANT:** v1.0.3 changed how Bitcoin Core connection works!

Create `sv2-tp-1.0.3/sv2-tp-config.toml`:

```bash
cd sv2-tp-1.0.3

# Create config file
cat > sv2-tp-config.toml <<EOF
# Bitcoin Core IPC connection (v1.0.3 requirement)
bitcoind_url = "unix://$HOME/.bitcoin/regtest/node.sock"

# Template Provider settings
tp_address = "127.0.0.1:18447"
core_rpc_url = "http://127.0.0.1:18443"
core_rpc_user = "sv2user"
core_rpc_pass = "sv2pass123"

# Coinbase output (where mining rewards go)
[[coinbase_outputs]]
output_script_type = "P2WPKH"
output_script_value = "bcrt1qe8le5cgtujqrx9r85e8q4r6zjy4c227zhgtyea"
EOF
```

**Key Differences from v1.0.2:**
- ❌ Old: `./bin/sv2-tp -chain=regtest` (command-line flag)
- ✅ New: `./bin/sv2-tp --config sv2-tp-config.toml` (config file with `bitcoind_url`)

### Starting Components Manually

```bash
# 1. Start Bitcoin Core
~/Downloads/bitcoin-30.0/bin/bitcoind \
  -regtest \
  -m node \
  -ipcbind=unix \
  -rpcuser=sv2user \
  -rpcpassword=sv2pass123 \
  -rpcport=18443 \
  -daemon

# Wait for Bitcoin to fully start
sleep 5

# 2. Start sv2-tp with config file
cd sv2-tp-1.0.3
./bin/sv2-tp --config sv2-tp-config.toml > /tmp/sv2-tp.log 2>&1 &
cd ..

# Wait for sv2-tp to extract authority key
sleep 3

# 3. Start SRI Pool (with config pointing to sv2-tp)
./stratum-reference/roles/target/release/pool_sv2 \
  -c config/sri_pool_regtest.WORKING.toml > /tmp/pool.log 2>&1 &

# 4. Start SRI Translator
./stratum-reference/roles/target/release/translator_sv2 \
  -c config/translator_config.WORKING.toml > /tmp/translator.log 2>&1 &

# Verify all components running
ps aux | grep -E "(bitcoind|sv2-tp|pool_sv2|translator_sv2)" | grep -v grep
```

**Why the config file matters:**
- v1.0.3 requires IPC socket path for multiprocess Bitcoin Core
- The socket location varies by network (regtest/signet/mainnet)
- Using `bitcoind_url` allows sv2-tp to find the correct socket

---

## Configuring Your Miner

### Bitaxe Configuration

1. Open your Bitaxe web interface (usually `http://10.0.0.11`)
2. Navigate to **Settings** → **Pool Configuration**
3. Configure as follows:
   - **Pool URL**: `YOUR_IP` (the IP from Step 4)
   - **Port**: `3333`
   - **Worker Name**: `bitaxe_test` (or any name you prefer)
   - **Password**: Can be anything (e.g., `x`)
4. Click **Save Settings**
5. Restart your Bitaxe

### Apollo BTC Configuration

1. Open Apollo web interface
2. Go to **Mining** → **Pool Settings**
3. Configure:
   - **URL**: `stratum+tcp://YOUR_IP:3333`
   - **Worker**: `apollo_test`
   - **Password**: `x`
4. Save and restart

### Generic Stratum V1 Miner

Any Stratum V1 miner can connect using:
- **Host**: `YOUR_IP:3333`
- **Protocol**: Stratum V1
- **Worker**: Any name
- **Password**: Any value

---

## Verification and Monitoring

### Check All Components Are Running

```bash
# Check processes
ps aux | grep -E "(bitcoin-node|sv2-tp|pool_sv2|translator_sv2)" | grep -v grep

# Check ports
lsof -i :18443  # Bitcoin RPC
lsof -i :18447  # sv2-tp
lsof -i :34254  # Pool
lsof -i :3333   # Translator (miner connection)
```

### Monitor Logs

Open separate terminal windows to watch logs in real-time:

```bash
# Terminal 1: sv2-tp logs
tail -f /tmp/sv2-tp.log

# Terminal 2: Pool logs
tail -f /tmp/pool.log

# Terminal 3: Translator logs
tail -f /tmp/translator.log
```

### Check Miner Connection

```bash
# See active connections on port 3333
lsof -i :3333 | grep ESTABLISHED
```

You should see output like:
```
translato 98708 user   11u  IPv4  TCP 10.0.0.16:3333->10.0.0.11:57387 (ESTABLISHED)
```

This confirms your miner (10.0.0.11) is connected to the translator (10.0.0.16).

### Check Mining Activity

```bash
# Check blockchain height
~/Downloads/bitcoin-30.0/bin/bitcoin-cli \
  -datadir=/tmp/bitcoin_regtest \
  -rpcuser=test \
  -rpcpassword=test \
  -rpcport=18443 \
  getblockcount

# Check latest block info
~/Downloads/bitcoin-30.0/bin/bitcoin-cli \
  -datadir=/tmp/bitcoin_regtest \
  -rpcuser=test \
  -rpcpassword=test \
  -rpcport=18443 \
  getblockchaininfo
```

If mining is working, you should see the block count increasing!

---

## What to Expect

### On Regtest (Low Difficulty)

**Bitaxe (700 GH/s):**
- Finds blocks almost instantly
- You'll see 10-50+ blocks per minute
- Block count increases rapidly

**Apollo BTC (3-4 TH/s):**
- Even faster block finding
- Hundreds of blocks per minute possible

### Log Messages to Look For

**Translator log (successful connection):**
```
New SV1 downstream connection from 10.0.0.11:57387
Downstream 0 registered successfully
Opening extended mining channel for downstream 0
```

**sv2-tp log (blocks found):**
```
Received 0x76 SubmitSolution from client id=0
Wrote block [hash].dat (submitted=1)
SubmitSolution accepted
```

**Pool log (share submission):**
```
Received SubmitSharesExtended from channel 0
Share accepted
```

---

## Troubleshooting

### Miner Shows "Connection Failed"

1. **Check firewall:**
   ```bash
   # macOS - Allow incoming on port 3333
   sudo /usr/libexec/ApplicationFirewall/socketfilterfw --add /path/to/translator_sv2

   # Linux - Open port 3333
   sudo ufw allow 3333/tcp
   ```

2. **Verify translator is listening:**
   ```bash
   lsof -i :3333 -sTCP:LISTEN
   ```

3. **Check you're using the correct IP:**
   - Don't use `127.0.0.1` or `localhost` from the miner
   - Use the actual network IP of the server

### No Blocks Being Found

1. **Check translator logs for miner connection:**
   ```bash
   tail -f /tmp/translator.log | grep "New SV1"
   ```

2. **Verify mining jobs are being sent:**
   ```bash
   tail -f /tmp/translator.log | grep "NewExtendedMiningJob"
   ```

3. **Check sv2-tp is connected to Bitcoin:**
   ```bash
   tail -f /tmp/sv2-tp.log | grep "Connected to bitcoin-node"
   ```

### Component Won't Start

1. **Check if port is already in use:**
   ```bash
   lsof -i :3333  # Translator
   lsof -i :34254 # Pool
   lsof -i :18447 # sv2-tp
   lsof -i :18443 # Bitcoin
   ```

2. **Kill existing processes:**
   ```bash
   pkill -f bitcoin-node
   pkill -f sv2-tp
   pkill -f pool_sv2
   pkill -f translator_sv2
   ```

3. **Check logs for errors:**
   ```bash
   cat /tmp/sv2-tp.log
   cat /tmp/pool.log
   cat /tmp/translator.log
   ```

### sv2-tp v1.0.3 Specific Issues

**Symptom: sv2-tp hangs on startup with no output**

This is the **#1 issue** with v1.0.3!

**Root Cause:** Using old v1.0.2 startup method (`-chain=regtest` flag)

**Solution:**
```bash
# ❌ DON'T DO THIS (v1.0.2 method - will hang!)
./bin/sv2-tp -chain=regtest

# ✅ DO THIS (v1.0.3 method - requires config file)
./bin/sv2-tp --config sv2-tp-config.toml
```

**How to verify config is correct:**
```bash
# Check your config has bitcoind_url
cat sv2-tp-1.0.3/sv2-tp-config.toml | grep bitcoind_url

# Should show something like:
# bitcoind_url = "unix:///Users/yourname/.bitcoin/regtest/node.sock"

# Verify the socket exists
ls -la ~/.bitcoin/regtest/node.sock
```

**Still hanging? Check Bitcoin Core started with `-m node -ipcbind=unix`:**
```bash
ps aux | grep bitcoind | grep -- "-m node"
```

---

### Bitcoin Core Issues

1. **Check IPC socket exists:**
   ```bash
   ls -la ~/.bitcoin/regtest/node.sock
   ```

2. **Verify Bitcoin is out of IBD:**
   ```bash
   ~/Downloads/bitcoin-30.0/bin/bitcoin-cli \
     -datadir=/tmp/bitcoin_regtest \
     -rpcuser=test \
     -rpcpassword=test \
     -rpcport=18443 \
     getblockchaininfo | grep initialblockdownload
   ```

   Should show: `"initialblockdownload": false`

---

## Testing Checklist

Use this checklist to verify everything is working:

- [ ] All 4 components started without errors
- [ ] Bitcoin Core shows `initialblockdownload: false`
- [ ] sv2-tp connected to Bitcoin via IPC
- [ ] Pool connected to sv2-tp (received NewTemplate)
- [ ] Translator connected to Pool
- [ ] Translator listening on port 3333
- [ ] Miner shows "Connected" status
- [ ] Miner connection visible in `lsof -i :3333`
- [ ] Translator logs show "New SV1 downstream connection"
- [ ] Block count is increasing
- [ ] sv2-tp logs show "SubmitSolution accepted"

---

## Stopping the Stack

To cleanly stop all components:

```bash
# Stop all processes
pkill -f bitcoin-node
pkill -f sv2-tp
pkill -f pool_sv2
pkill -f translator_sv2

# Clean up data directory (optional)
rm -rf /tmp/bitcoin_regtest
```

---

## Advanced Testing

### Test with Multiple Miners

You can connect multiple miners simultaneously:

1. Configure each miner with the same pool URL but different worker names
2. Monitor connections: `lsof -i :3333 | grep ESTABLISHED`
3. Each miner will show as a separate downstream connection in logs

### Monitor Performance

```bash
# Watch blocks being found in real-time
watch -n 1 "~/Downloads/bitcoin-30.0/bin/bitcoin-cli \
  -datadir=/tmp/bitcoin_regtest \
  -rpcuser=test \
  -rpcpassword=test \
  -rpcport=18443 \
  getblockcount"
```

### Test Protocol Conversion

The Translator converts Stratum V1 → Stratum V2. You can see this in action:

```bash
# Watch SV1 messages from miner
tail -f /tmp/translator.log | grep "Received mining"

# Watch SV2 messages to pool
tail -f /tmp/translator.log | grep "OpenExtendedMiningChannel\|SubmitShares"
```

---

## Success Criteria

Your test is successful if:

1. [PASS] All components start and remain running
2. [PASS] Miner connects and shows "online" status
3. [PASS] Block count increases over time
4. [PASS] Logs show "SubmitSolution accepted" messages
5. [PASS] No errors or crashes in component logs

---

## Reporting Issues

If you encounter problems during testing:

1. **Collect logs:**
   ```bash
   tar -czf sv2-test-logs.tar.gz /tmp/sv2-tp.log /tmp/pool.log /tmp/translator.log
   ```

2. **Gather system info:**
   ```bash
   # OS version
   uname -a

   # Bitcoin version
   ~/Downloads/bitcoin-30.0/bin/bitcoin --version

   # Component status
   ps aux | grep -E "(bitcoin|sv2-tp|pool_sv2|translator)"
   ```

3. **Submit issue with:**
   - Log archive
   - System info
   - Miner model and configuration
   - Steps to reproduce

---

## Additional Resources

- **Bitcoin Core Documentation**: https://bitcoincore.org/en/doc/
- **sv2-tp Repository**: https://github.com/Sjors/sv2-tp
- **v1.0.3 Release Notes**: https://github.com/Sjors/sv2-tp/releases/tag/v1.0.3
- **Stratum V2 Specification**: https://github.com/stratum-mining/sv2-spec
- **SRI Documentation**: https://github.com/stratum-mining/stratum

---

## FAQ

**Q: What changed between sv2-tp v1.0.2 and v1.0.3?**
A: **CRITICAL BREAKING CHANGE!**
- v1.0.2: Used `-chain=regtest` command-line flag
- v1.0.3: Requires config file with `bitcoind_url = "unix://..."`
- v1.0.3 will **hang forever** if started with old v1.0.2 method
- Config file name changed: now checks for `sv2-tp.conf` instead of `bitcoin.conf`
- See [release notes](https://github.com/Sjors/sv2-tp/releases/tag/v1.0.3) for full details

**Q: Why regtest and not testnet/mainnet?**
A: Regtest has extremely low difficulty, making it perfect for testing. Your miner will find blocks within seconds, confirming the full stack works end-to-end.

**Q: Can I use this setup for real mining?**
A: This demo uses regtest. For real mining, you'd need to:
- Configure for mainnet or testnet
- Point to a real mining pool or solo mine
- Use proper security (authentication, TLS)

**Q: Why does my miner find so many blocks?**
A: Regtest difficulty is minimal by design. A 700 GH/s Bitaxe can find dozens of blocks per minute on regtest.

**Q: Do I need a wallet?**
A: Not for this test. Mining rewards are tracked but you don't need a wallet to verify mining works.

**Q: Can I test without real hardware?**
A: Yes! You can use CPU mining software like `cpuminer` configured for Stratum V1, though it will mine very slowly.

---

## Next Steps

After successful testing:

1. Try pointing multiple miners at once
2. Monitor protocol messages in logs
3. Experiment with different configurations
4. Test pool failover scenarios
5. Explore Stratum V2 features (job declaration, version rolling)

---

**Good luck with your testing!**
