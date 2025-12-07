#!/usr/bin/env bash
# Verify cluster integrity

set -euo pipefail

COORDINATOR="${1:-http://127.0.0.1:5000}"
DEEP="${2:-false}"

echo "Verifying cluster integrity"
echo "  Coordinator: ${COORDINATOR}"
echo "  Deep check: ${DEEP}"
echo ""

# Check coordinator health
echo "Checking coordinator..."
if ! curl -sf "${COORDINATOR}/health" > /dev/null; then
    echo "✗ Coordinator unreachable"
    exit 1
fi
echo "✓ Coordinator healthy"

# Run CLI verify command
if [ "${DEEP}" = "true" ]; then
    ./target/release/minikv verify --coordinator "${COORDINATOR}" --deep
else
    ./target/release/minikv verify --coordinator "${COORDINATOR}"
fi
