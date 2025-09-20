#!/bin/bash
set -euo pipefail

# Installation script for sv2d toolkit on Linux x86_64

INSTALL_DIR="${INSTALL_DIR:-/usr/local}"
SERVICE_USER="${SERVICE_USER:-sv2d}"
CONFIG_DIR="${CONFIG_DIR:-/etc/sv2d}"
DATA_DIR="${DATA_DIR:-/var/lib/sv2d}"
LOG_DIR="${LOG_DIR:-/var/log/sv2d}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

check_root() {
    if [[ $EUID -ne 0 ]]; then
        echo "This script must be run as root (use sudo)"
        exit 1
    fi
}

create_user() {
    if ! id "$SERVICE_USER" &>/dev/null; then
        log "Creating service user: $SERVICE_USER"
        useradd --system --home-dir "$DATA_DIR" --shell /bin/false "$SERVICE_USER"
    else
        log "Service user $SERVICE_USER already exists"
    fi
}

install_binaries() {
    log "Installing binaries to $INSTALL_DIR/bin/"
    mkdir -p "$INSTALL_DIR/bin"
    
    cp bin/sv2d "$INSTALL_DIR/bin/"
    cp bin/sv2-cli "$INSTALL_DIR/bin/"
    cp bin/sv2-web "$INSTALL_DIR/bin/"
    
    chmod +x "$INSTALL_DIR/bin/sv2d"
    chmod +x "$INSTALL_DIR/bin/sv2-cli"
    chmod +x "$INSTALL_DIR/bin/sv2-web"
    
    log "Binaries installed successfully"
}

setup_directories() {
    log "Setting up directories..."
    
    mkdir -p "$CONFIG_DIR"
    mkdir -p "$DATA_DIR"
    mkdir -p "$LOG_DIR"
    
    chown "$SERVICE_USER:$SERVICE_USER" "$DATA_DIR"
    chown "$SERVICE_USER:$SERVICE_USER" "$LOG_DIR"
    
    log "Directories created and configured"
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

install_systemd_service() {
    if [[ -d /etc/systemd/system && -f systemd/sv2d.service ]]; then
        log "Installing systemd service..."
        
        # Substitute variables in service file
        sed -e "s|@INSTALL_DIR@|$INSTALL_DIR|g" \
            -e "s|@SERVICE_USER@|$SERVICE_USER|g" \
            -e "s|@CONFIG_DIR@|$CONFIG_DIR|g" \
            -e "s|@DATA_DIR@|$DATA_DIR|g" \
            systemd/sv2d.service > /etc/systemd/system/sv2d.service
        
        systemctl daemon-reload
        log "Systemd service installed. Enable with: systemctl enable sv2d"
    else
        log "Systemd not available or service file not found"
    fi
}

setup_logrotate() {
    log "Setting up log rotation..."
    
    cat > /etc/logrotate.d/sv2d << EOF
$LOG_DIR/*.log {
    daily
    missingok
    rotate 30
    compress
    delaycompress
    notifempty
    create 0644 $SERVICE_USER $SERVICE_USER
    postrotate
        systemctl reload sv2d 2>/dev/null || true
    endscript
}
EOF
    
    log "Log rotation configured"
}

main() {
    log "Starting sv2d toolkit installation..."
    
    check_root
    create_user
    install_binaries
    setup_directories
    install_config
    install_systemd_service
    setup_logrotate
    
    log "Installation complete!"
    log ""
    log "Next steps:"
    log "1. Edit configuration files in $CONFIG_DIR"
    log "2. Enable and start the service: systemctl enable --now sv2d"
    log "3. Check status: systemctl status sv2d"
    log "4. View logs: journalctl -u sv2d -f"
    log ""
    log "CLI usage: sv2-cli --help"
    log "Web dashboard: http://localhost:8080 (when sv2-web is running)"
}

main "$@"