# Bitcoin Core Setup for sv2d

## The bitcoin-node Error

If you see this error:
```
Error: execvp failed to execute 'bitcoin-node': No such file or directory
```

This means you're trying to use multiprocess mode (`-m node`) but your Bitcoin Core build doesn't include the separate process binaries.

## Solution: Use Standard Mode

Use `bitcoind` directly instead of the `bitcoin -m node` command.

### For Regtest (Local Testing)

```bash
# Start Bitcoin Core in regtest mode
~/Downloads/bitcoin-30.0rc2/bin/bitcoind \
  -regtest \
  -rpcuser=sv2user \
  -rpcpassword=sv2pass123 \
  -rpcport=18443 \
  -daemon \
  -txindex

# Check status
~/Downloads/bitcoin-30.0rc2/bin/bitcoin-cli \
  -regtest \
  -rpcuser=sv2user \
  -rpcpassword=sv2pass123 \
  -rpcport=18443 \
  getblockchaininfo

# Generate blocks for testing
~/Downloads/bitcoin-30.0rc2/bin/bitcoin-cli \
  -regtest \
  -rpcuser=sv2user \
  -rpcpassword=sv2pass123 \
  -rpcport=18443 \
  generatetoaddress 200 bcrt1qe8le5cgtujqrx9r85e8q4r6zjy4c227zhgtyea
```

### For Signet (Testnet-like)

```bash
# Start Bitcoin Core in signet mode
~/Downloads/bitcoin-30.0rc2/bin/bitcoind \
  -signet \
  -rpcuser=sv2user \
  -rpcpassword=sv2pass123 \
  -rpcport=38332 \
  -datadir=/tmp/bitcoin_signet \
  -daemon

# Check status
~/Downloads/bitcoin-30.0rc2/bin/bitcoin-cli \
  -signet \
  -rpcuser=sv2user \
  -rpcpassword=sv2pass123 \
  -rpcport=38332 \
  getblockchaininfo

# Check sync progress
~/Downloads/bitcoin-30.0rc2/bin/bitcoin-cli \
  -signet \
  -rpcuser=sv2user \
  -rpcpassword=sv2pass123 \
  -rpcport=38332 \
  getmininginfo
```

## Why Not IPC Mode?

The `-ipcbind=unix` flag is useful for certain setups, but it's optional. Standard RPC works fine for sv2d testing.

**Only add `-ipcbind=unix` if:**
- You're using sv2-tp (template provider) which requires IPC
- You need Bitcoin Core's multiprocess architecture
- You have a Bitcoin Core build with multiprocess support

For basic sv2d testing, standard RPC is sufficient.

## Stopping Bitcoin Core

```bash
# Regtest
~/Downloads/bitcoin-30.0rc2/bin/bitcoin-cli -regtest -rpcuser=sv2user -rpcpassword=sv2pass123 stop

# Signet
~/Downloads/bitcoin-30.0rc2/bin/bitcoin-cli -signet -rpcuser=sv2user -rpcpassword=sv2pass123 stop
```

## Quick Reference

| Network | RPC Port | Generate Blocks |
|---------|----------|-----------------|
| Regtest | 18443    | Yes (instant)   |
| Signet  | 38332    | No (network)    |
| Mainnet | 8332     | No (real BTC)   |

## Testing sv2d After Bitcoin Core Starts

```bash
# Build sv2d
cd /Users/munje/dawn/stratum-v2-tools
cargo build --release

# Start sv2d with example config
./target/release/sv2d --config examples/configs/bitaxe-regtest.toml

# Check status
./target/release/sv2-cli status
```
