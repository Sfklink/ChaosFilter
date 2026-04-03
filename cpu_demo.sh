#!/usr/bin/env bash
set -euo pipefail

# Demo for only CPU


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



# Path to the compiled Chaosfilter binary
CHAOS_BIN="$(command -v chaosfilter || true)"

# Initialize PING_PID for use later
PING_PID=""

# Initalize both TARGET and CHAOS_PID for use later
CHAOS_PID=""
TARGET_PID=""

echo "---------------------------------------"
echo "ChaosFilter CPU Demo"
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
