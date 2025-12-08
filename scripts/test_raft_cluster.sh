#!/bin/bash

set -e

PORTS=(50051 50052 50053)
IDS=(node1 node2 node3)

for i in {0..2}; do
  echo "Lancement du coordinator ${IDS[$i]} sur le port ${PORTS[$i]}..."
  RUST_LOG=info ./target/debug/coord \
    --id ${IDS[$i]} \
    --port ${PORTS[$i]} \
    --peers "127.0.0.1:${PORTS[(($i+1)%3)]},127.0.0.1:${PORTS[(($i+2)%3)]}" \
    > logs_${IDS[$i]}.txt 2>&1 &
  PIDS[$i]=$!
done

sleep 5

echo "Affichage des rôles et logs :"
for i in {0..2}; do
  echo "--- ${IDS[$i]} ---"
  grep -E "leader|candidate|follower|AppendEntries" logs_${IDS[$i]}.txt || true
  echo ""
done

LEADER_PID=$(ps aux | grep coord | grep leader | awk '{print $2}' | head -n1)
if [ -n "$LEADER_PID" ]; then
  echo "Kill du leader PID $LEADER_PID"
  kill $LEADER_PID
  sleep 5
  echo "Nouvelle élection :"
  for i in {0..2}; do
    echo "--- ${IDS[$i]} ---"
    grep -E "leader|candidate|follower|AppendEntries" logs_${IDS[$i]}.txt || true
    echo ""
  done
fi

trap 'kill ${PIDS[@]}' EXIT
