#!/usr/bin/env bash
# AeroPulse-NG — local webkit dev environment (no sudo required).
#
# The machine already ships the webkit RUNTIME libraries; only the -dev
# headers were missing. scripts/setup-webkit-deps.sh downloads them into
# ~/ap-deps and builds a merged sysroot; sourcing this file makes
# pkg-config resolve webkit2gtk-4.1 for the Tauri build.
export PKG_CONFIG_PATH="$HOME/ap-deps/merged/usr/lib/x86_64-linux-gnu/pkgconfig:$HOME/ap-deps/merged/usr/share/pkgconfig${PKG_CONFIG_PATH:+:$PKG_CONFIG_PATH}"
export PKG_CONFIG_SYSROOT_DIR="$HOME/ap-deps/merged"
