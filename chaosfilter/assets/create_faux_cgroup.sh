#!/usr/bin/bash

CGROUP_NAME="chaos-test"
CGROUP_PATH="/sys/fs/cgroup/${CGROUP_NAME}"
PIDS_COUNT=5
PID_FILE="/tmp/chaos_test_pids"

# Usage:
#   sudo ./create_faux_cgroup.sh          # setup cgroup + PIDS
#   sudo ./create_faux_cgroup.sh teardown # kill workers and remove cgroup

die() {
  echo "ERROR: $*" >&2
  exit 1
}

[[ $EUID -eq 0 ]] || die "run as root : sudo bash $0"

teardown() {
  echo "tearing down..."
  echo ""

  if [[ -f "$PID_FILE" ]]; then
    while read -r pid; do
      if kill -0 "$pid" 2>/dev/null; then
        kill "$pid" 2>/dev/null
        echo "killed worker PID $pid"
      fi

    done <"$PID_FILE"
    rm -f "$PID_FILE"
  fi

  if [[ -d "$CGROUP_PATH" ]]; then

    while read -r pid; do
      echo "$pid" >/sys/fs/cgroup/cgroup.procs 2>/dev/null || true
    done <"${CGROUP_PATH}/cgroup.procs" 2>/dev/null || true

    echo ""
    rmdir "$CGROUP_PATH" 2>/dev/null && echo "removed $CGROUP_PATH" || true
    echo ""
  fi

  echo "done."
}

setup() {
  : >"$PID_FILE"

  if [[ ! -d "$CGROUP_PATH" ]]; then
    mkdir "$CGROUP_PATH"
    echo "created cgroup: $CGROUP_PATH"
  else
    echo "cgroup already exists, reusing: $CGROUP_PATH"
  fi

  echo ""
  echo "spawning $PIDS_COUNT hungry hungry pids..."
  echo ""

  for i in $(seq 1 "$PIDS_COUNT"); do
    bash -c '
        while true; do
          # add openat operations
          exec 3</dev/null 2>/dev/null && exec 3>&- || true

          # keep them alive
          read -t 0.1 < /dev/null || true
        done
    ' &
    wpid=$!
    echo "$wpid" >>"$PID_FILE"
    echo "$wpid" >"${CGROUP_PATH}/cgroup.procs"

    echo "PID $i: PID $wpid -> $CGROUP_PATH"
  done

  echo "cgroup.procs contents: $(tr '\n' ' ' <"${CGROUP_PATH}/cgroup.procs")"
  echo ""
  echo "Ready. Now go run ChaosFilter, champ."
  echo ""
  echo "When done: sudo ./create_faux_cgroup.sh teardown"
}

case "${1:-setup}" in
teardown) teardown ;;
setup) setup ;;
*) die "unknown argument '$1' (use 'setup' or 'teardown')" ;;
esac
