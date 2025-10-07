# Stratum V2 Proxy Demo

A working Stratum V2 proxy implementation using SRI (Stratum Reference Implementation) components for Bitcoin mining hardware like Bitaxe.

## 🚀 Quick Start

```bash
# 1. Clone and build
git clone https://github.com/xyephy/stratum-v2-tools.git
cd stratum-v2-tools
cargo build --release

# 2. Download dependencies
# - Bitcoin Core v30: https://bitcoincore.org/bin/bitcoin-core-30.0/test.rc2/
# - sv2-tp: https://github.com/demand-open-source/sv2-tp/releases
# - SRI components: https://github.com/stratum-mining/stratum/

# 3. Test the stack
python3 end_to_end_test.py
```

## 🏗️ Architecture

```
Bitaxe (SV1) → SRI Translator → SRI Pool → sv2-tp → Bitcoin Core v30
     ↑              ↑             ↑          ↑            ↑
   :3333         Proxy Mode    :34254    :18447       regtest
```

## 📁 Key Files

- **`config/sri_pool_regtest.toml`** - SRI Pool configuration with correct authority keys
- **`config/translator_config.toml`** - SRI Translator configuration for Bitaxe
- **`end_to_end_test.py`** - Test script that verifies mining.subscribe → mining.notify flow
- **`sv2d/src/main.rs`** - Simplified proxy mode wrapper

## ✅ Verified Working

- **Flow**: Bitcoin Core v30 → sv2-tp → SRI Pool → SRI Translator → Mining hardware
- **Test**: 3 consecutive end-to-end tests pass
- **Hardware**: Ready for Bitaxe on `localhost:3333`
- **Protocol**: Stratum V1 downstream, Stratum V2 upstream

## 🔧 Configuration

All configs use standard test authority keys:
- **Authority Public Key**: `9auqWEzQDVyd2oe1JVGFLMLHZtCo2FFqZwtKA5gd9xbuEu7PH72`
- **Authority Secret Key**: `mkDLTBBRxdBv998612qipDYoTK3YUrqLe8uWw7gu3iXbSrn2n`

## 🧪 Testing

```bash
# Verify mining flow works
python3 end_to_end_test.py

# Expected output:
# 1. Subscribe: {"id":1,"error":null,"result":...}
# 2. Authorize: {"id":2,"error":null,"result":true}
# 3. Work notify: {"method":"mining.notify","params":...}
# SUCCESS: Received mining work
```

## 🎯 Production Ready

This is a **working demo** that composes battle-tested SRI components instead of reinventing the protocol. The approach prioritizes **stability** and **compatibility** over custom implementations.

## License

Open source project for Stratum V2 mining development.