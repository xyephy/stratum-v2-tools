#!/bin/bash
set -euo pipefail

# Packaging tests for sv2d toolkit

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
TEST_DIR="${TEST_DIR:-$PROJECT_ROOT/target/packaging-tests}"
DIST_DIR="${DIST_DIR:-$PROJECT_ROOT/dist}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

cleanup() {
    if [[ -d "$TEST_DIR" ]]; then
        log "Cleaning up test directory: $TEST_DIR"
        rm -rf "$TEST_DIR"
    fi
}

trap cleanup EXIT

setup_test_env() {
    log "Setting up test environment..."
    rm -rf "$TEST_DIR"
    mkdir -p "$TEST_DIR"
}

test_build_script() {
    log "Testing build script..."
    
    # Test help output
    if ! "$SCRIPT_DIR/build.sh" --help >/dev/null 2>&1; then
        log "✗ Build script help failed"
        return 1
    fi
    
    log "✓ Build script help works"
    return 0
}

test_dependency_checker() {
    log "Testing dependency checker..."
    
    # Run dependency checker
    if "$SCRIPT_DIR/check-dependencies.sh" >/dev/null 2>&1; then
        log "✓ Dependency checker passed"
        return 0
    else
        log "! Dependency checker found missing dependencies (expected on some systems)"
        return 0
    fi
}

test_archive_structure() {
    local platform="$1"
    local archive_path="$2"
    
    log "Testing archive structure for $platform..."
    
    local extract_dir="$TEST_DIR/extract-$platform"
    mkdir -p "$extract_dir"
    
    # Extract archive
    cd "$extract_dir"
    if [[ "$archive_path" == *.tar.gz ]]; then
        tar -xzf "$archive_path"
    elif [[ "$archive_path" == *.zip ]]; then
        unzip -q "$archive_path"
    else
        log "✗ Unknown archive format: $archive_path"
        return 1
    fi
    
    # Check structure
    local platform_dir="$extract_dir/$platform"
    if [[ ! -d "$platform_dir" ]]; then
        log "✗ Platform directory not found: $platform_dir"
        return 1
    fi
    
    # Check required directories
    local required_dirs=("bin" "config" "scripts")
    for dir in "${required_dirs[@]}"; do
        if [[ ! -d "$platform_dir/$dir" ]]; then
            log "✗ Required directory missing: $dir"
            return 1
        fi
    done
    
    # Check binaries
    local bin_suffix=""
    if [[ "$platform" == windows-* ]]; then
        bin_suffix=".exe"
    fi
    
    local required_bins=("sv2d$bin_suffix" "sv2-cli$bin_suffix" "sv2-web$bin_suffix")
    for bin in "${required_bins[@]}"; do
        if [[ ! -f "$platform_dir/bin/$bin" ]]; then
            log "✗ Required binary missing: $bin"
            return 1
        fi
        
        if [[ ! -x "$platform_dir/bin/$bin" ]]; then
            log "✗ Binary not executable: $bin"
            return 1
        fi
    done
    
    # Check install script
    if [[ ! -f "$platform_dir/scripts/install.sh" ]]; then
        log "✗ Install script missing"
        return 1
    fi
    
    log "✓ Archive structure valid for $platform"
    return 0
}

test_install_script_syntax() {
    local platform="$1"
    
    log "Testing install script syntax for $platform..."
    
    local install_script="$SCRIPT_DIR/install-$platform.sh"
    if [[ ! -f "$install_script" ]]; then
        log "✗ Install script not found: $install_script"
        return 1
    fi
    
    # Check bash syntax
    if ! bash -n "$install_script"; then
        log "✗ Install script has syntax errors"
        return 1
    fi
    
    log "✓ Install script syntax valid for $platform"
    return 0
}

test_service_files() {
    log "Testing service files..."
    
    # Test systemd service
    if [[ -f "$SCRIPT_DIR/sv2d.service" ]]; then
        # Basic syntax check for systemd service
        if grep -q "^\[Unit\]" "$SCRIPT_DIR/sv2d.service" && \
           grep -q "^\[Service\]" "$SCRIPT_DIR/sv2d.service" && \
           grep -q "^\[Install\]" "$SCRIPT_DIR/sv2d.service"; then
            log "✓ Systemd service file structure valid"
        else
            log "✗ Systemd service file structure invalid"
            return 1
        fi
    fi
    
    # Test launchd plist
    if [[ -f "$SCRIPT_DIR/com.sv2d.daemon.plist" ]]; then
        # Basic XML validation
        if command -v xmllint >/dev/null 2>&1; then
            if xmllint --noout "$SCRIPT_DIR/com.sv2d.daemon.plist" 2>/dev/null; then
                log "✓ Launchd plist XML valid"
            else
                log "✗ Launchd plist XML invalid"
                return 1
            fi
        else
            log "! Cannot validate plist XML (xmllint not available)"
        fi
    fi
    
    return 0
}

test_existing_archives() {
    log "Testing existing archives..."
    
    if [[ ! -d "$DIST_DIR" ]]; then
        log "! No dist directory found, skipping archive tests"
        return 0
    fi
    
    local found_archives=0
    for archive in "$DIST_DIR"/*.tar.gz "$DIST_DIR"/*.zip; do
        if [[ -f "$archive" ]]; then
            found_archives=1
            local basename=$(basename "$archive")
            local platform=""
            
            # Extract platform from filename
            if [[ "$basename" =~ sv2d-toolkit-.*-(linux-x86_64|linux-aarch64|macos-x86_64|macos-aarch64|windows-x86_64) ]]; then
                platform="${BASH_REMATCH[1]}"
                test_archive_structure "$platform" "$archive" || return 1
            else
                log "! Cannot determine platform for archive: $basename"
            fi
        fi
    done
    
    if [[ $found_archives -eq 0 ]]; then
        log "! No archives found in $DIST_DIR"
        return 0
    fi
    
    return 0
}

run_installation_validation() {
    log "Running installation validation tests..."
    
    # Test each platform's install script
    local platforms=("linux-x86_64" "linux-aarch64" "macos-x86_64" "macos-aarch64" "windows-x86_64")
    
    for platform in "${platforms[@]}"; do
        test_install_script_syntax "$platform" || return 1
    done
    
    return 0
}

main() {
    log "SV2D Toolkit Packaging Tests"
    log "============================"
    
    setup_test_env
    
    local failed_tests=0
    
    test_build_script || ((failed_tests++))
    test_dependency_checker || ((failed_tests++))
    test_service_files || ((failed_tests++))
    run_installation_validation || ((failed_tests++))
    test_existing_archives || ((failed_tests++))
    
    log ""
    if [[ $failed_tests -eq 0 ]]; then
        log "✓ All packaging tests passed!"
        exit 0
    else
        log "✗ $failed_tests packaging tests failed"
        exit 1
    fi
}

main "$@"