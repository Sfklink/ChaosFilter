# ChaosFilter Network Setup & Cleanup (One-Page Guide)

This document describes the shell scripts used to create and remove the
Linux networking environment required by **ChaosFilter**.

The scripts use Linux network namespaces and veth pairs to provide a
controlled, isolated network topology where ChaosFilter can safely
inject latency, packet loss, and other network faults using `tc` and
qdiscs.

---

## What These Scripts Do

### Setup (`chaos-net-setup.sh`)
Creates the following topology:

- Network namespace: `chaos`
- Primary test path:
  - `vethA` (host) → `vethB` (inside `chaos`)
  - IPs: `10.0.0.1/24` ↔ `10.0.0.2/24`
- Control (baseline) path:
  - `control` (host) → `control-peer` (inside `chaos`)
  - IPs: `10.0.1.1/24` ↔ `10.0.1.2/24`
- Default route inside `chaos` via `10.0.0.1`

### Cleanup (`chaos-net-cleanup.sh`)
Removes **all** resources created by the setup script:
- veth interfaces
- network namespace
- associated routing state

> These scripts **do not** create qdiscs or IFB devices.
> ChaosFilter manages those dynamically at runtime.

---

## Requirements

- Linux system with network namespace support
- `iproute2` (`ip` command available)
- Root privileges (`sudo`)
- Must be re-run after each reboot

---

## Files
shellscripts/
chaos-net-setup.sh # Create namespace + veth topology
chaos-net-cleanup.sh # Remove all created resources
README.md # This documentation

---

## Usage
______________________________________________________________________________________________________________
### 1. Set up the network (required before running ChaosFilter)
```bash
sudo ./shellscripts/chaos-net-setup.sh
______________________________________________________________________________________________________________
###2. Verify setup (recommended sanity check)
ip netns list | grep chaos

sudo ip netns exec chaos ping -c 3 -I vethB 10.0.0.1
sudo ip netns exec chaos ping -c 3 -I control-peer 10.0.1.1

Expected:

Namespace chaos exists

Pings succeed with very low latency and 0% packet loss
_______________________________________________________________________________________________________________
###3. Run ChaosFilter

From the project root:

sudo AYA_BUILD_SKIP=1 cargo run -p aya-qdisc

ChaosFilter will:

Attach netem qdiscs

Create IFB devices

Apply ingress and egress shaping dynamically
______________________________________________________________________________________________________________
###4. Clean up after testing
sudo ./shellscripts/chaos-net-cleanup.sh


The cleanup script is safe to run multiple times.

Typical Workflow
sudo ./shellscripts/chaos-net-setup.sh
sudo AYA_BUILD_SKIP=1 cargo run -p aya-qdisc
sudo ./shellscripts/chaos-net-cleanup.sh
_______________________________________________________________________________________________________________
###Notes & Warnings

Do not manually add qdiscs or IFBs

Do not modify the control interface

Intended for local testing only

Re-running setup is safe and idempotent
_________________________________________________________________________________________________________________
###Troubleshooting

If setup fails:

Run the cleanup script

Re-run the setup script

Ensure 10.0.0.0/24 and 10.0.1.0/24 are unused
________________________________________________________________________________________________________________
###Helpful inspection commands:

ip netns list
ip link
sudo ip -n chaos addr
sudo ip -n chaos route
