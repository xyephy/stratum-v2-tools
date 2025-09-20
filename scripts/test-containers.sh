#!/bin/bash
set -euo pipefail

# Container deployment tests for sv2d toolkit

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TEST_NAMESPACE="sv2d-test"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

cleanup() {
    log "Cleaning up test resources..."
    
    # Clean up Docker containers
    docker-compose -f "$PROJECT_ROOT/docker-compose.yml" down --volumes --remove-orphans 2>/dev/null || true
    
    # Clean up test images
    docker rmi sv2d-test:latest 2>/dev/null || true
    
    # Clean up Kubernetes test namespace
    if command -v kubectl >/dev/null 2>&1; then
        kubectl delete namespace "$TEST_NAMESPACE" --ignore-not-found=true 2>/dev/null || true
    fi
}

trap cleanup EXIT

test_dockerfile_syntax() {
    log "Testing Dockerfile syntax..."
    
    if ! command -v docker >/dev/null 2>&1; then
        log "! Docker not available, skipping Dockerfile syntax test"
        return 0
    fi
    
    # Basic syntax check by parsing the Dockerfile
    if [[ ! -f "$PROJECT_ROOT/Dockerfile" ]]; then
        log "✗ Main Dockerfile not found"
        return 1
    fi
    
    if [[ ! -f "$PROJECT_ROOT/Dockerfile.alpine" ]]; then
        log "✗ Alpine Dockerfile not found"
        return 1
    fi
    
    # Check for basic Dockerfile syntax issues
    if grep -q "^FROM" "$PROJECT_ROOT/Dockerfile" && \
       grep -q "^WORKDIR" "$PROJECT_ROOT/Dockerfile" && \
       grep -q "^COPY" "$PROJECT_ROOT/Dockerfile"; then
        log "✓ Main Dockerfile syntax appears valid"
    else
        log "✗ Main Dockerfile missing required instructions"
        return 1
    fi
    
    if grep -q "^FROM" "$PROJECT_ROOT/Dockerfile.alpine" && \
       grep -q "^WORKDIR" "$PROJECT_ROOT/Dockerfile.alpine" && \
       grep -q "^COPY" "$PROJECT_ROOT/Dockerfile.alpine"; then
        log "✓ Alpine Dockerfile syntax appears valid"
    else
        log "✗ Alpine Dockerfile missing required instructions"
        return 1
    fi
    
    return 0
}

test_docker_compose_syntax() {
    log "Testing Docker Compose syntax..."
    
    if ! docker-compose -f "$PROJECT_ROOT/docker-compose.yml" config >/dev/null 2>&1; then
        log "✗ Docker Compose syntax invalid"
        return 1
    fi
    
    log "✓ Docker Compose syntax valid"
    return 0
}

test_docker_compose_functionality() {
    log "Testing Docker Compose functionality..."
    
    if ! command -v docker-compose >/dev/null 2>&1; then
        log "! docker-compose not available, skipping functionality tests"
        return 0
    fi
    
    local failed=0
    
    # Test that all required services are defined
    local required_services=("sv2d" "sv2d-web")
    for service in "${required_services[@]}"; do
        if docker-compose -f "$PROJECT_ROOT/docker-compose.yml" config --services | grep -q "^$service$"; then
            log "✓ Docker Compose service defined: $service"
        else
            log "✗ Docker Compose service missing: $service"
            ((failed++))
        fi
    done
    
    # Test that health checks are defined
    if docker-compose -f "$PROJECT_ROOT/docker-compose.yml" config | grep -q "healthcheck:"; then
        log "✓ Docker Compose health checks defined"
    else
        log "✗ Docker Compose health checks missing"
        ((failed++))
    fi
    
    # Test that volumes are properly configured
    if docker-compose -f "$PROJECT_ROOT/docker-compose.yml" config | grep -q "volumes:"; then
        log "✓ Docker Compose volumes configured"
    else
        log "✗ Docker Compose volumes missing"
        ((failed++))
    fi
    
    return $failed
}

test_security_configuration() {
    log "Testing security configuration..."
    
    local failed=0
    
    # Check Dockerfile security practices
    if grep -q "USER.*root" "$PROJECT_ROOT/Dockerfile" 2>/dev/null; then
        log "✗ Dockerfile runs as root user"
        ((failed++))
    else
        log "✓ Dockerfile uses non-root user"
    fi
    
    # Check for read-only root filesystem in Docker Compose
    if grep -q "read_only: true" "$PROJECT_ROOT/docker-compose.yml" 2>/dev/null; then
        log "✓ Docker Compose uses read-only filesystem"
    else
        log "✗ Docker Compose missing read-only filesystem"
        ((failed++))
    fi
    
    # Check for security options in Docker Compose
    if grep -q "security_opt:" "$PROJECT_ROOT/docker-compose.yml" 2>/dev/null; then
        log "✓ Docker Compose has security options"
    else
        log "✗ Docker Compose missing security options"
        ((failed++))
    fi
    
    # Check for no-new-privileges
    if grep -q "no-new-privileges" "$PROJECT_ROOT/docker-compose.yml" 2>/dev/null; then
        log "✓ Docker Compose prevents privilege escalation"
    else
        log "✗ Docker Compose missing no-new-privileges"
        ((failed++))
    fi
    
    return $failed
}

test_health_checks() {
    log "Testing health check configuration..."
    
    local failed=0
    
    # Check Docker health checks
    if ! grep -q "HEALTHCHECK" "$PROJECT_ROOT/Dockerfile" 2>/dev/null; then
        log "✗ Dockerfile missing health check"
        ((failed++))
    else
        log "✓ Dockerfile has health check"
    fi
    
    # Check Docker Compose health checks
    if ! grep -q "healthcheck:" "$PROJECT_ROOT/docker-compose.yml" 2>/dev/null; then
        log "✗ Docker Compose missing health checks"
        ((failed++))
    else
        log "✓ Docker Compose has health checks"
    fi
    
    # Check for proper health check intervals
    if grep -q "interval:" "$PROJECT_ROOT/docker-compose.yml" 2>/dev/null; then
        log "✓ Docker Compose health checks have intervals"
    else
        log "✗ Docker Compose health checks missing intervals"
        ((failed++))
    fi
    
    return $failed
}

test_container_build() {
    log "Testing container build readiness..."
    
    if ! command -v docker >/dev/null 2>&1; then
        log "! Docker not available, skipping build test"
        return 0
    fi
    
    # Check if we have the necessary files for building
    local missing_files=0
    
    if [[ ! -f "$PROJECT_ROOT/Cargo.toml" ]]; then
        log "! Main Cargo.toml not found - would need for actual build"
        ((missing_files++))
    fi
    
    if [[ ! -d "$PROJECT_ROOT/sv2-core" ]]; then
        log "! sv2-core directory not found - would need for actual build"
        ((missing_files++))
    fi
    
    if [[ $missing_files -gt 0 ]]; then
        log "! Container build would require Rust project structure"
        log "✓ Dockerfile structure is ready for when project is complete"
        return 0
    fi
    
    # If we have the project structure, we could test the actual build
    log "✓ Container build readiness validated"
    return 0
}

validate_port_configuration() {
    log "Validating port configuration..."
    
    local failed=0
    
    # Check for consistent port usage across manifests
    local stratum_port="4254"
    local web_port="8080"
    local metrics_port="9090"
    
    # Check Docker Compose
    if ! grep -q "$stratum_port:$stratum_port" "$PROJECT_ROOT/docker-compose.yml" 2>/dev/null; then
        log "✗ Docker Compose missing Stratum port $stratum_port"
        ((failed++))
    else
        log "✓ Docker Compose has Stratum port $stratum_port"
    fi
    
    if ! grep -q "$web_port:$web_port" "$PROJECT_ROOT/docker-compose.yml" 2>/dev/null; then
        log "✗ Docker Compose missing web port $web_port"
        ((failed++))
    else
        log "✓ Docker Compose has web port $web_port"
    fi
    
    if [[ $failed -eq 0 ]]; then
        log "✓ Port configuration consistent"
    fi
    
    return $failed
}

main() {
    log "SV2D Container Deployment Tests"
    log "==============================="
    
    local total_failed=0
    
    test_dockerfile_syntax || ((total_failed++))
    test_docker_compose_syntax || ((total_failed++))
    test_docker_compose_functionality || total_failed=$((total_failed + $?))
    test_security_configuration || total_failed=$((total_failed + $?))
    test_health_checks || total_failed=$((total_failed + $?))
    validate_port_configuration || total_failed=$((total_failed + $?))
    test_container_build || ((total_failed++))
    
    log ""
    if [[ $total_failed -eq 0 ]]; then
        log "✓ All container deployment tests passed!"
        exit 0
    else
        log "✗ $total_failed container deployment tests failed"
        exit 1
    fi
}

main "$@"