#!/bin/bash

# Script to collect Prometheus metrics and Raft logs for minikv
# Usage: ./collect_metrics.sh <coordinator_host> <output_dir>

set -e

COORD_HOST=${1:-localhost:5000}
OUTDIR=${2:-./metrics_logs}

mkdir -p "$OUTDIR"
DATE=$(date +"%Y%m%d_%H%M%S")

echo "Collecting Prometheus metrics..."
curl -s "http://$COORD_HOST/metrics" > "$OUTDIR/metrics_$DATE.txt"

echo "Collecting Raft logs (example: via API or local file)..."
# Replace with actual command if exposed
if curl -s "http://$COORD_HOST/admin/raft_log" > "$OUTDIR/raft_log_$DATE.txt"; then
  echo "Raft log collected via HTTP endpoint."
else
  echo "Endpoint /admin/raft_log not available, log not collected."
fi

echo "Collecting system logs (optional)"
# journalctl or docker logs if applicable
# docker logs minikv-coordinator > "$OUTDIR/coordinator_docker_$DATE.txt"

echo "Metrics and logs collected in $OUTDIR."
