#!/usr/bin/env bash
# AeroPulse-NG — desktop launcher.
# Sources the local webkit dev environment, then boots the Tauri workspace
# (Rust runtime + radar window + ops HUD + sidecar).
set -e
cd "$(dirname "$0")/.."
source scripts/webkit-env.sh
exec npm run tauri dev
