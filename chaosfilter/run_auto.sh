#!/usr/bin/env bash
#!/usr/bin/env bash
set -euo pipefail

# Always anchor paths to repo root (where this script lives)
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SRC_CFG="$ROOT/src/test_config.toml"
AUTO_CFG="$ROOT/src/test_config_auto.toml"

if [[ ! -f "$SRC_CFG" ]]; then
  echo "[auto] ERROR: config not found at $SRC_CFG"
  echo "[auto] Current directory: $(pwd)"
  echo "[auto] Available candidates:"
  find "$ROOT" -name "test_config.toml"
  exit 1
fi

echo "[auto] cargo build"
cargo build

echo "[auto] starting sleep 120 in background..."
sleep 120 &
PID="$!"
echo "[auto] sleep PID = $PID"

echo "[auto] copying config: $SRC_CFG -> $AUTO_CFG"
cp -f "$SRC_CFG" "$AUTO_CFG"

# 1) Remove a line that is just a double-quote (your current corruption)
#    (also strips whitespace around it)
perl -i -ne 'print unless /^\s*"\s*$/' "$AUTO_CFG"

# 2) Ensure [targets] has cgroup = "PID"
perl -0777 -i -pe '
  my $pid = $ENV{PID};
  if (m/^\[targets\]\n/m) {
    if (m/^\[targets\][\s\S]*?^\s*cgroup\s*=/m) {
      s/(^\s*cgroup\s*=\s*")\d+(")/$1$pid$2/m;
    } else {
      # Insert cgroup right after iface line if present, else right after [targets]
      if (s/(\[targets\]\n[\s\S]*?^\s*iface\s*=\s*".*?"\s*\n)/$1cgroup = "$pid"\n/m) {
        # inserted after iface
      } else {
        s/(\[targets\]\n)/$1cgroup = "$pid"\n/m;
      }
    }
  }
' "$AUTO_CFG"

# 3) Ensure [injectors.memory_config] has pid = PID
perl -0777 -i -pe '
  my $pid = $ENV{PID};
  if (m/^\[injectors\.memory_config\]\n/m) {
    if (m/^\[injectors\.memory_config\][\s\S]*?^\s*pid\s*=/m) {
      s/(^\s*pid\s*=\s*)\d+/$1$pid/m;
      s/(^\s*pid\s*=\s*")\d+(")/$1$pid$2/m; # if quoted form exists
    } else {
      s/(\[injectors\.memory_config\]\n)/$1pid = $pid\n/m;
    }
  }
' "$AUTO_CFG"

echo "[auto] patched targets.cgroup and injectors.memory_config.pid:"
grep -nE '^\s*(cgroup\s*=|pid\s*=)\s*' "$AUTO_CFG" || true

echo "[auto] running chaos plan with auto config..."
cargo run -- chaos --config "$AUTO_CFG"

echo "[auto] done. (sleep PID was $PID)"
