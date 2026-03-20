#!/usr/bin/env bash
set -euo pipefail

# ChaosFilter Full Demonstration
#
# YOU MUST RUN DEMO IN THE REPOSITORY ROOT DIRECTORY 
#
# This demo gives the user a guided walkthrough of the current working systems in ChaosFilter
# Each subsystem is tested independently of the others so the user can visibly see what is happening
#
# The user just needs to run this and hit enter occasionaly
#
# To add new sections 3 things are needed.
#   1. A workload to stress the new system
#   2. A config update section for the new system
#   3. A section a the very end that runs the new demo section


# Makes sure the binary can always be found
export PATH="$HOME/.cargo/bin:$PATH"

# pause
#   Helper function that is used throughout the demo to hold script execution until user hits enter
#   Also pauses and resumes the iperf print out if running so it looks good
pause() {
    echo
    read -r -p "Press Enter to continue..." _
}

# Apply a work load to the CPU
echo "Starting Workload Process:"
taskset -c 0 yes > /dev/null &
CPU_PID=$!
pause

# Apply a workload to memory
python3 <<'PY' &
import os
import time

chunks = []

print(f"[workload] PID={os.getpid()} growing memory safely", flush=True)

while True:
    if len(chunks) < 60:
        chunks.append(bytearray(10 * 1024 * 1024))
        time.sleep(0.5)
    else:
        time.sleep(1)
PY
MEM_PID=$!


echo "CPU PID: $CPU_PID"
echo "Memory PID: $MEM_PID"
# echo "Target PID: $WORKLOAD_PID"
pause

# need_cmd 
#   Helper function that checks to make sure all required programs are installed and prints an error if one is missing 
#   Checks for: cargo, iperf3, and python3 
need_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1"
        exit 1
    }
}
need_cmd cargo
need_cmd iperf3
need_cmd python3

# Path to the compiled Chaosfilter binary
CHAOS_BIN="$(command -v chaosfilter || true)"

# Initalize IPERF_PID for use later 
IPERF_PID=""

# Initalize both TARGET and CHAOS_PID for use later 
CHAOS_PID=""
TARGET_PID=""

echo "---------------------------------------"
echo "ChaosFilter Interactive Demo"
echo "Network Stack"
echo "---------------------------------------"
echo
echo "Initalizing sudo if needed"
sudo -v
pause

# Installing ChaosFilter 
#   Runs 'cargo install --path' to get the path to the binary with force to rebuild if it already exists
echo "Step 0 - Installing ChaosFilter:"
echo "cargo install --path chaosfilter --force"
cargo install --path chaosfilter --force
pause

# cleanup 
#   Handler that makes sure anything started by the demo stops when the demo is over or is ended early
cleanup() {
    echo
    echo "Cleanup:"
    [[ -n "${CPU_PID:-}" ]] && kill "$CPU_PID" 2>/dev/null || true
    [[ -n "${MEM_PID:-}" ]] && kill "$MEM_PID" 2>/dev/null || true
    [[ -n "${IPERF_PID:-}" ]] && kill "$IPERF_PID" 2>/dev/null || true
    [[ -n "${CHAOS_PID:-}" ]] && kill "$CHAOS_PID" 2>/dev/null || true
    pkill -f "iperf3 -s" 2>/dev/null || true
}
trap cleanup EXIT


# Config file updates 
#   Enables all the sections execpt for block chaos and initializes all the variables for the chaos
set_config() {
    TARGET_PID=$1
cat <<EOF > "demo_config.toml"
# This config is based on the one made by the init command and is for the demo.
# No comments or anything just arguments

name = "example-plan"

[targets]
iface = "default"
cgroup = "chaosfilter"

[schedule]
duration_s = 10

[injectors.network_config]
enabled = true
target_iface = "default"
delay_ms = 400
loss_percent = 50.0

[injectors.memory_config]
enabled = true
target_pid = $TARGET_PID
move_pid = true
enable = ["cpu", "memory"]
cpu_max = "20000 100000"
cpu_weight = 200
mem_max = "500M"
mem_high = "400M"
swap_max = "0"

EOF

echo "demo_config.toml updated for full demo"
}

# Gets the tick rate for the cpu
get_cpu_ticks() {
    awk '{print $14 + $15}' /proc/$TARGET_PID/stat
}

# Watches the CPU in real time while chaos is active
monitor_cpu() {
    CLK_TCK=$(getconf CLK_TCK)
    PREV=$(get_cpu_ticks)
    sleep 1

    while kill -0 "$CHAOS_PID" 2>/dev/null; do
        CURR=$(get_cpu_ticks)
        DELTA=$((CURR - PREV))
        PREV=$CURR

        CPU=$((DELTA * 100 / CLK_TCK))

        printf "CPU: %3d%%\n" "$CPU"
        sleep 1
    done
}

# Gets the memory in KB
get_mem_kb() {
    grep VmRSS /proc/$MEM_PID/status | awk '{print $2}'
}

# Watches the memory in real time while chaos is active
monitor_mem() {
    while kill -0 "$CHAOS_PID" 2>/dev/null; do
        MEM=$(get_mem_kb)
        MEM_MB=$((MEM / 1024))
        printf "Memory: %4d MB\n" "$MEM_MB"    
        sleep 1
    done
}

# DEMOS

# Network Stack Chaos Section
echo "-------------------------------"
echo "Network Stack Chaos Demo:"
echo "-------------------------------"
pause

# Build the config for chaos
set_config "$CPU_PID"

# Removes any potentially existing iperf servers
echo "Checking for and removing existing iperf servers..."
pkill -f "iperf3 -s" 2>/dev/null || true
echo

# Creates the iperf server for the demo
echo "Starting iperf server..."
iperf3 -s > /dev/null &
sleep 2
echo

# Applied the load to the network for visible network chaos
echo "Generating network traffic..."
stdbuf -oL -eL iperf3 -c 127.0.0.1 -P 2 -i 1 -t 0 &
IPERF_PID=$!
sleep 6
kill -STOP "$IPERF_PID"
pause

# Run the chaosfilter
echo "Applying ChaosFilter (NETWORK)..."
echo "sudo -E $CHAOS_BIN chaos --config demo_config.toml"
kill -CONT "$IPERF_PID"
sudo -E "$CHAOS_BIN" chaos --config demo_config.toml &
CHAOS_PID=$!
wait "$CHAOS_PID"
CHAOS_PID=""

# Stops the network load and iperf
kill "$IPERF_PID" 2>/dev/null || true
wait "$IPERF_PID" 2>/dev/null || true
IPERF_PID=""
echo
echo "Network Chaos Done"
echo
pause

# CPU chaos section
echo "----------------------------------"
echo "CPU Chaos Demo"
echo "----------------------------------"
pause

# Config setup
set_config "$CPU_PID"

# Baseline CPU value
echo "CPU Baseline"
(
    PREV=$(get_cpu_ticks)
    CLK_TCK=$(getconf CLK_TCK)
    sleep 1
    for _ in {1..10}; do
        CURR=$(get_cpu_ticks)
        DELTA=$((CURR - PREV))
        PREV=$CURR
        CPU=$((DELTA * 100 / CLK_TCK))
        printf "CPU: %3d%%\n" "$CPU"
        sleep 1
    done
)
pause

# Runs ChaosFilter and the real-time results
echo "Applying ChaosFilter (CPU)..."
sudo -E "$CHAOS_BIN" chaos --config demo_config.toml &
CHAOS_PID=$!
monitor_cpu
wait "$CHAOS_PID"
CHAOS_PID=""
echo
echo "CPU Chaos Done"
pause

# Memory chaos section
echo "----------------------------------"
echo "Memory Chaos Demo"
echo "----------------------------------"
pause

# Build the config for chaos
set_config "$MEM_PID"


# Baseline Memmory value
echo "Memory Baseline"
for _ in {1..10}; do
    MEM=$(get_mem_kb)
    printf "Memory: %4d MB\n" $((MEM / 1024))
    sleep 1
done
pause

# Runs ChaosFilter and shows real-time results
echo "Applying ChaosFilter (MEMORY)..."
sudo -E "$CHAOS_BIN" chaos --config demo_config.toml &
CHAOS_PID=$!
monitor_mem
wait "$CHAOS_PID"
CHAOS_PID=""
echo
echo "Memory Chaos Done"
pause

echo "----------------------------------"
echo "ChaosFilter Full Demo Complete"
echo "Cleaning up..."
echo "----------------------------------"