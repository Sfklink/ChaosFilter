#!/usr/bin/env bash
set -euo pipefail

# Demo for only Network Stack

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

# Path to the compiled Chaosfilter binary
CHAOS_BIN="$(command -v chaosfilter || true)"

# Initialize PING_PID for use later
PING_PID=""

# Initalize both TARGET and CHAOS_PID for use later
CHAOS_PID=""
TARGET_PID=""

echo "---------------------------------------"
echo "ChaosFilter Network Stack Demo"
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
enabled = true
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

# Network Stack Chaos Section
echo "-------------------------------"
echo "Network Stack Chaos Demo:"
echo "-------------------------------"
pause

# Build the config for chaos
set_config "${CPU_PID:-0}"

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
