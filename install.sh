#!/usr/bin/env bash
# Installs the plugin into CoolerControl's plugin folder and restarts the daemon.
# Run from the project folder after building:  sudo ./install.sh
set -euo pipefail
cd "$(dirname "$0")"
ID=msi_mysticlight
DEST=/var/lib/coolercontrol/plugins/$ID
BIN=target/release/$ID
[ "$(id -u)" -eq 0 ] || { echo "Run with sudo"; exit 1; }
[ -x "$BIN" ] || { echo "Build first: cargo build --release"; exit 1; }
install -d -m 755 "$DEST" "$DEST/backups"
install -m 644 manifest.toml "$DEST/manifest.toml"
install -m 755 "$BIN" "$DEST/$ID"
if command -v restorecon >/dev/null 2>&1 && command -v semanage >/dev/null 2>&1 && selinuxenabled 2>/dev/null; then
  semanage fcontext -a -t bin_t "$DEST/$ID" 2>/dev/null || semanage fcontext -m -t bin_t "$DEST/$ID"
  restorecon -v "$DEST/$ID"
fi
systemctl restart coolercontrold
echo "Installed to $DEST. Logs: journalctl -u coolercontrold -u cc-plugin-$ID -f"
