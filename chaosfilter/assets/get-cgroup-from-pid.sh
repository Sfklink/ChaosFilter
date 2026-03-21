#!/usr/bin/env bash
set -euo pipefail

pid="${1:?usage: $0 <pid>}"

cgroup_path="$(cut -d: -f3 < "/proc/${pid}/cgroup")"
stat -Lc '%i %n' "/sys/fs/cgroup${cgroup_path}"