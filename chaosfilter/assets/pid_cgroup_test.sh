#!/usr/bin/env bash


#So this whole file is here to automate starting a process, and just writes the test_config
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

SRC_CFG="$ROOT/chaosfilter/src/test_pid_config.toml"
AUTO_CFG="$ROOT/chaosfilter/src/test_pid_config_auto.toml"

[[ -f "$SRC_CFG" ]] || { echo "[auto] missing: $SRC_CFG"; exit 1; }

echo "[auto] cargo build"
cargo build

# Start a yes, now it doesnt have to be yes
echo "[auto] starting CPU burner: yes > /dev/null"
yes > /dev/null &
PID="$!"
export PID
echo "[auto] burner PID = $PID"

cp -f "$SRC_CFG" "$AUTO_CFG"

#4remove stray line
perl -i -ne 'print unless /^\s*"\s*$/' "$AUTO_CFG"

# Patch/insert targets.cgroup = "PID"
perl -0777 -i -pe '
  my $pid = $ENV{PID} // "";
  die "PID env var not set\n" unless length $pid;

  if (m/^\[targets\]\n/m) {
    if (m/^\[targets\][\s\S]*?^\s*cgroup\s*=/m) {
      s/^\s*cgroup\s*=\s*".*?"\s*$/cgroup = "$pid"/m;
    } else {
      if (s/(\[targets\]\n[\s\S]*?^\s*iface\s*=\s*".*?"\s*\n)/$1cgroup = "$pid"\n/m) { }
      else { s/(\[targets\]\n)/$1cgroup = "$pid"\n/m; }
    }
  } else { die "No [targets] section found\n"; }
' "$AUTO_CFG"

# Patch/insert injectors.memory_config.pid = PID (numeric)
perl -0777 -i -pe '
  my $pid = $ENV{PID} // "";
  die "PID env var not set\n" unless length $pid;

  if (m/^\[injectors\.memory_config\]\n/m) {
    if (m/^\[injectors\.memory_config\][\s\S]*?^\s*pid\s*=/m) {
      s/^\s*pid\s*=\s*.*?$/pid = $pid/m;
    } else {
      s/(\[injectors\.memory_config\]\n)/$1pid = $pid\n/m;
    }
  } else { die "No [injectors.memory_config] section found\n"; }
' "$AUTO_CFG"

echo
echo "[auto] sanity check: show [injectors.memory_config] block in auto config"
awk '
  /^\[injectors\.memory_config\]/{in_section=1; print NR ":" $0; next}
  /^\[/{if(in_section){exit}}
  in_section{print NR ":" $0}
' "$AUTO_CFG"

# Fail fast if pid is missing (prevents your validator error)

echo
echo "[auto] running chaos plan with auto config: $AUTO_CFG"
cargo run -- chaos --config "$AUTO_CFG"