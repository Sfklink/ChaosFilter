#!/usr/bin/env bash
set -euo pipefail

# Demo for filesystem only

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
echo "ChaosFilter Filesystem Demo"
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
enabled = false
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
soft_limit = 60
hard_limit = 90

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

# Sets up the cgroup since CPU/mem are not here doing it
setup_cgroup() {
    echo "Setting up cgroup..."
    sudo mkdir -p /sys/fs/cgroup/$FD_PID
    echo "+pids" | sudo tee /sys/fs/cgroup/cgroup.subtree_control > /dev/null 2>&1 || true
    echo "$FD_PID" | sudo tee /sys/fs/cgroup/$FD_PID/cgroup.procs > /dev/null
    echo "Cgroup ready"
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


# Filesystem chaos section
echo "----------------------------------"
echo "Filesystem Chaos Demo"
echo "----------------------------------"
pause

# Start workload and config
echo "Starting workload..."
fd_workload
sleep 1
setup_cgroup
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
