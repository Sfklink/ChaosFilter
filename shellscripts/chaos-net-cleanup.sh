#!/usr/bin/env bash
#
# ChaosFilter Network Cleanup Script
# ---------------------------------
# This script removes all networking resources created by
# chaos-net-setup.sh.
#
# It deletes:
#   - vethA (and its peer vethB)
#   - control (and its peer control-peer)
#   - the "chaos" network namespace
#
# Safe to run multiple times.
#

set -euo pipefail

CHAOS_NS="chaos"
VETH_HOST="vethA"
CTRL_HOST="control"

# Ensure script is run as root
if [[ $EUID -ne 0 ]]; then
  echo "ERROR: This script must be run as root (use sudo)"
  exit 1
fi

echo "[+] Removing primary test interface"
ip link del "${VETH_HOST}" 2>/dev/null || true

echo "[+] Removing control interface"
ip link del "${CTRL_HOST}" 2>/dev/null || true

echo "[+] Removing chaos network namespace"
ip netns del "${CHAOS_NS}" 2>/dev/null || true

echo "[✓] ChaosFilter network cleanup complete"
