# Docker Deployment Guide

This guide covers deploying the SV2D Stratum V2 Toolkit using Docker containers.

## Quick Start

### Prerequisites

- Docker Engine 20.10+
- Docker Compose 2.0+
- 2GB RAM minimum
- 10GB disk space

### Build and Deploy

1. **Build the container images:**
   ```bash
   ./scripts/deploy-containers.sh build
   ```

2. **Deploy with Docker Compose:**
   ```bash
   ./scripts/deploy-containers.sh deploy
   ```

3. **Check status:**
   ```bash
   ./scripts/deploy-containers.sh status
   ```

4. **View logs:**
   ```bash
   ./scripts/deploy-containers.sh logs
   ```

## Services

The Docker Compose setup includes:

- **sv2d**: Main Stratum V2 daemon
  - Port 4254: Stratum V2 protocol
  - Port 9090: Prometheus metrics
  - Port 8080: Health checks

- **sv2d-web**: Web dashboard
  - Port 8080: Web interface

## Configuration

### Default Configuration

The containers use configuration files from the `config/` directory. On first run, example configurations are copied if none exist.

### Custom Configuration

1. Create your configuration files in the `config/` directory:
   ```bash
   mkdir -p config
   cp sv2-core/examples/solo_config.toml config/sv2d.toml
   ```

2. Edit the configuration as needed:
   ```bash
   nano config/sv2d.toml
   ```

3. Restart the services:
   ```bash
   docker-compose restart
   ```

## Security Features

The container deployment includes several security hardening measures:

- **Non-root user**: Containers run as user ID 1000
- **Read-only filesystem**: Root filesystem is mounted read-only
- **No new privileges**: Prevents privilege escalation
- **Minimal attack surface**: Uses distroless base images
- **Resource limits**: CPU and memory limits configured

## Monitoring

### Health Checks

All services include health checks:
- **Interval**: 30 seconds
- **Timeout**: 10 seconds
- **Retries**: 3 attempts

### Metrics

Prometheus metrics are available at:
- http://localhost:9090/metrics

### Web Dashboard

Access the web dashboard at:
- http://localhost:8080

## Troubleshooting

### Check Service Status
```bash
docker-compose ps
```

### View Service Logs
```bash
docker-compose logs sv2d
docker-compose logs sv2d-web
```

### Restart Services
```bash
docker-compose restart
```

### Clean Restart
```bash
docker-compose down
docker-compose up -d
```

### Check Resource Usage
```bash
docker stats
```

## Data Persistence

The following data is persisted in Docker volumes:
- **sv2d_data**: Database and application data
- **sv2d_logs**: Log files

To backup data:
```bash
docker run --rm -v sv2d_data:/data -v $(pwd):/backup alpine tar czf /backup/sv2d-backup.tar.gz /data
```

To restore data:
```bash
docker run --rm -v sv2d_data:/data -v $(pwd):/backup alpine tar xzf /backup/sv2d-backup.tar.gz -C /
```

## Advanced Usage

### Custom Docker Images

Build with specific tags:
```bash
docker build -t sv2d:custom .
docker build -t sv2d:custom-alpine -f Dockerfile.alpine .
```

### Production Deployment

For production use:

1. Use specific image tags (not `latest`)
2. Configure proper resource limits
3. Set up log rotation
4. Use external databases for persistence
5. Configure TLS termination at load balancer
6. Set up monitoring and alerting

### Environment Variables

Key environment variables:
- `RUST_LOG`: Log level (debug, info, warn, error)
- `SV2D_CONFIG_DIR`: Configuration directory
- `SV2D_DATA_DIR`: Data directory
- `SV2D_LOG_DIR`: Log directory

## Cleanup

To remove all containers and volumes:
```bash
./scripts/deploy-containers.sh undeploy
docker system prune -f
```

## Support

For issues with container deployment:
1. Check the logs: `docker-compose logs`
2. Verify configuration files
3. Ensure ports are not in use
4. Check Docker daemon status