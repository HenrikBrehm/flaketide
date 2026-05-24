#!/usr/bin/env bash
set -u
RUN_ID="${FLAKETIDE_RUN_ID:-0}"
MOD=$((RUN_ID % 3))
emit() { printf '%s\n' "$1"; }
emit '{ "type": "suite", "event": "started", "test_count": 2 }'
emit '{ "type": "test", "event": "started", "name": "fixtures::flaky::always_passes" }'
emit '{ "type": "test", "name": "fixtures::flaky::always_passes", "event": "ok", "exec_time": 0.001 }'
emit '{ "type": "test", "event": "started", "name": "fixtures::flaky::sometimes_fails" }'
if [ "$MOD" -eq 0 ]; then
  emit '{ "type": "test", "name": "fixtures::flaky::sometimes_fails", "event": "failed", "exec_time": 0.001, "stdout": "assertion failed" }'
  emit '{ "type": "suite", "event": "failed", "passed": 1, "failed": 1, "ignored": 0, "measured": 0, "filtered_out": 0, "exec_time": 0.002 }'
  exit 101
else
  emit '{ "type": "test", "name": "fixtures::flaky::sometimes_fails", "event": "ok", "exec_time": 0.001 }'
  emit '{ "type": "suite", "event": "ok", "passed": 2, "failed": 0, "ignored": 0, "measured": 0, "filtered_out": 0, "exec_time": 0.002 }'
  exit 0
fi
