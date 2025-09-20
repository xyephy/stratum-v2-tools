#!/bin/bash
set -euo pipefail

# Build script for sv2d toolkit
# Supports cross-compilation for multiple platforms

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
VERSION="${VERSION:-$(grep '^version' "$PROJECT_ROOT/Cargo.toml" 2>/dev/null | sed 's/.*"\(.*\)".*/\1/' || echo "0.1.0")}"
BUILD_DIR="${BUILD_DIR:-$PROJECT_ROOT/target/release}"
DIST_DIR="${DIST_DIR:-$PROJECT_ROOT/dist}"

usage() {
    echo "Usage: $0 [OPTIONS]"
    echo "Options:"
    echo "  -p, --platform PLATFORM    Target platform (linux-x86_64, linux-aarch64, macos-x86_64, macos-aarch64, windows-x86_64, all)"
    echo "  -v, --version VERSION       Version to build (default: from Cargo.toml)"
    echo "  -o, --output DIR           Output directory (default: dist/)"
    echo "  -h, --help                 Show this help"
    exit 0
}

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

install_target() {
    local target="$1"
    log "Installing Rust target: $target"
    rustup target add "$target" || {
        log "Warning: Failed to install target $target, skipping..."
        return 1
    }
}

build_platform() {
    local platform="$1"
    local target="${TARGETS[$platform]}"
    
    log "Building for platform: $platform (target: $target)"
    
    # Install target if not present
    install_target "$target" || return 1
    
    # Build all binaries
    log "Building sv2d daemon..."
    cargo build --release --target "$target" --bin sv2d
    
    log "Building sv2-cli..."
    cargo build --release --target "$target" --bin sv2-cli
    
    log "Building sv2-web..."
    cargo build --release --target "$target" --bin sv2-web
    
    # Create distribution directory
    local dist_platform_dir="$DIST_DIR/$platform"
    mkdir -p "$dist_platform_dir/bin"
    mkdir -p "$dist_platform_dir/config"
    mkdir -p "$dist_platform_dir/scripts"
    
    # Copy binaries
    local bin_suffix=""
    if [[ "$platform" == windows-* ]]; then
        bin_suffix=".exe"
    fi
    
    cp "$PROJECT_ROOT/target/$target/release/sv2d$bin_suffix" "$dist_platform_dir/bin/"
    cp "$PROJECT_ROOT/target/$target/release/sv2-cli$bin_suffix" "$dist_platform_dir/bin/"
    cp "$PROJECT_ROOT/target/$target/release/sv2-web$bin_suffix" "$dist_platform_dir/bin/"
    
    # Copy configuration files
    cp "$PROJECT_ROOT/sv2-core/examples/"*.toml "$dist_platform_dir/config/" 2>/dev/null || true
    
    # Copy installation scripts
    cp "$SCRIPT_DIR/install-$platform.sh" "$dist_platform_dir/scripts/install.sh" 2>/dev/null || {
        log "Warning: No platform-specific install script found for $platform"
    }
    
    # Copy service files
    if [[ "$platform" == linux-* ]]; then
        mkdir -p "$dist_platform_dir/systemd"
        cp "$SCRIPT_DIR/sv2d.service" "$dist_platform_dir/systemd/" 2>/dev/null || true
    elif [[ "$platform" == macos-* ]]; then
        mkdir -p "$dist_platform_dir/launchd"
        cp "$SCRIPT_DIR/com.sv2d.daemon.plist" "$dist_platform_dir/launchd/" 2>/dev/null || true
    fi
    
    # Create archive
    local archive_name="sv2d-toolkit-$VERSION-$platform"
    log "Creating archive: $archive_name"
    
    cd "$DIST_DIR"
    if [[ "$platform" == windows-* ]]; then
        zip -r "$archive_name.zip" "$platform/"
    else
        tar -czf "$archive_name.tar.gz" "$platform/"
    fi
    cd "$PROJECT_ROOT"
    
    log "Build complete for $platform: $DIST_DIR/$archive_name"
}

# Parse command line arguments
PLATFORM=""
HELP_REQUESTED=false

while [[ $# -gt 0 ]]; do
    case $1 in
        -p|--platform)
            PLATFORM="$2"
            shift 2
            ;;
        -v|--version)
            VERSION="$2"
            shift 2
            ;;
        -o|--output)
            DIST_DIR="$2"
            shift 2
            ;;
        -h|--help)
            HELP_REQUESTED=true
            shift
            ;;
        *)
            echo "Unknown option: $1"
            usage
            ;;
    esac
done

# Handle help request
if [[ "$HELP_REQUESTED" == "true" ]]; then
    usage
fi

# Platform configurations
declare -A TARGETS=(
    ["linux-x86_64"]="x86_64-unknown-linux-gnu"
    ["linux-aarch64"]="aarch64-unknown-linux-gnu"
    ["macos-x86_64"]="x86_64-apple-darwin"
    ["macos-aarch64"]="aarch64-apple-darwin"
    ["windows-x86_64"]="x86_64-pc-windows-gnu"
)

# Validate platform
if [[ -z "$PLATFORM" ]]; then
    echo "Error: Platform must be specified"
    usage
fi

# Clean previous builds
rm -rf "$DIST_DIR"
mkdir -p "$DIST_DIR"

log "Starting build process..."
log "Version: $VERSION"
log "Output directory: $DIST_DIR"

if [[ "$PLATFORM" == "all" ]]; then
    log "Building for all platforms..."
    for platform in "${!TARGETS[@]}"; do
        build_platform "$platform" || log "Failed to build for $platform"
    done
else
    if [[ -z "${TARGETS[$PLATFORM]:-}" ]]; then
        echo "Error: Unknown platform '$PLATFORM'"
        echo "Available platforms: ${!TARGETS[*]} all"
        exit 1
    fi
    build_platform "$PLATFORM"
fi

log "Build process complete!"