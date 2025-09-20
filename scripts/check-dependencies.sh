#!/bin/bash
set -euo pipefail

# Dependency checker for sv2d toolkit

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

check_command() {
    local cmd="$1"
    local package="$2"
    local install_hint="$3"
    
    if command -v "$cmd" >/dev/null 2>&1; then
        log "✓ $cmd found"
        return 0
    else
        log "✗ $cmd not found"
        log "  Package: $package"
        log "  Install: $install_hint"
        return 1
    fi
}

check_library() {
    local lib="$1"
    local package="$2"
    local install_hint="$3"
    
    if ldconfig -p 2>/dev/null | grep -q "$lib" || \
       find /usr/lib* /lib* -name "*$lib*" 2>/dev/null | grep -q "$lib"; then
        log "✓ $lib found"
        return 0
    else
        log "✗ $lib not found"
        log "  Package: $package"
        log "  Install: $install_hint"
        return 1
    fi
}

detect_os() {
    if [[ "$OSTYPE" == "linux-gnu"* ]]; then
        if command -v apt-get >/dev/null 2>&1; then
            echo "debian"
        elif command -v yum >/dev/null 2>&1; then
            echo "rhel"
        elif command -v pacman >/dev/null 2>&1; then
            echo "arch"
        else
            echo "linux"
        fi
    elif [[ "$OSTYPE" == "darwin"* ]]; then
        echo "macos"
    elif [[ "$OSTYPE" == "msys" ]] || [[ "$OSTYPE" == "cygwin" ]]; then
        echo "windows"
    else
        echo "unknown"
    fi
}

check_dependencies() {
    local os="$1"
    local missing=0
    
    log "Checking dependencies for $os..."
    
    # Common dependencies
    case "$os" in
        "debian")
            check_command "gcc" "build-essential" "sudo apt-get install build-essential" || ((missing++))
            check_library "libssl" "libssl-dev" "sudo apt-get install libssl-dev" || ((missing++))
            check_library "libsqlite3" "libsqlite3-dev" "sudo apt-get install libsqlite3-dev" || ((missing++))
            ;;
        "rhel")
            check_command "gcc" "gcc" "sudo yum install gcc" || ((missing++))
            check_library "libssl" "openssl-devel" "sudo yum install openssl-devel" || ((missing++))
            check_library "libsqlite3" "sqlite-devel" "sudo yum install sqlite-devel" || ((missing++))
            ;;
        "arch")
            check_command "gcc" "gcc" "sudo pacman -S gcc" || ((missing++))
            check_library "libssl" "openssl" "sudo pacman -S openssl" || ((missing++))
            check_library "libsqlite3" "sqlite" "sudo pacman -S sqlite" || ((missing++))
            ;;
        "macos")
            check_command "gcc" "xcode-tools" "xcode-select --install" || ((missing++))
            # macOS usually has OpenSSL via Homebrew or system
            if ! check_library "libssl" "openssl" "brew install openssl" 2>/dev/null; then
                log "  Note: OpenSSL might be available via system or Homebrew"
            fi
            ;;
        "windows")
            log "Windows dependencies should be handled by the Rust toolchain"
            ;;
        *)
            log "Unknown OS, please ensure you have:"
            log "  - C compiler (gcc/clang)"
            log "  - OpenSSL development libraries"
            log "  - SQLite development libraries"
            ;;
    esac
    
    return $missing
}

check_rust() {
    log "Checking Rust installation..."
    
    if ! command -v rustc >/dev/null 2>&1; then
        log "✗ Rust not found"
        log "  Install from: https://rustup.rs/"
        return 1
    fi
    
    local rust_version=$(rustc --version | cut -d' ' -f2)
    log "✓ Rust found: $rust_version"
    
    # Check minimum version (1.70.0)
    local min_version="1.70.0"
    if printf '%s\n%s\n' "$min_version" "$rust_version" | sort -V | head -n1 | grep -q "^$min_version$"; then
        log "✓ Rust version is sufficient"
        return 0
    else
        log "✗ Rust version $rust_version is too old (minimum: $min_version)"
        log "  Update with: rustup update"
        return 1
    fi
}

check_network() {
    log "Checking network connectivity..."
    
    if command -v curl >/dev/null 2>&1; then
        if curl -s --connect-timeout 5 https://crates.io >/dev/null; then
            log "✓ Network connectivity to crates.io"
            return 0
        else
            log "✗ Cannot reach crates.io"
            return 1
        fi
    elif command -v wget >/dev/null 2>&1; then
        if wget -q --timeout=5 --spider https://crates.io; then
            log "✓ Network connectivity to crates.io"
            return 0
        else
            log "✗ Cannot reach crates.io"
            return 1
        fi
    else
        log "! Cannot test network (no curl/wget)"
        return 0
    fi
}

main() {
    log "SV2D Toolkit Dependency Checker"
    log "================================"
    
    local os=$(detect_os)
    log "Detected OS: $os"
    
    local total_missing=0
    
    check_rust || ((total_missing++))
    check_dependencies "$os" || total_missing=$((total_missing + $?))
    check_network || ((total_missing++))
    
    log ""
    if [[ $total_missing -eq 0 ]]; then
        log "✓ All dependencies satisfied!"
        log "You can proceed with building sv2d toolkit"
        exit 0
    else
        log "✗ $total_missing dependencies missing"
        log "Please install the missing dependencies before building"
        exit 1
    fi
}

main "$@"