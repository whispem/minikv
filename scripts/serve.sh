#!/usr/bin/env bash
# Convenience script to start a local minikv cluster

set -euo pipefail

NUM_COORDS="${1:-3}"
NUM_VOLUMES="${2:-3}"

echo "Starting minikv cluster"
echo "  Coordinators: ${NUM_COORDS}"
echo "  Volumes: ${NUM_VOLUMES}"
echo ""

# Build first
echo "Building..."
cargo build --release

# Create data directories
mkdir -p data/coord{1..${NUM_COORDS}}
mkdir -p data/vol{1..${NUM_VOLUMES}}-{data,wal}

# Start coordinators
echo "Starting coordinators..."
for i in $(seq 1 ${NUM_COORDS}); do
    COORD_HTTP=$((5000 + (i-1)*2))
    COORD_GRPC=$((5001 + (i-1)*2))
    
    # Build peers
    PEERS=""
    for j in $(seq 1 ${NUM_COORDS}); do
        if [ $j -ne $i ]; then
            PEER_GRPC=$((5001 + (j-1)*2))
            if [ -z "$PEERS" ]; then
                PEERS="127.0.0.1:$PEER_GRPC"
            else
                PEERS="$PEERS,127.0.0.1:$PEER_GRPC"
            fi
        fi
    done
    
    ./target/release/minikv-coord serve \
        --id "coord-$i" \
        --bind "127.0.0.1:${COORD_HTTP}" \
        --grpc "127.0.0.1:${COORD_GRPC}" \
        --db "./data/coord${i}" \
        --peers "$PEERS" \
        --replicas 3 \
        > "./data/coord${i}.log" 2>&1 &
    
    echo "  ✓ Coordinator $i: http://127.0.0.1:${COORD_HTTP}"
done

sleep 2

# Start volumes
echo "Starting volumes..."
for i in $(seq 1 ${NUM_VOLUMES}); do
    VOL_HTTP=$((6000 + (i-1)*2))
    VOL_GRPC=$((6001 + (i-1)*2))
    
    ./target/release/minikv-volume serve \
        --id "vol-$i" \
        --bind "127.0.0.1:${VOL_HTTP}" \
        --grpc "127.0.0.1:${VOL_GRPC}" \
        --data "./data/vol${i}-data" \
        --wal "./data/vol${i}-wal" \
        --coordinators "http://127.0.0.1:5000" \
        > "./data/vol${i}.log" 2>&1 &
    
    echo "  ✓ Volume $i: http://127.0.0.1:${VOL_HTTP}"
done

echo ""
echo "Cluster started!"
echo ""
echo "Coordinator: http://127.0.0.1:5000"
echo "Logs: ./data/*.log"
echo ""
echo "Press Ctrl+C to stop all processes"

# Wait and cleanup on exit
wait
