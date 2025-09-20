#!/bin/bash
set -euo pipefail

# Installation script for sv2d toolkit on Windows x86_64
# This script is designed to run in Git Bash, WSL, or similar Unix-like environment on Windows

INSTALL_DIR="${INSTALL_DIR:-/c/Program Files/sv2d}"
CONFIG_DIR="${CONFIG_DIR:-/c/ProgramData/sv2d}"
DATA_DIR="${DATA_DIR:-/c/ProgramData/sv2d/data}"
LOG_DIR="${LOG_DIR:-/c/ProgramData/sv2d/logs}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

check_environment() {
    if [[ ! -d "/c" ]]; then
        echo "Error: This script requires a Unix-like environment on Windows (Git Bash, WSL, etc.)"
        echo "Please run this script from Git Bash or WSL"
        exit 1
    fi
    
    log "Detected Windows environment"
}

install_binaries() {
    log "Installing binaries to $INSTALL_DIR/"
    mkdir -p "$INSTALL_DIR"
    
    cp bin/sv2d.exe "$INSTALL_DIR/"
    cp bin/sv2-cli.exe "$INSTALL_DIR/"
    cp bin/sv2-web.exe "$INSTALL_DIR/"
    
    log "Binaries installed successfully"
}

setup_directories() {
    log "Setting up directories..."
    
    mkdir -p "$CONFIG_DIR"
    mkdir -p "$DATA_DIR"
    mkdir -p "$LOG_DIR"
    
    log "Directories created"
}

install_config() {
    log "Installing configuration files..."
    
    # Install example configs if they don't exist
    for config_file in config/*.toml; do
        if [[ -f "$config_file" ]]; then
            local basename=$(basename "$config_file")
            local target="$CONFIG_DIR/$basename"
            
            if [[ ! -f "$target" ]]; then
                cp "$config_file" "$target"
                log "Installed config: $target"
            else
                cp "$config_file" "$target.example"
                log "Config exists, installed example: $target.example"
            fi
        fi
    done
}

create_batch_scripts() {
    log "Creating Windows batch scripts..."
    
    # Create start script
    cat > "$INSTALL_DIR/start-sv2d.bat" << 'EOF'
@echo off
cd /d "%~dp0"
echo Starting sv2d daemon...
sv2d.exe --config "C:\ProgramData\sv2d\solo_config.toml"
pause
EOF

    # Create CLI wrapper
    cat > "$INSTALL_DIR/sv2-cli.bat" << 'EOF'
@echo off
cd /d "%~dp0"
sv2-cli.exe %*
EOF

    # Create web dashboard script
    cat > "$INSTALL_DIR/start-web.bat" << 'EOF'
@echo off
cd /d "%~dp0"
echo Starting sv2-web dashboard...
sv2-web.exe
pause
EOF

    log "Batch scripts created"
}

add_to_path() {
    log "Adding to system PATH..."
    
    # Create a PowerShell script to add to PATH
    cat > "$INSTALL_DIR/add-to-path.ps1" << EOF
# Add sv2d to system PATH
\$currentPath = [Environment]::GetEnvironmentVariable("PATH", "Machine")
\$sv2dPath = "$INSTALL_DIR"

if (\$currentPath -notlike "*\$sv2dPath*") {
    \$newPath = \$currentPath + ";" + \$sv2dPath
    [Environment]::SetEnvironmentVariable("PATH", \$newPath, "Machine")
    Write-Host "Added \$sv2dPath to system PATH"
    Write-Host "Please restart your command prompt to use sv2-cli from anywhere"
} else {
    Write-Host "sv2d is already in system PATH"
}
EOF

    log "PATH script created at $INSTALL_DIR/add-to-path.ps1"
    log "Run as Administrator: PowerShell -ExecutionPolicy Bypass -File \"$INSTALL_DIR/add-to-path.ps1\""
}

create_service_script() {
    log "Creating Windows service installation script..."
    
    # Create NSSM service installation script
    cat > "$INSTALL_DIR/install-service.bat" << EOF
@echo off
echo Installing sv2d as Windows service using NSSM...
echo.
echo This script requires NSSM (Non-Sucking Service Manager)
echo Download from: https://nssm.cc/download
echo.
pause

nssm install sv2d "$INSTALL_DIR\\sv2d.exe"
nssm set sv2d Parameters "--config \"$CONFIG_DIR\\solo_config.toml\""
nssm set sv2d DisplayName "SV2D Stratum V2 Daemon"
nssm set sv2d Description "Stratum V2 mining daemon"
nssm set sv2d Start SERVICE_AUTO_START

echo Service installed. Start with: nssm start sv2d
pause
EOF

    log "Service installation script created"
}

main() {
    log "Starting sv2d toolkit installation on Windows..."
    
    check_environment
    install_binaries
    setup_directories
    install_config
    create_batch_scripts
    add_to_path
    create_service_script
    
    log "Installation complete!"
    log ""
    log "Next steps:"
    log "1. Edit configuration files in $CONFIG_DIR"
    log "2. Add to PATH: Run PowerShell as Administrator and execute:"
    log "   PowerShell -ExecutionPolicy Bypass -File \"$INSTALL_DIR/add-to-path.ps1\""
    log "3. Start daemon: Double-click $INSTALL_DIR/start-sv2d.bat"
    log "4. Or install as service: Run $INSTALL_DIR/install-service.bat as Administrator"
    log ""
    log "CLI usage: sv2-cli.exe --help"
    log "Web dashboard: http://localhost:8080 (when sv2-web is running)"
}

main "$@"