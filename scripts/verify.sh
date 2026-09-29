#!/usr/bin/env bash
# Runs `minikv verify` against a coordinator.
#
# verify is not implemented in minikv 2.0.1 (planned for 2.1.0): the command
# says so and exits with status 1, and so does this script.

set -euo pipefail

COORDINATOR="${1:-http://127.0.0.1:5000}"
DEEP="${2:-false}"

echo "Verifying cluster integrity"
echo "  Coordinator: ${COORDINATOR}"
echo "  Deep check: ${DEEP}"
echo ""

# Check that the coordinator answers
echo "Checking coordinator..."
if ! curl -sf "${COORDINATOR}/health/live" > /dev/null; then
    echo "[FAIL] Coordinator unreachable"
    exit 1
fi
echo "[OK] Coordinator reachable"

# Run CLI verify command
if [ "${DEEP}" = "true" ]; then
    ./target/release/minikv verify --coordinator "${COORDINATOR}" --deep
else
    ./target/release/minikv verify --coordinator "${COORDINATOR}"
fi
