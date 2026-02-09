#!/usr/bin/env bash
#
# chaos-net-del-iface.sh
#
# PURPOSE:
#   This script deletes a single ChaosFilter virtual Ethernet (veth) pair that
#   was previously created by chaos-net-add-iface.sh. It is intended to be
#   invoked by the ChaosFilter CLI when a user selects the "delete interface"
#   or "remove qdisc interface" option.
#
# HOW IT WORKS:
#   - Accepts a single argument: the base interface name (e.g. "control")
#   - Deletes only:
#       <base-name>        (host namespace interface)
#       <base-name>-peer   (interface inside the "chaos" network namespace)
#   - No other interfaces, namespaces, or qdiscs are modified
#
# SAFETY GUARANTEES:
#   - Only the specified interface and its paired peer are removed
#   - If the interface does not exist, the script exits cleanly
#   - Other chaos interfaces and namespaces remain untouched
#
# REQUIREMENTS:
#   - Must be run as root (or via sudo)
#   - Requires the 'ip' utility (iproute2)
#
# EXAMPLE:
#   sudo ./chaos-net-del-iface.sh control
#
#   Deletes:
#     - control
#     - control-peer (from netns "chaos")
#
# NOTES:
#   - IP addresses associated with the interface are released automatically
#   - This script does NOT delete the "chaos" namespace itself
#   - Namespace cleanup (if desired) should be handled separately
#

set -e

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <iface-name>"
    exit 1
fi

BASE_IFACE="$1"
PEER_IFACE="${BASE_IFACE}-peer"
NS="chaos"

echo "[+] Deleting chaos interface pair for '$BASE_IFACE'"

# Deleting the host-side veth automatically removes its peer
# regardless of whether the peer lives in a namespace
if ip link show "$BASE_IFACE" &>/dev/null; then
    echo "[+] Removing host interface: $BASE_IFACE"
    ip link del "$BASE_IFACE"
else
    echo "[!] Interface '$BASE_IFACE' not found — nothing to delete"
fi

echo "[✓] Delete operation complete"
