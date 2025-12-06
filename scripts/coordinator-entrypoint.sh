#!/bin/bash
set -e

echo "Starting minikv coordinator: ${NODE_ID}"

exec minikv-coord serve \
  --id "${NODE_ID}" \
  --bind "${HTTP_BIND}" \
  --grpc "${GRPC_BIND}" \
  --db "${DB_PATH}" \
  --peers "${PEERS}" \
  --replicas "${REPLICAS}"
