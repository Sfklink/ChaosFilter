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

# need_cmd
#   Helper function that checks to make sure all required programs are installed and prints an error if one is missing
#   Checks for: cargo, grep/sed, python3, and stress-ng
need_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1"
        exit 1
    }
}
need_cmd cargo
need_cmd python3
need_cmd grep
need_cmd sed
need_cmd stress-ng


# pause
#   Helper function that is used throughout the demo to hold script execution until user hits enter
#   Also pauses and resumes the ping print out if running so it looks good
pause() {
    echo
    read -r -p "Press Enter to continue..." _
    echo
}

# Apply a work load to the CPU
#   Weird layout for formatting reasons
echo "Starting CPU Workload..."
pause
stress-ng --cpu 1 --cpu-method loop --cpu-load 100 --timeout 0 > /dev/null 2>&1 &
CPU_PID=$!


# Apply a workload to memory
mem_workload() {
    python3 <<'PY' &
import os
import time

chunks = []
limit_chunks = 100   # ~100 * 10MB = ~1GB max

print(f"[workload] PID={os.getpid()} growing memory...", flush=True)

while True:
    try:
        chunks.append(bytearray(10 * 1024 * 1024))
        time.sleep(0.3)
    except MemoryError:
        time.sleep(1)
PY
    MEM_PID=$!
}

# Apply a workload to filesystem (FD exhaustion)
fd_workload() {
    python3 <<'PY' &
import os
import time
import sys

fds = []
print(f"[workload] PID={os.getpid()} starting FD exhaustion workload", flush=True)

while True:
    try:
        # Open a dummy file
        f = open("/dev/null", "r")
        fds.append(f)
        time.sleep(0.1)
    except OSError as e:
        # This will happen when RLIMIT_NOFILE is reached
        print(f"[workload] Error opening file: {e}", flush=True)
        time.sleep(1)
PY
    FD_PID=$!
}


# Path to the compiled Chaosfilter binary
CHAOS_BIN="$(command -v chaosfilter || true)"

# Initialize PING_PID for use later
PING_PID=""

# Initalize both TARGET and CHAOS_PID for use later
CHAOS_PID=""
TARGET_PID=""

echo "---------------------------------------"
echo "ChaosFilter Interactive Demo"
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
    echo "Cleaning up..."
    [[ -n "${CPU_PID:-}" ]] && kill "$CPU_PID" 2>/dev/null || true
    [[ -n "${MEM_PID:-}" ]] && kill "$MEM_PID" 2>/dev/null || true
    [[ -n "${FD_PID:-}" ]] && kill "$FD_PID" 2>/dev/null || true
    [[ -n "${PING_PID:-}" ]] && kill "$PING_PID" 2>/dev/null || true
    [[ -n "${CHAOS_PID:-}" ]] && kill "$CHAOS_PID" 2>/dev/null || true

    echo
    echo "Done"
}
trap cleanup EXIT


# Config file updates
#   Enables all the sections execpt for block chaos and initializes all the variables for the chaos
set_config() {
    TARGET_PID=$1
cat <<EOF > demo_config.toml
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
mem_max = "300M"
mem_high = "200M"
swap_max = "0"

[injectors.filesystem_config]
enabled = true
target_pid = $TARGET_PID
soft_limit = 150
hard_limit = 170

EOF

echo "demo_config.toml updated for full demo"
}

# get_config_value
#   Reads from the config at a specific line that is passed when called so I can tell the user exactly
#   what is about to happen when the chaos runs
get_config_value() {
    local key="$1"
    grep "^$key" demo_config.toml | cut -d '=' -f2 | sed 's/^ *//; s/"//g'
}

# Gets the tick rate for the cpu
get_cpu_ticks() {
    local total=0

    # include parent + all children
    for pid in $(pgrep -P "$CPU_PID"); do
        if [[ -f /proc/$pid/stat ]]; then
            ticks=$(awk '{print $14 + $15}' /proc/$pid/stat)
            total=$((total + ticks))
        fi
    done

    # include parent itself
    if [[ -f /proc/$CPU_PID/stat ]]; then
        ticks=$(awk '{print $14 + $15}' /proc/$CPU_PID/stat)
        total=$((total + ticks))
    fi

    echo "$total"
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
        ((CPU > 100)) && CPU=100
        ((CPU < 0)) && CPU=0

        # ps value (for credibility)
        PS_CPU=$(ps --ppid "$CPU_PID" -o %cpu= 2>/dev/null | awk '{s+=$1} END {printf "%d", s}')
        [[ -z "$PS_CPU" ]] && PS_CPU=0

        printf "CPU (cgroup): %3d%% | ps: %3d%%\n" "$CPU" "$PS_CPU"

        sleep 1
    done
}

# Gets the memory in KB
get_mem_kb() {
    if [[ -f /proc/$MEM_PID/status ]]; then
        grep VmRSS /proc/$MEM_PID/status | awk '{print $2}'
    else
        echo 0
    fi
}

# Gets the cgroup memory in MB for the actual process cgroup path
get_mem_cgroup_mb() {
    local cgroup_rel=""
    cgroup_rel=$(awk -F: '$1=="0" {print $3}' /proc/"$MEM_PID"/cgroup 2>/dev/null || true)

    if [[ -n "$cgroup_rel" && -f "/sys/fs/cgroup${cgroup_rel}/memory.current" ]]; then
        awk '{print int($1 / 1024 / 1024)}' "/sys/fs/cgroup${cgroup_rel}/memory.current"
    else
        echo 0
    fi
}

# Watches the memory in real time while chaos is active
monitor_mem() {
    # Convert values like "200M" -> 200
    HIGH_MB=$(echo "$MEM_HIGH" | sed 's/M//')
    MAX_MB=$(echo "$MEM_MAX" | sed 's/M//')

    while kill -0 "$CHAOS_PID" 2>/dev/null; do
        MEM_KB=$(get_mem_kb)
        MEM_MB=$((MEM_KB / 1024))

        CGROUP_MB=$(get_mem_cgroup_mb)

        if (( CGROUP_MB >= HIGH_MB )); then
            STATE="LIMIT ENFORCED"
        else
            STATE="GROWING"
        fi

        printf "Memory (process): %4d MB | cgroup: %4d MB (%s)\n" \
            "$MEM_MB" "$CGROUP_MB" "$STATE"

        sleep 1
    done
}

# Gets the number of open file descriptors
get_fd_count() {
    if [[ -d /proc/$FD_PID/fd ]]; then
        ls /proc/"$FD_PID"/fd | wc -l
    else
        echo 0
    fi
}

# Gets the current FD limit for the process
get_fd_limit() {
    if [[ -f /proc/$FD_PID/limits ]]; then
        grep "Max open files" /proc/"$FD_PID"/limits | awk '{print $4}'
    else
        echo 0
    fi
}

# Watches the FDs in real time while chaos is active
monitor_fd() {
    while kill -0 "$CHAOS_PID" 2>/dev/null; do
        COUNT=$(get_fd_count)
        LIMIT=$(get_fd_limit)

        if (( COUNT >= LIMIT )); then
            STATE="EXHAUSTED"
        else
            STATE="OPENING"
        fi

        printf "Open FDs: %3d | Limit: %3d (%s)\n" \
            "$COUNT" "$LIMIT" "$STATE"

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

# Start continuous ping
stdbuf -oL ping -O 8.8.8.8 &
PING_PID=$!
kill -STOP "$PING_PID"

# Show baseline first
echo "Network Baseline"
echo "You will first see normal traffic before chaos is applied"
pause
kill -CONT "$PING_PID"
sleep 10
kill -STOP "$PING_PID"

# Get the chaos values
DELAY=$(get_config_value "delay_ms")
LOSS=$(get_config_value "loss_percent")

# Apply chaos
echo
echo "Applying ChaosFilter (NETWORK)..."
echo "ChaosFilter will inject ${DELAY}ms delay and ${LOSS}% loss"
echo "You will now see the network during chaos"
pause
echo "sudo -E $CHAOS_BIN chaos --config demo_config.toml"

sudo -E "$CHAOS_BIN" chaos --config demo_config.toml &
CHAOS_PID=$!

# Let chaos attach before traffic resumes
sleep 1
kill -CONT "$PING_PID"

wait "$CHAOS_PID"
CHAOS_PID=""

# Stop ping
kill "$PING_PID" 2>/dev/null || true
wait "$PING_PID" 2>/dev/null || true
PING_PID=""
echo "Network Chaos Done"
pause

# CPU chaos section
echo "----------------------------------"
echo "CPU Chaos Demo"
echo "----------------------------------"
pause

# Config setup
sleep 1
CPU_CHILD_PID=$(pgrep -P "$CPU_PID" | head -n 1)
set_config "$CPU_CHILD_PID"
echo

# Baseline CPU value
echo "CPU Baseline"
echo "Running CPU-intensive workload..."
PREV=$(get_cpu_ticks)
CLK_TCK=$(getconf CLK_TCK)
sleep 1

for _ in {1..10}; do
    CURR=$(get_cpu_ticks)
    DELTA=$((CURR - PREV))
    PREV=$CURR
    CPU=$((DELTA * 100 / CLK_TCK))

    ((CPU > 100)) && CPU=100
    ((CPU < 0)) && CPU=0

    PS_CPU=$(ps --ppid "$CPU_PID" -o %cpu= 2>/dev/null | awk '{s+=$1} END {printf "%d", s}')
    [[ -z "$PS_CPU" ]] && PS_CPU=0

    printf "CPU (cgroup): %3d%% | ps: %3d%%\n" "$CPU" "$PS_CPU"
    sleep 1
done

# Get the chaos values
CPU_MAX=$(get_config_value "cpu_max")
CPU_WEIGHT=$(get_config_value "cpu_weight")

# Runs ChaosFilter and the real-time results
echo
echo "Applying ChaosFilter (CPU)..."
echo "ChaosFilter will restrict CPU (quota: $CPU_MAX, weight: $CPU_WEIGHT)"
pause

sudo -E "$CHAOS_BIN" chaos --config demo_config.toml &
CHAOS_PID=$!

monitor_cpu
wait "$CHAOS_PID"
CHAOS_PID=""
echo "CPU Chaos Done"
pause

# Memory chaos section
echo "----------------------------------"
echo "Memory Chaos Demo"
echo "----------------------------------"
pause

# Start workload and config
echo "Starting workload and setting config"
mem_workload
sleep 1
set_config "$MEM_PID"

MEM_MAX=$(get_config_value "mem_max")
MEM_HIGH=$(get_config_value "mem_high")

# Start chaos
echo
echo "Applying ChaosFilter (MEMORY)..."
echo "ChaosFilter will constrain memory (soft limit: $MEM_HIGH, hard cap: $MEM_MAX)"
pause

sudo -E "$CHAOS_BIN" chaos --config demo_config.toml &
CHAOS_PID=$!

monitor_mem

wait "$CHAOS_PID"
CHAOS_PID=""

echo "Memory Chaos Done"
pause

# Filesystem chaos section
echo "----------------------------------"
echo "Filesystem Chaos Demo"
echo "----------------------------------"
pause

# Start workload and config
echo "Starting workload..."
fd_workload
sleep 1
set_config "$FD_PID"

FD_SOFT=$(get_config_value "soft_limit")
FD_HARD=$(get_config_value "hard_limit")

# Start chaos
echo
echo "Applying ChaosFilter (FILESYSTEM)..."
echo "ChaosFilter will exhaust file descriptors (soft limit: $FD_SOFT, hard limit: $FD_HARD)"
pause

sudo -E "$CHAOS_BIN" chaos --config demo_config.toml &
CHAOS_PID=$!

# Let chaos attach before we start monitoring limits
sleep 1
monitor_fd

wait "$CHAOS_PID"
CHAOS_PID=""

echo "Filesystem Chaos Done"

echo "----------------------------------"
echo "ChaosFilter Full Demo Complete"
echo "----------------------------------"