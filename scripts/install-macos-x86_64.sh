#!/bin/bash
set -euo pipefail

# Installation script for sv2d toolkit on macOS x86_64

INSTALL_DIR="${INSTALL_DIR:-/usr/local}"
SERVICE_USER="${SERVICE_USER:-_sv2d}"
CONFIG_DIR="${CONFIG_DIR:-/usr/local/etc/sv2d}"
DATA_DIR="${DATA_DIR:-/usr/local/var/lib/sv2d}"
LOG_DIR="${LOG_DIR:-/usr/local/var/log/sv2d}"

log() {
    echo "[$(date '+%Y-%m-%d %H:%M:%S')] $*" >&2
}

check_permissions() {
    if [[ $EUID -ne 0 ]]; then
        echo "This script should be run as root (use sudo) for system-wide installation"
        echo "Or set INSTALL_DIR to a user-writable location for user installation"
        
        if [[ ! -w "$INSTALL_DIR" ]]; then
            exit 1
        fi
    fi
}

create_user() {
    if [[ $EUID -eq 0 ]]; then
        if ! dscl . -read /Users/"$SERVICE_USER" &>/dev/null; then
            log "Creating service user: $SERVICE_USER"
            
            # Find next available UID in system range
            local next_uid=$(dscl . -list /Users UniqueID | awk '{print $2}' | sort -n | tail -1)
            next_uid=$((next_uid + 1))
            
            dscl . -create /Users/"$SERVICE_USER"
            dscl . -create /Users/"$SERVICE_USER" UserShell /usr/bin/false
            dscl . -create /Users/"$SERVICE_USER" RealName "SV2D Service User"
            dscl . -create /Users/"$SERVICE_USER" UniqueID "$next_uid"
            dscl . -create /Users/"$SERVICE_USER" PrimaryGroupID 20
            dscl . -create /Users/"$SERVICE_USER" NFSHomeDirectory "$DATA_DIR"
        else
            log "Service user $SERVICE_USER already exists"
        fi
    else
        log "Skipping user creation (not running as root)"
        SERVICE_USER="$(whoami)"
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
    
    if [[ $EUID -eq 0 ]]; then
        chown "$SERVICE_USER:staff" "$DATA_DIR"
        chown "$SERVICE_USER:staff" "$LOG_DIR"
    fi
    
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

install_launchd_service() {
    if [[ -f launchd/com.sv2d.daemon.plist && $EUID -eq 0 ]]; then
        log "Installing launchd service..."
        
        # Substitute variables in plist file
        sed -e "s|@INSTALL_DIR@|$INSTALL_DIR|g" \
            -e "s|@SERVICE_USER@|$SERVICE_USER|g" \
            -e "s|@CONFIG_DIR@|$CONFIG_DIR|g" \
            -e "s|@DATA_DIR@|$DATA_DIR|g" \
            -e "s|@LOG_DIR@|$LOG_DIR|g" \
            launchd/com.sv2d.daemon.plist > /Library/LaunchDaemons/com.sv2d.daemon.plist
        
        chmod 644 /Library/LaunchDaemons/com.sv2d.daemon.plist
        log "Launchd service installed. Load with: sudo launchctl load /Library/LaunchDaemons/com.sv2d.daemon.plist"
    else
        log "Launchd service file not found or not running as root"
    fi
}

setup_newsyslog() {
    if [[ $EUID -eq 0 ]]; then
        log "Setting up log rotation..."
        
        cat >> /etc/newsyslog.conf << EOF
# sv2d log rotation
$LOG_DIR/sv2d.log    $SERVICE_USER:staff    644    7    *    @T00    J
EOF
        
        log "Log rotation configured"
    else
        log "Skipping log rotation setup (not running as root)"
    fi
}

main() {
    log "Starting sv2d toolkit installation on macOS..."
    
    check_permissions
    create_user
    install_binaries
    setup_directories
    install_config
    install_launchd_service
    setup_newsyslog
    
    log "Installation complete!"
    log ""
    log "Next steps:"
    log "1. Edit configuration files in $CONFIG_DIR"
    if [[ $EUID -eq 0 ]]; then
        log "2. Load and start the service: sudo launchctl load /Library/LaunchDaemons/com.sv2d.daemon.plist"
        log "3. Check status: sudo launchctl list | grep sv2d"
        log "4. View logs: tail -f $LOG_DIR/sv2d.log"
    else
        log "2. Start manually: $INSTALL_DIR/bin/sv2d --config $CONFIG_DIR/solo_config.toml"
    fi
    log ""
    log "CLI usage: sv2-cli --help"
    log "Web dashboard: http://localhost:8080 (when sv2-web is running)"
}

main "$@"