$ErrorActionPreference = "Stop"
$runId = if ($env:FLAKETIDE_RUN_ID) { [int]$env:FLAKETIDE_RUN_ID } else { 0 }
$mod = $runId % 3
Write-Output '{ "type": "suite", "event": "started", "test_count": 2 }'
Write-Output '{ "type": "test", "event": "started", "name": "fixtures::flaky::always_passes" }'
Write-Output '{ "type": "test", "name": "fixtures::flaky::always_passes", "event": "ok", "exec_time": 0.001 }'
Write-Output '{ "type": "test", "event": "started", "name": "fixtures::flaky::sometimes_fails" }'
if ($mod -eq 0) {
    Write-Output '{ "type": "test", "name": "fixtures::flaky::sometimes_fails", "event": "failed", "exec_time": 0.001, "stdout": "assertion failed" }'
    Write-Output '{ "type": "suite", "event": "failed", "passed": 1, "failed": 1, "ignored": 0, "measured": 0, "filtered_out": 0, "exec_time": 0.002 }'
    exit 101
} else {
    Write-Output '{ "type": "test", "name": "fixtures::flaky::sometimes_fails", "event": "ok", "exec_time": 0.001 }'
    Write-Output '{ "type": "suite", "event": "ok", "passed": 2, "failed": 0, "ignored": 0, "measured": 0, "filtered_out": 0, "exec_time": 0.002 }'
    exit 0
}
