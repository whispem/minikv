# minikv Helm chart

> **Not functional in minikv 2.0.1.** The templates set none of the environment variables that the entrypoints of the images need (`NODE_ID`, `HTTP_BIND`, `GRPC_BIND`, `DB_PATH`, `PEERS` and `REPLICAS` for the coordinators; `VOLUME_ID`, `HTTP_BIND`, `GRPC_BIND`, `DATA_PATH`, `WAL_PATH` and `COORDINATORS` for the volumes), so the containers exit at startup. The coordinators would also need a StatefulSet, for a stable identity and storage. To run a cluster, use `docker-compose.yml` or `scripts/serve.sh`.

## Install

```bash
helm upgrade --install minikv ./k8s/helm/minikv -f k8s/helm/minikv/values-dev.yaml
```

## Profiles

- Dev: values-dev.yaml
- Staging: values-staging.yaml
- Prod: values-prod.yaml

## Examples

```bash
helm upgrade --install minikv ./k8s/helm/minikv -f k8s/helm/minikv/values-staging.yaml
helm upgrade --install minikv ./k8s/helm/minikv -f k8s/helm/minikv/values-prod.yaml
```
