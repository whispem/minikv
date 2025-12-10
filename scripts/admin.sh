#!/usr/bin/env bash
# Admin automation script for minikv cluster
# Usage: ./admin.sh <command> [args]

set -euo pipefail

COMMAND="${1:-help}"

case "$COMMAND" in
    health)
        echo "Checking cluster health..."
        curl -s http://localhost:5000/health
        ;;
    rebalance)
        echo "Triggering auto-rebalancing..."
        cargo run --release --bin cli rebalance --coordinator http://localhost:5000
        ;;
    upgrade)
        echo "Preparing seamless upgrade..."
        cargo run --release --bin cli upgrade --coordinator http://localhost:5000
        ;;
    stream)
        KEY="${2:-}"
        if [ -z "$KEY" ]; then
            echo "Usage: ./admin.sh stream <key>"
            exit 1
        fi
        echo "Streaming large blob for key: $KEY"
        cargo run --release --bin cli stream --key "$KEY" --coordinator http://localhost:5000
        ;;
    *)
        echo "Available commands: health, rebalance, upgrade, stream <key>"
        ;;
esac
