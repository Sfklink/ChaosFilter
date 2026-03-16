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

# need_cmd 
#   Helper function that checks to make sure all required programs are installed and prints an error if one is missing 
#   Checks for: cargo, iperf3, and sed/grep 
need_cmd() {
    command -v "$1" >/dev/null 2>&1 || {
        echo "Missing required command: $1"
        exit 1
    }
}
need_cmd cargo
need_cmd iperf3

# Path to the compiled Chaosfilter binary
CHAOS_BIN="$(command -v chaosfilter || true)"

# Initalize IPERF_PID for use later 
IPERF_PID=""

echo "---------------------------------------"
echo "ChaosFilter Interactive Demo"
echo "Network Stack"
echo "---------------------------------------"

pause

# Step 0 - Installing ChaosFilter 
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
    [[ -n "${IPERF_PID:-}" ]] && kill "$IPERF_PID" 2>/dev/null || true
    pkill -f "iperf3 -s" 2>/dev/null || true
}
trap cleanup EXIT


# Config file updates 

# Network Configuration
#   Enables tc netem delay and packet loss and uses iperf to measure it all
set_network_config() {

cat <<EOF > "demo_config.toml"
# This config is based on the one made by the init command and is for the demo.
# No comments or anything just arguments

name = "example-plan"

[targets]
iface = "default"
cgroup = "0"

[schedule]
duration_s = 10

[injectors.network_config]
enabled = true
target_iface = "default"
delay_ms = 400
loss_percent = 50.0

[injectors.memory_config]
enabled = false
target_pid = 0
move_pid = true
enable = ["cpu", "memory"]
cpu_max = "max 100000"
cpu_weight = 100
mem_max = "max"
mem_high = "max"
swap_max = "max"

[injectors.block_config]
enabled = false
device = "/dev/sda"
rbps = 10485760
wbps = 10485760
EOF

echo "demo_config.toml updated for network stack demo"
}


# DEMOS

# Network Stack Chaos Section
echo "-------------------------------"
echo "Network Stack Chaos Demo:"
echo "-------------------------------"
pause

# Build the config for network chaos
set_network_config

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
echo "Applying ChaosFilter..."
echo "sudo -E $CHAOS_BIN chaos --config demo_config.toml"
kill -CONT "$IPERF_PID"
sudo -E "$CHAOS_BIN" chaos --config demo_config.toml > demo_network_results.txt 2>&1 &
CHAOS_PID=$!
wait "$CHAOS_PID"

# Stops the network load and iperf
kill "$IPERF_PID" 2>/dev/null || true
wait "$IPERF_PID" 2>/dev/null || true
echo
cat demo_network_results.txt
echo
pause

echo "----------------------------------"
echo "ChaosFilter Full Demo Complete"
echo "Cleaning up..."
echo "----------------------------------"