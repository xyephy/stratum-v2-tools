# Stratum V2 Tools

A plug-and-play Stratum V2 infrastructure tool for sovereign miners. Connect any miner (legacy SV1 or new SV2) to your own pool with minimal setup.

## Quick Start

### Prerequisites
- Rust 1.70 or later
- Bitcoin Core (regtest, signet, or mainnet)
- [sv2-tp](https://github.com/Sjors/sv2-tp) (template provider)

### Building
```bash
cargo build --release
```

### Setup Bitcoin Core

See [BITCOIN_SETUP.md](BITCOIN_SETUP.md) for detailed Bitcoin Core setup instructions.

Quick regtest setup:
```bash
bitcoind -regtest -rpcuser=sv2user -rpcpassword=sv2pass123 -rpcport=18443 -daemon -txindex
```

### Running sv2d

```bash
# Start with example config
./target/release/sv2d --config examples/configs/bitaxe-regtest.toml

# Check status
./target/release/sv2-cli status

# View logs
./target/release/sv2-cli logs

# Stop daemon
./target/release/sv2-cli stop
```

## Documentation

- **[examples/configs/README.md](examples/configs/README.md)** - Comprehensive configuration guide with examples
- **[BITCOIN_SETUP.md](BITCOIN_SETUP.md)** - Bitcoin Core setup for regtest and signet
- **[BUILD_ISSUES.md](BUILD_ISSUES.md)** - Build troubleshooting and resolution
- **[ROADMAP.md](ROADMAP.md)** - Project roadmap and development status

## Components

- **sv2-core** - Core Stratum V2 protocol implementation
- **sv2d** - Stratum V2 daemon (orchestrates all components)
- **sv2-cli** - Command-line interface for daemon management

## Configuration Examples

Workshop-style configs with inline documentation:

- `examples/configs/bitaxe-regtest.toml` - Bitaxe solo mining on regtest (testing)
- `examples/configs/bitaxe-signet.toml` - Bitaxe solo mining on signet (production-like testing)

See [examples/configs/README.md](examples/configs/README.md) for detailed guides.

## Current Status

**Phase 1 Complete** - Build system fixed, comprehensive documentation added, stable operation achieved.

**Next Phase** - Refactoring to single `sv2-node` binary for simplified deployment.

See [ROADMAP.md](ROADMAP.md) for full development timeline.

## Architecture

```
Bitcoin Core → sv2-tp → SRI Pool → SRI Translator → sv2d orchestrates all
                                                    ↓
                                                Your Miner (SV1 or SV2)
```

## Contributing

This is an active development project. See [ROADMAP.md](ROADMAP.md) for current priorities.

## License

This project is open source.