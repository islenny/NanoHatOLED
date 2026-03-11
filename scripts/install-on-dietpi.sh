#!/usr/bin/env bash
set -euo pipefail

if [[ "$(id -u)" -ne 0 ]]; then
  echo "Run as root: sudo $0 <path-to-binary>"
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
BIN_PATH="${1:-}"
if [[ -z "$BIN_PATH" ]]; then
  echo "Usage: sudo $0 /path/to/nanohat-oled-rs"
  exit 1
fi

if [[ ! -f "$BIN_PATH" ]]; then
  echo "Binary not found: $BIN_PATH"
  exit 1
fi

install -d /opt/nanohat-oled-rs
install -m 0755 "$BIN_PATH" /usr/local/bin/nanohat-oled-rs
install -m 0644 "$ROOT_DIR/deploy/nanohat-oled-rs.service" /etc/systemd/system/nanohat-oled-rs.service

if [[ ! -f /etc/default/nanohat-oled-rs ]]; then
  install -m 0644 "$ROOT_DIR/deploy/nanohat-oled-rs.env" /etc/default/nanohat-oled-rs
fi

systemctl daemon-reload
systemctl enable --now nanohat-oled-rs.service
systemctl restart nanohat-oled-rs.service

echo "Installed and started nanohat-oled-rs."
echo "Check status: systemctl status nanohat-oled-rs.service"
