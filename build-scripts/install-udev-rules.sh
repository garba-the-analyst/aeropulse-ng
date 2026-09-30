#!/usr/bin/env bash
# Install udev rules for RTL-SDR / AWOS USB serial adapters (sudo-free check, root for install).
set -euo pipefail
RULE_FILE="/etc/udev/rules.d/99-aeropulse-sdr.rules"
RULE='SUBSYSTEM=="usb", ATTRS{idVendor}=="0bda", ATTRS{idProduct}=="2838", MODE="0666", GROUP="plugdev", SYMLINK+="aeropulse-1090"
SUBSYSTEM=="tty", ATTRS{idVendor}=="0403", MODE="0666", GROUP="dialout", SYMLINK+="aeropulse-awos"
SUBSYSTEM=="tty", KERNEL=="ttyUSB*", MODE="0666", GROUP="dialout"
SUBSYSTEM=="tty", KERNEL=="ttyACM*", MODE="0666", GROUP="dialout"'
if [[ "${1:-}" == "--print" ]]; then
  echo "$RULE"
  exit 0
fi
echo "Writing $RULE_FILE (requires sudo)..."
echo "$RULE" | sudo tee "$RULE_FILE" >/dev/null
sudo udevadm control --reload-rules || true
sudo udevadm trigger || true
echo "Done. Add user to plugdev/dialout: sudo usermod -aG plugdev,dialout \$USER"
