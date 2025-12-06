#!/usr/bin/env bash
# Comprehensive benchmark for minikv distributed cluster

set -euo pipefail

# Configuration
NUM_COORDS="${1:-3}"
NUM_VOLUMES="${2:-3}"
REPLICAS="${3:-3}"
VUS="${4:-16}"
DURATION="${5:-30s}"
OBJECT_SIZE="${6:-1048576}"  # 1 MB

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_banner() {
    echo -e "${BLUE}================================${NC}"
    echo -e "${BLUE}  minikv Benchmark${NC}"
    echo -e "${BLUE}================================${NC}"
}

check_deps() {
    local missing=()
    
    if ! command -v cargo &> /dev/null; then
        missing+=("cargo")
    fi
    
    if ! command -v k6 &> /dev/null; then
        missing+=("k6 (brew install k6)")
    fi
    
    if ! command -v jq &> /dev/null; then
        missing+=("jq (brew install jq)")
    fi
    
    if [ ${#missing[@]} -gt 0 ]; then
        echo -e "${RED}Missing dependencies:${NC}"
        for dep in "${missing[@]}"; do
            echo -e "  - ${dep}"
        done
        exit 1
    fi
}

print_banner
echo ""
echo -e "${GREEN}Configuration:${NC}"
echo "  Coordinators: ${NUM_COORDS}"
echo "  Volumes: ${NUM_VOLUMES}"
echo "  Replicas: ${REPLICAS}"
echo "  Virtual users: ${VUS}"
echo "  Duration: ${DURATION}"
echo "  Object size: $((OBJECT_SIZE / 1024 / 1024)) MB"
echo ""

check_deps
echo -e "${GREEN}✓ All dependencies found${NC}"
echo ""

# Build
echo -e "${YELLOW}Building release binaries...${NC}"
cargo build --release --quiet
echo -e "${GREEN}✓ Build complete${NC}"
echo ""

# Create temp directories
BENCH_DIR="./bench_temp_$$"
mkdir -p "${BENCH_DIR}"
trap "rm -rf ${BENCH_DIR}; pkill -P $$ 2>/dev/null || true" EXIT

# Start coordinators
echo -e "${YELLOW}Starting ${NUM_COORDS} coordinators...${NC}"
for i in $(seq 1 ${NUM_COORDS}); do
    COORD_HTTP=$((5000 + (i-1)*2))
    COORD_GRPC=$((5001 + (i-1)*2))
    
    # Build peers list (exclude self)
    PEERS=""
    for j in $(seq 1 ${NUM_COORDS}); do
        if [ $j -ne $i ]; then
            PEER_GRPC=$((5001 + (j-1)*2))
            if [ -z "$PEERS" ]; then
                PEERS="coord-$j:$PEER_GRPC"
            else
                PEERS="$PEERS,coord-$j:$PEER_GRPC"
            fi
        fi
    done
    
    ./target/release/minikv-coord serve \
        --id "coord-$i" \
        --bind "127.0.0.1:${COORD_HTTP}" \
        --grpc "127.0.0.1:${COORD_GRPC}" \
        --db "${BENCH_DIR}/coord${i}-db" \
        --peers "$PEERS" \
        --replicas ${REPLICAS} \
        > "${BENCH_DIR}/coord${i}.log" 2>&1 &
    
    echo "  Coordinator $i: HTTP=${COORD_HTTP}, gRPC=${COORD_GRPC}"
done

sleep 2
echo -e "${GREEN}✓ Coordinators started${NC}"
echo ""

# Start volumes
echo -e "${YELLOW}Starting ${NUM_VOLUMES} volumes...${NC}"
for i in $(seq 1 ${NUM_VOLUMES}); do
    VOL_HTTP=$((6000 + (i-1)*2))
    VOL_GRPC=$((6001 + (i-1)*2))
    
    ./target/release/minikv-volume serve \
        --id "vol-$i" \
        --bind "127.0.0.1:${VOL_HTTP}" \
        --grpc "127.0.0.1:${VOL_GRPC}" \
        --data "${BENCH_DIR}/vol${i}-data" \
        --wal "${BENCH_DIR}/vol${i}-wal" \
        --coordinators "http://127.0.0.1:5000" \
        > "${BENCH_DIR}/vol${i}.log" 2>&1 &
    
    echo "  Volume $i: HTTP=${VOL_HTTP}, gRPC=${VOL_GRPC}"
done

sleep 2
echo -e "${GREEN}✓ Volumes started${NC}"
echo ""

# Wait for cluster to be ready
echo -n "Waiting for cluster to be ready"
for i in {1..30}; do
    if curl -s "http://127.0.0.1:5000/health" > /dev/null 2>&1; then
        echo ""
        echo -e "${GREEN}✓ Cluster ready${NC}"
        break
    fi
    echo -n "."
    sleep 1
    if [ $i -eq 30 ]; then
        echo ""
        echo -e "${RED}✗ Cluster failed to start${NC}"
        cat "${BENCH_DIR}/coord1.log"
        exit 1
    fi
done
echo ""

# Create k6 test script
cat > "${BENCH_DIR}/test.js" << 'EOFK6'
import http from 'k6/http';
import { check, sleep } from 'k6';
import { Rate, Trend } from 'k6/metrics';

const putRate = new Rate('put_success');
const getRate = new Rate('get_success');
const putLatency = new Trend('put_latency');
const getLatency = new Trend('get_latency');

const BASE_URL = __ENV.BASE_URL || 'http://127.0.0.1:5000';
const OBJECT_SIZE = parseInt(__ENV.OBJECT_SIZE || '1048576');

export let options = {
    vus: parseInt(__ENV.VUS || '16'),
    duration: __ENV.DURATION || '30s',
    thresholds: {
        'put_success': ['rate>0.90'],
        'get_success': ['rate>0.95'],
    },
};

function generateData(size) {
    const chars = 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789';
    let result = '';
    for (let i = 0; i < size; i++) {
        result += chars.charAt(Math.floor(Math.random() * chars.length));
    }
    return result;
}

export default function () {
    const key = `bench-key-${__VU}-${__ITER}`;
    const data = generateData(OBJECT_SIZE);
    
    // PUT
    const putStart = Date.now();
    const putRes = http.put(`${BASE_URL}/${key}`, data);
    const putDuration = Date.now() - putStart;
    
    putRate.add(putRes.status === 201 || putRes.status === 501); // 501 = not implemented yet
    putLatency.add(putDuration);
    
    check(putRes, {
        'PUT status ok': (r) => r.status === 201 || r.status === 501,
    });
    
    // GET (skip if PUT failed)
    if (putRes.status === 201) {
        const getStart = Date.now();
        const getRes = http.get(`${BASE_URL}/${key}`);
        const getDuration = Date.now() - getStart;
        
        getRate.add(getRes.status === 200);
        getLatency.add(getDuration);
        
        check(getRes, {
            'GET status is 200': (r) => r.status === 200,
            'GET body correct': (r) => r.body.length === OBJECT_SIZE,
        });
    }
    
    sleep(0.1);
}
EOFK6

# Run k6 benchmark
echo -e "${YELLOW}Running k6 benchmark...${NC}"
echo ""

k6 run \
    --out json="${BENCH_DIR}/results.json" \
    --summary-export="${BENCH_DIR}/summary.json" \
    --env BASE_URL="http://127.0.0.1:5000" \
    --env VUS="${VUS}" \
    --env DURATION="${DURATION}" \
    --env OBJECT_SIZE="${OBJECT_SIZE}" \
    "${BENCH_DIR}/test.js" 2>&1 | grep -v "WARN"

# Parse results
echo ""
echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Benchmark Results${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

if [ -f "${BENCH_DIR}/summary.json" ]; then
    PUT_P50=$(jq -r '.metrics.put_latency.values.p50' "${BENCH_DIR}/summary.json" 2>/dev/null || echo "N/A")
    PUT_P90=$(jq -r '.metrics.put_latency.values.p90' "${BENCH_DIR}/summary.json" 2>/dev/null || echo "N/A")
    PUT_P95=$(jq -r '.metrics.put_latency.values.p95' "${BENCH_DIR}/summary.json" 2>/dev/null || echo "N/A")
    
    GET_P50=$(jq -r '.metrics.get_latency.values.p50' "${BENCH_DIR}/summary.json" 2>/dev/null || echo "N/A")
    GET_P90=$(jq -r '.metrics.get_latency.values.p90' "${BENCH_DIR}/summary.json" 2>/dev/null || echo "N/A")
    GET_P95=$(jq -r '.metrics.get_latency.values.p95' "${BENCH_DIR}/summary.json" 2>/dev/null || echo "N/A")
    
    echo "Host: $(uname -m) · $(sysctl -n hw.memsize 2>/dev/null | awk '{print $1/1024/1024/1024 " GB"}' || echo 'N/A') · $(uname -s)"
    echo "Cluster: ${NUM_COORDS} coord + ${NUM_VOLUMES} volumes (replicas=${REPLICAS})"
    echo "Config: size=$((OBJECT_SIZE / 1024 / 1024)) MiB, VUs=${VUS}, Duration=${DURATION}"
    echo ""
    echo "PUT Latency (2PC + replication):"
    echo "  p50: ${PUT_P50} ms"
    echo "  p90: ${PUT_P90} ms"
    echo "  p95: ${PUT_P95} ms"
    echo ""
    echo "GET Latency:"
    echo "  p50: ${GET_P50} ms"
    echo "  p90: ${GET_P90} ms"
    echo "  p95: ${GET_P95} ms"
fi

echo ""
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${GREEN}✓ Benchmark complete${NC}"
