#!/bin/bash
set -euo pipefail

# Container deployment script for sv2d toolkit

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
NAMESPACE="${NAMESPACE:-sv2d}"
IMAGE_TAG="${IMAGE_TAG:-latest}"
DEPLOYMENT_TYPE="${DEPLOYMENT_TYPE:-docker-compose}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

usage() {
    echo "Usage: $0 [OPTIONS] COMMAND"
    echo ""
    echo "Commands:"
    echo "  build       Build container images"
    echo "  deploy      Deploy containers"
    echo "  undeploy    Remove deployed containers"
    echo "  status      Check deployment status"
    echo "  logs        Show container logs"
    echo ""
    echo "Options:"
    echo "  -t, --type TYPE         Deployment type (docker-compose)"
    echo "  -n, --namespace NS      Kubernetes namespace (default: sv2d)"
    echo "  -i, --image-tag TAG     Container image tag (default: latest)"
    echo "  -h, --help              Show this help"
    exit 0
}

build_images() {
    log "Building container images..."
    
    cd "$PROJECT_ROOT"
    
    # Build main image
    log "Building main sv2d image..."
    docker build -t "sv2d:$IMAGE_TAG" -f Dockerfile .
    
    # Build Alpine image
    log "Building Alpine sv2d image..."
    docker build -t "sv2d:$IMAGE_TAG-alpine" -f Dockerfile.alpine .
    
    log "✓ Container images built successfully"
}

deploy_docker_compose() {
    log "Deploying with Docker Compose..."
    
    cd "$PROJECT_ROOT"
    
    # Create config directory if it doesn't exist
    mkdir -p config
    
    # Copy example configs if they don't exist
    if [[ ! -f config/sv2d.toml ]]; then
        if [[ -f sv2-core/examples/solo_config.toml ]]; then
            cp sv2-core/examples/solo_config.toml config/sv2d.toml
            log "Copied example config to config/sv2d.toml"
        fi
    fi
    
    # Deploy services
    docker-compose up -d
    
    log "✓ Docker Compose deployment complete"
    log "Services:"
    log "  - SV2D Daemon: http://localhost:4254 (Stratum V2)"
    log "  - Web Dashboard: http://localhost:8080"
    log "  - Metrics: http://localhost:9090"
}



undeploy_docker_compose() {
    log "Removing Docker Compose deployment..."
    
    cd "$PROJECT_ROOT"
    docker-compose down --volumes --remove-orphans
    
    log "✓ Docker Compose deployment removed"
}



show_status() {
    log "Docker Compose status:"
    cd "$PROJECT_ROOT"
    docker-compose ps
}

show_logs() {
    log "Docker Compose logs:"
    cd "$PROJECT_ROOT"
    docker-compose logs -f
}

# Parse command line arguments
COMMAND=""
while [[ $# -gt 0 ]]; do
    case $1 in
        -t|--type)
            DEPLOYMENT_TYPE="$2"
            shift 2
            ;;
        -n|--namespace)
            NAMESPACE="$2"
            shift 2
            ;;
        -i|--image-tag)
            IMAGE_TAG="$2"
            shift 2
            ;;
        -h|--help)
            usage
            ;;
        build|deploy|undeploy|status|logs)
            COMMAND="$1"
            shift
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Validate deployment type
case "$DEPLOYMENT_TYPE" in
    "docker-compose")
        ;;
    *)
        echo "Error: Invalid deployment type '$DEPLOYMENT_TYPE'"
        echo "Valid types: docker-compose"
        exit 1
        ;;
esac

# Execute command
case "$COMMAND" in
    "build")
        build_images
        ;;
    "deploy")
        deploy_docker_compose
        ;;
    "undeploy")
        undeploy_docker_compose
        ;;
    "status")
        show_status
        ;;
    "logs")
        show_logs
        ;;
    "")
        echo "Error: Command required"
        usage
        ;;
    *)
        echo "Error: Unknown command '$COMMAND'"
        usage
        ;;
esac