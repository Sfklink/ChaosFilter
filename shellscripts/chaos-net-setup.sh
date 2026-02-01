#!/usr/bin/env bash
#
# ChaosFilter Network Setup Script
# --------------------------------
# This script sets up the required Linux networking environment
# used by ChaosFilter to simulate network delay and packet loss.
#
# It creates:
#   - A network namespace called "chaos"
#   - A primary test veth pair (vethA <-> vethB)
#   - A control/baseline veth pair (control <-> control-peer)
#
# NOTE:
# - Must be run once per reboot
# - Requires root privileges
# - Do NOT manually add qdiscs or IFBs (the program handles those)
#

set -euo pipefail

CHAOS_NS="chaos"

# Primary test interfaces
VETH_HOST="vethA"
VETH_NS="vethB"
VETH_HOST_IP="10.0.0.1/24"
VETH_NS_IP="10.0.0.2/24"

# Control (baseline) interfaces
CTRL_HOST="control"
CTRL_NS="control-peer"
CTRL_HOST_IP="10.0.1.1/24"
CTRL_NS_IP="10.0.1.2/24"

# Ensure script is run as root
if [[ $EUID -ne 0 ]]; then
  echo "ERROR: This script must be run as root (use sudo)"
  exit 1
fi

echo "[+] Creating chaos network namespace (if missing)"
ip netns list | grep -q "^${CHAOS_NS} " || ip netns add "${CHAOS_NS}"
ip netns exec "${CHAOS_NS}" ip link set lo up

echo "[+] Creating primary test veth pair"
if ! ip link show "${VETH_HOST}" &>/dev/null; then
  ip link add "${VETH_HOST}" type veth peer name "${VETH_NS}"
  ip link set "${VETH_NS}" netns "${CHAOS_NS}"
fi

echo "[+] Configuring primary test interfaces"
ip addr add "${VETH_HOST_IP}" dev "${VETH_HOST}" 2>/dev/null || true
ip link set "${VETH_HOST}" up

ip netns exec "${CHAOS_NS}" ip addr add "${VETH_NS_IP}" dev "${VETH_NS}" 2>/dev/null || true
ip netns exec "${CHAOS_NS}" ip link set "${VETH_NS}" up
ip netns exec "${CHAOS_NS}" ip route replace default via 10.0.0.1

echo "[+] Creating control (baseline) veth pair"
if ! ip link show "${CTRL_HOST}" &>/dev/null; then
  ip link add "${CTRL_HOST}" type veth peer name "${CTRL_NS}"
  ip link set "${CTRL_NS}" netns "${CHAOS_NS}"
fi

echo "[+] Configuring control interfaces"
ip addr add "${CTRL_HOST_IP}" dev "${CTRL_HOST}" 2>/dev/null || true
ip link set "${CTRL_HOST}" up

ip netns exec "${CHAOS_NS}" ip addr add "${CTRL_NS_IP}" dev "${CTRL_NS}" 2>/dev/null || true
ip netns exec "${CHAOS_NS}" ip link set "${CTRL_NS}" up

echo "[✓] ChaosFilter network setup complete"
echo "    You may now run the ChaosFilter program."
