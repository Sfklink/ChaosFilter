#!/usr/bin/env bash
#
# chaos-net-add-iface.sh
#
# PURPOSE:
#   This script creates a paired virtual Ethernet (veth) interface for
#   ChaosFilter network experiments. It is intended to be invoked by the
#   ChaosFilter CLI when a user selects the "add qdisc / add interface" option.
#
# HOW IT WORKS:
#   - Accepts a single argument: a base interface name (e.g. "control")
#   - Creates a veth pair:
#       <base-name>        (host namespace)
#       <base-name>-peer   (chaos network namespace)
#   - The peer interface is moved into the "chaos" network namespace
#   - Both interfaces are brought UP and assigned IPv4 addresses
#
# IP ADDRESSING:
#   The script uses a fixed, small pool of IPv4 addresses to keep behavior
#   deterministic and easy to reason about during testing.
#
#     Host namespace IPs      Chaos namespace IPs
#     ------------------      -------------------
#     10.0.0.3/24             10.0.0.4/24
#     10.0.0.5/24             10.0.0.6/24
#     10.0.0.7/24             10.0.0.8/24
#
#   The first available pair is selected automatically. If all pairs are
#   already in use, the script will fail and instruct the user to delete an
#   existing chaos interface before creating another.
#
# REQUIREMENTS:
#   - Must be run as root (or via sudo)
#   - Requires the 'ip' utility (iproute2)
#
# EXAMPLE:
#   sudo ./chaos-net-add-iface.sh control
#
#   Creates:
#     - control        (host)
#     - control-peer   (in netns "chaos")
#
# NOTES:
#   - This script does NOT attach any qdiscs by itself.
#   - Qdisc creation and modification is handled separately by the CLI.
#   - Interfaces created here are intended for root-level qdisc attachment.
#

set -e

if [[ $# -ne 1 ]]; then
    echo "Usage: $0 <iface-name>"
    exit 1
fi

BASE_IFACE="$1"
PEER_IFACE="${BASE_IFACE}-peer"
NS="chaos"

# Allowed IP pairs
HOST_IPS=(3 5 7)
NS_IPS=(4 6 8)

echo "[+] Ensuring namespace '$NS' exists"
ip netns add "$NS" 2>/dev/null || true

# Find next available IP pair
SELECTED_HOST_IP=""
SELECTED_NS_IP=""

for i in "${!HOST_IPS[@]}"; do
    H_IP="10.0.0.${HOST_IPS[$i]}"
    N_IP="10.0.0.${NS_IPS[$i]}"

    if ! ip addr show | grep -q "$H_IP" && \
       ! ip netns exec "$NS" ip addr show | grep -q "$N_IP"; then
        SELECTED_HOST_IP="$H_IP"
        SELECTED_NS_IP="$N_IP"
        break
    fi
done

if [[ -z "$SELECTED_HOST_IP" ]]; then
    echo "[!] No available IP pairs left."
    echo "    Delete an existing chaos interface before creating a new one."
    exit 1
fi

echo "[+] Using IPs:"
echo "    Host      : $SELECTED_HOST_IP/24"
echo "    Namespace : $SELECTED_NS_IP/24"

echo "[+] Creating veth pair: $BASE_IFACE <-> $PEER_IFACE"
ip link add "$BASE_IFACE" type veth peer name "$PEER_IFACE"

echo "[+] Moving $PEER_IFACE into namespace $NS"
ip link set "$PEER_IFACE" netns "$NS"

echo "[+] Configuring host interface"
ip addr add "$SELECTED_HOST_IP/24" dev "$BASE_IFACE"
ip link set "$BASE_IFACE" up

echo "[+] Configuring namespace interface"
ip netns exec "$NS" ip addr add "$SELECTED_NS_IP/24" dev "$PEER_IFACE"
ip netns exec "$NS" ip link set "$PEER_IFACE" up
ip netns exec "$NS" ip link set lo up

echo "[✓] Chaos veth pair created successfully"
echo "    Host iface      : $BASE_IFACE"
echo "    Namespace iface : $PEER_IFACE (in netns '$NS')"
