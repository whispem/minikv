#!/usr/bin/env bash
# Run all benchmark scenarios

set -euo pipefail

RESULTS_DIR="bench/results/$(date +%Y%m%d-%H%M%S)"
mkdir -p "${RESULTS_DIR}"

echo "🏃 Running all benchmark scenarios"
echo "Results will be saved to: ${RESULTS_DIR}"
echo ""

# Start cluster
echo "Starting test cluster..."
./scripts/serve.sh 1 1 &
CLUSTER_PID=$!
sleep 5

cleanup() {
    echo "Stopping cluster..."
    kill ${CLUSTER_PID} 2>/dev/null || true
    wait ${CLUSTER_PID} 2>/dev/null || true
}
trap cleanup EXIT

# Run scenarios
scenarios=(
    "write-heavy:Write-Heavy (90% writes)"
    "read-heavy:Read-Heavy (90% reads)"
)

for scenario in "${scenarios[@]}"; do
    IFS=':' read -r name desc <<< "$scenario"
    
    echo ""
    echo "📊 Running: ${desc}"
    echo "─────────────────────────────────"
    
    k6 run \
        --out json="${RESULTS_DIR}/${name}.json" \
        --summary-export="${RESULTS_DIR}/${name}-summary.json" \
        "bench/scenarios/${name}.js" \
        2>&1 | tee "${RESULTS_DIR}/${name}.log"
    
    echo "✓ ${desc} completed"
done

echo ""
echo "✅ All benchmarks completed!"
echo "Results: ${RESULTS_DIR}"

# Generate summary report
cat > "${RESULTS_DIR}/SUMMARY.md" << EOF
# Benchmark Results

**Date:** $(date)
**Cluster:** 1 coordinator + 1 volume
**Machine:** $(uname -m)

## Scenarios

EOF

for scenario in "${scenarios[@]}"; do
    IFS=':' read -r name desc <<< "$scenario"
    
    if [ -f "${RESULTS_DIR}/${name}-summary.json" ]; then
        echo "### ${desc}" >> "${RESULTS_DIR}/SUMMARY.md"
        echo "\`\`\`" >> "${RESULTS_DIR}/SUMMARY.md"
        jq . "${RESULTS_DIR}/${name}-summary.json" >> "${RESULTS_DIR}/SUMMARY.md"
        echo "\`\`\`" >> "${RESULTS_DIR}/SUMMARY.md"
        echo "" >> "${RESULTS_DIR}/SUMMARY.md"
    fi
done

echo ""
echo "Summary report: ${RESULTS_DIR}/SUMMARY.md"
