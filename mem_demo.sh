#!/usr/bin/env bash
set -euo pipefail

# Demo for only memory


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



# Path to the compiled Chaosfilter binary
CHAOS_BIN="$(command -v chaosfilter || true)"

# Initialize PING_PID for use later
PING_PID=""

# Initalize both TARGET and CHAOS_PID for use later
CHAOS_PID=""
TARGET_PID=""

echo "---------------------------------------"
echo "ChaosFilter Memory Demo"
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
#   Enables all the sections and initializes all the variables for the chaos
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
enabled = false
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
enabled = false
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
