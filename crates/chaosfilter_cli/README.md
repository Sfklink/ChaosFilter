# ChaosFilter CLI

This directory contains the **interactive command-line interface (CLI)** for ChaosFilter. 
The CLI allows users to **apply and observe network chaos (netem qdisc)** on a selected inte>

This CLI is intended for **manual experimentation and testing**, rather than batch or config>

---

## Prerequisites

- Linux system
- `iproute2` (provides `ip` and `tc`)
- Root privileges (required for qdisc manipulation and namespaces)
- Rust toolchain (stable)

Install Rust if needed:

```bash
rustup toolchain install stable
```
Note that editing the qdisc requires running commands at root.

## Usage

Run these in the order in the root DIR (ChaosFilter) to create two (2) qdiscs for modification
```bash
sudo ./scripts/chaos-net-cleanup.sh
sudo ./scripts/chaos-net-setup.sh
```
Cleanup is optional but highly recommended

Mode 1: Config file. Run from a pre-existing *.toml file

Run the program from the root DIR with
```bash
sudo -E cargo run -p chaosfilter_cli --bin interactive
```

Mode 2: Inline flags. 

Usage: 
```bash
chaosfilter run <COMMAND>

--iface <iface>
Network interface to apply chaos to (required for inline mode)

--duration-ms <ms>
Duration to hold chaos before revert (default: 5000)

--netem-enabled <bool>
Enable/disable qdisc netem (default: true)

--netem-delay-ms <ms>
Artificial latency in milliseconds

--netem-loss-percent <percent>
Packet loss percentage (e.g. 0.2)

--cgroup <name>
Optional cgroup (relative to /sys/fs/cgroup)
(currently validated only; scoping via eBPF is future work)

--load-ebpf
Enable eBPF loading (feature-gated, optional)
```

Example:
```bash
cargo build -p chaosfilter_cli
sudo target/debug/chaosfilter_cli run --iface enp34s0 --duration-ms 5000 --netem-delay-ms 50 --netem-loss-percent 0.2
``` 

## Overview
On start with mode 1 you will be presented with the following menu
```bash
What system would you like to test?
1) Network Stack
2) Disk I/O
3) CPU / Scheduling
4) Exit
>
```
Use the corresponding numbers to navigate. At this point in time
only Networkstack is fully implemented

Selecting option 1 will take you to this menu
```bash
Network Stack options:
1) Show current qdisc state
2) Create root qdisc
3) Apply netem (delay + loss)
4) Delete root qdisc
5) Return to main menu
>
```
Again navigation is done through the corresponding number

For more information on how the backend works see the README in the root DIR (ChaosFilter)
