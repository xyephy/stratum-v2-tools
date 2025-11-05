# Configuration Examples

Welcome to sv2d configuration examples! These templates will get you mining in minutes.

## 📁 Available Configurations

### For Beginners

1. **`bitaxe-signet.toml`** ⭐ **START HERE**
   - **What**: Solo mining on Bitcoin's signet test network
   - **Why**: Real Bitcoin behavior, but with test coins (worthless)
   - **Time to setup**: 10 minutes + sync time
   - **When to use**: Learning, testing, development
   - **Difficulty**: Easy

2. **`bitaxe-regtest.toml`**
   - **What**: Solo mining on local regtest network
   - **Why**: Instant blocks, complete control
   - **Time to setup**: 5 minutes
   - **When to use**: Development, debugging
   - **Difficulty**: Easiest (but requires manual block generation)

### Coming Soon
- `multi-miner-signet.toml` - Multiple miners on signet
- `pool-operator.toml` - Run a pool for others
- `mainnet-solo.toml` - Real Bitcoin mining (advanced)

## 🚀 Quick Start

### Option A: Signet (Recommended)

```bash
# 1. Start Bitcoin Core on signet
bitcoin -m node -signet -ipcbind=unix -rpcuser=sv2user -rpcpassword=sv2pass123

# 2. Wait for sync (check with: bitcoin-cli -signet getblockchaininfo)

# 3. Generate mining address
bitcoin-cli -signet createwallet "mining"
bitcoin-cli -signet getnewaddress

# 4. Edit bitaxe-signet.toml and update coinbase_address with your address

# 5. Start sv2d
sv2-cli start --config examples/configs/bitaxe-signet.toml

# 6. Configure your Bitaxe to connect to YOUR_IP:3333

# 7. Watch it mine!
sv2-cli status --follow
```

### Option B: Regtest (Quick Testing)

```bash
# 1. Start Bitcoin Core on regtest
bitcoind -regtest -rpcuser=sv2user -rpcpassword=sv2pass123

# 2. Generate some blocks to lower difficulty
bitcoin-cli -regtest generatetoaddress 200 bcrt1qe8le5cgtujqrx9r85e8q4r6zjy4c227zhgtyea

# 3. Start sv2d
sv2-cli start --config examples/configs/bitaxe-regtest.toml

# 4. Configure Bitaxe to connect to YOUR_IP:3333

# 5. Watch blocks being found instantly!
sv2-cli status --follow
```

## 📝 Customizing Configurations

All example configs are **heavily commented** - open them in your favorite editor to understand what each setting does.

### Key Settings to Change

1. **Mining Address** (`coinbase_address`)
   - **CRITICAL**: This is where your rewards go!
   - Generate with: `bitcoin-cli -<network> getnewaddress`
   - Don't use the example addresses - they're not yours!

2. **Bitcoin RPC Credentials** (`rpc_user`, `rpc_password`)
   - Must match your Bitcoin Core configuration
   - Default examples use `sv2user` / `sv2pass123`

3. **Network Port** (`bind_address`)
   - Default: `0.0.0.0:3333` (standard Stratum port)
   - Change to `127.0.0.1:3333` for local-only access
   - Your miners connect to this port

4. **Network Type** (`network`)
   - Must match your Bitcoin Core network
   - Options: `Regtest`, `Signet`, `Testnet`, `Mainnet`

## 🔍 What's In Each Config?

Every config file includes:

- ✅ **Inline documentation** - explains every setting
- ✅ **Expected output** - know when things are working
- ✅ **Troubleshooting tips** - fix common issues
- ✅ **Quick start guide** - step-by-step instructions
- ✅ **Safe defaults** - works out of the box

## 🎯 Which Config Should I Use?

```
┌─────────────────────────────────────────────────────┐
│  Are you new to mining?                             │
│  ├─ Yes → bitaxe-signet.toml (safest)              │
│  └─ No  → Keep reading...                          │
└─────────────────────────────────────────────────────┘
                          │
┌─────────────────────────────────────────────────────┐
│  Do you need instant results for testing?           │
│  ├─ Yes → bitaxe-regtest.toml (fastest)            │
│  └─ No  → bitaxe-signet.toml (realistic)           │
└─────────────────────────────────────────────────────┘
                          │
┌─────────────────────────────────────────────────────┐
│  Ready for real Bitcoin?                            │
│  ├─ Yes → Wait for mainnet-solo.toml (coming soon) │
│  └─ No  → Stick with signet for now               │
└─────────────────────────────────────────────────────┘
```

## 📚 Learning Path

### Week 1: Get Comfortable
- Use `bitaxe-regtest.toml`
- Understand all the settings
- Find a few blocks instantly
- Experiment with changes

### Week 2: Realistic Testing
- Switch to `bitaxe-signet.toml`
- Experience real block times (~10 min)
- See how fees work
- Monitor metrics

### Week 3: Scale Up
- Try `multi-miner-signet.toml` (coming soon)
- Connect multiple miners
- Monitor performance
- Optimize settings

### Week 4+: Advanced
- Experiment with custom transaction selection
- Try pool operation
- Consider mainnet (when ready)

## 🆘 Common Issues

### "Connection refused" to Bitcoin Core
```bash
# Check Bitcoin Core is running:
ps aux | grep bitcoin

# Check it's on the right network:
bitcoin-cli -<network> getblockchaininfo

# For sv2-tp, ensure it was started with -ipcbind=unix:
bitcoin -m node -<network> -ipcbind=unix
```

### "Waiting for IBD"
```bash
# IBD = Initial Block Download (blockchain sync)
# Check sync progress:
bitcoin-cli -<network> getblockchaininfo

# Look for blocks == headers (fully synced)
# On signet: ~30 minutes
# On mainnet: days/weeks
```

### Bitaxe won't connect
```bash
# 1. Check your computer's IP address:
ip addr show  # Linux
ifconfig      # macOS

# 2. Verify sv2d is listening:
sv2-cli status

# 3. Configure Bitaxe:
#    - Pool URL: http://YOUR_ACTUAL_IP:3333  (not localhost!)
#    - Worker: anything
#    - Password: x

# 4. Check firewall isn't blocking port 3333
```

### No blocks being found (signet/mainnet)
```
This is normal! Block finding depends on:
- Your hashrate vs network hashrate
- Current difficulty
- Pure luck

On signet with a single Bitaxe:
- Expect blocks every few days/weeks
- You're competing against the whole signet network
- This is realistic Bitcoin mining experience

On regtest:
- Blocks should be found within seconds
- If not, lower difficulty: bitcoin-cli -regtest generatetoaddress 200 <address>
```

## 🔗 Resources

- **Stratum V2 Spec**: https://stratumprotocol.org/specification
- **Bitcoin Core**: https://bitcoincore.org
- **sv2-tp Repository**: https://github.com/Sjors/sv2-tp
- **SRI Reference**: https://github.com/stratum-mining/stratum
- **Sjors' Workshop**: https://github.com/Sjors/sv2-workshop

## 💡 Tips & Best Practices

1. **Start with signet** - It's the perfect middle ground between regtest and mainnet
2. **Read the comments** - Every config file is a mini-tutorial
3. **Monitor metrics** - Access at http://YOUR_IP:9090/metrics
4. **Keep logs at "info"** - Only use "debug" when troubleshooting
5. **Backup your wallet** - Those test coins might be worthless, but good practice!
6. **Join the community** - Ask questions, share findings
7. **Be patient with IBD** - Initial sync takes time, but only happens once

## 🎓 Understanding The Components

When you start sv2d, it orchestrates several components:

```
┌──────────────────┐
│  Bitcoin Core    │  Your node (bitcoind)
│  (you start)     │
└────────┬─────────┘
         │ IPC
┌────────┴─────────┐
│  sv2-tp          │  Template Provider (sv2d spawns)
│  (auto-started)  │  Gets block templates from Bitcoin Core
└────────┬─────────┘
         │ Stratum V2
┌────────┴─────────┐
│  SRI Pool        │  Pool role (sv2d spawns)
│  (auto-started)  │  Manages mining work distribution
└────────┬─────────┘
         │ Stratum V2
┌────────┴─────────┐
│  SRI Translator  │  Protocol translator (sv2d spawns)
│  (auto-started)  │  Converts Stratum V1 ↔ V2
└────────┬─────────┘
         │ Stratum V1
    ┌────┴────┐
    │ Bitaxe  │  Your miner (you configure)
    └─────────┘
```

**You manage**:
- Bitcoin Core (start it manually)
- Bitaxe (configure to connect)
- sv2d (via sv2-cli commands)

**sv2d manages**:
- sv2-tp spawning and monitoring
- SRI pool spawning and monitoring
- SRI translator spawning and monitoring
- Health checks and auto-restart
- Metrics collection

This is **better than the workshop** because sv2d handles all the process orchestration automatically!

---

**Questions?** Check the main README.md or open an issue!

**Found a bug in these configs?** Please report it!

**Have a use case we're missing?** Request a new template!
