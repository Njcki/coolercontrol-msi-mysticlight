#!/usr/bin/env bash
# Removes the plugin and restarts CoolerControl:  sudo ./uninstall.sh
# State backups taken by the plugin are copied to ./backups first.
set -euo pipefail
cd "$(dirname "$0")"
ID=msi_mysticlight
DEST=/var/lib/coolercontrol/plugins/$ID
[ "$(id -u)" -eq 0 ] || { echo "Run with sudo"; exit 1; }
if [ -d "$DEST/backups" ]; then
  mkdir -p backups
  cp -n "$DEST"/backups/* backups/ 2>/dev/null || true
  [ -n "${SUDO_UID:-}" ] && chown -R "$SUDO_UID:${SUDO_GID:-$SUDO_UID}" backups
fi
systemctl stop coolercontrold
systemctl disable --now "cc-plugin-$ID.service" 2>/dev/null || true
rm -f "/etc/systemd/system/cc-plugin-$ID.service"
systemctl daemon-reload
rm -rf "$DEST"
rm -f "/run/coolercontrol-plugin-$ID.sock"
systemctl start coolercontrold
echo "Removed. CoolerControl restarted. Backups are in $(pwd)/backups"
