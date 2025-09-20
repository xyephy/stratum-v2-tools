# Stratum V2 Tools

A comprehensive suite of tools for Stratum V2 mining protocol implementation.

## Components

- **sv2-core** - Core Stratum V2 protocol implementation
- **sv2d** - Stratum V2 daemon
- **sv2-cli** - Command-line interface for managing Stratum V2 operations
- **sv2-web** - Web interface for monitoring and management
- **sv2-test** - Testing framework and hardware compatibility tests

## Getting Started

### Prerequisites
- Rust 1.70 or later
- PostgreSQL or SQLite (optional)

### Building
```bash
cargo build --release
```

### Running Tests
```bash
cargo test
```

## Configuration

Configuration files are available in the `config/` directory. See individual component READMEs for specific configuration options.

## Docker Support

Docker containers are available for easy deployment:
```bash
docker-compose up
```

## License

This project is open source.