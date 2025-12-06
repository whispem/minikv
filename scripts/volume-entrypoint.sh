#!/bin/bash
set -e

echo "Starting minikv volume: ${VOLUME_ID}"

exec minikv-volume serve \
  --id "${VOLUME_ID}" \
  --bind "${HTTP_BIND}" \
  --grpc "${GRPC_BIND}" \
  --data "${DATA_PATH}" \
  --wal "${WAL_PATH}" \
  --coordinators "${COORDINATORS}"
