# minikv

Distributed, multi-tenant key-value and object store in Rust, with Raft consensus, WAL durability, and production-oriented operations.

[![Repo](https://img.shields.io/badge/github-whispem%2Fminikv-blue)](https://github.com/whispem/minikv)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange.svg)](https://rustup.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![CI](https://github.com/whispem/minikv/actions/workflows/ci.yml/badge.svg)](https://github.com/whispem/minikv/actions/workflows/ci.yml)

## v2.0.0

v2.0.0 reworks the distributed layer.

- Coordinators replicate metadata through a persistent Raft log: elections with log up-to-date checks, majority commit, automatic failover.
- Writes use two-phase commit between the leader and the volume servers, with one blob per object version.
- Reads are linearizable on every coordinator (ReadIndex).
- Volume servers run a gRPC storage service and send heartbeats to every coordinator.
- A new end-to-end test runs 3 coordinators and 3 volumes, and kills the leader along the way.

The v1.0.0 features (time-series API, vector search, Python SDK, Helm chart) are unchanged. See the [CHANGELOG](CHANGELOG.md) for details and breaking changes.

## Table of Contents

- [What is minikv](#what-is-minikv)
- [How it works](#how-it-works)
- [Quick Start](#quick-start)
- [Python SDK](#python-sdk)
- [Core Features](#core-features)
- [Operations and Release Engineering](#operations-and-release-engineering)
- [Roadmap](#roadmap)
- [Development](#development)
- [Contributing](#contributing)

## What is minikv

minikv is a distributed systems reference implementation and an extensible data platform.

- Strong consistency: Raft-replicated metadata, two-phase commit to the volume servers, linearizable reads.
- Durability: WAL and pluggable storage backends.
- Multi-tenancy and security: RBAC, API keys/JWT, encryption at rest.
- Real-time and analytics pathways: watch/SSE and time-series APIs.

## How it works

A cluster has two kinds of nodes: coordinators, which form a Raft group and hold the metadata, and volume servers, which hold the bytes. Each volume server sends a heartbeat to every coordinator listed in `--coordinators`.

A write (`PUT`) goes through the leader:

1. The leader picks the target volumes with rendezvous hashing (HRW).
2. Prepare: each target volume receives the bytes and checks their size and BLAKE3 hash, without making them visible.
3. Commit: each volume persists the blob. If one of them fails, the write is rolled back everywhere.
4. The leader appends the new metadata to its Raft log and answers once a majority of coordinators has stored it.
5. The blob of the previous version is deleted in the background.

A read (`GET`) works on any coordinator:

1. The coordinator asks the leader for its commit index (ReadIndex) and waits until it has applied it.
2. It reads the metadata locally, fetches the blob from a live replica and checks its BLAKE3 hash.

If the leader goes down, the remaining coordinators elect a new one within about a second. A follower answers writes with `503` and an `x-minikv-leader` header that names the leader.

## Quick Start

Build and run a local cluster of 3 coordinators and 3 volumes:

```bash
git clone https://github.com/whispem/minikv.git
cd minikv
make build
make serve
```

On macOS, port 5000 is taken by the AirPlay Receiver. Turn it off in System Settings › General › AirDrop & Handoff before running `make serve`.

Basic checks:

```bash
curl -s http://127.0.0.1:5000/health/live
curl -s http://127.0.0.1:5000/health/ready
curl -s http://127.0.0.1:5000/metrics
curl -s http://127.0.0.1:5000/admin/status
```

Store an object, then read it from the other coordinators. The coordinators listen on ports 5000, 5002 and 5004. Writes go to the leader: if coordinator 1 is not the leader, the `PUT` answers `503` and the `x-minikv-leader` header tells you which one is.

```bash
curl -X PUT --data-binary "hello" http://127.0.0.1:5000/s3/demo/hello.txt
curl http://127.0.0.1:5002/s3/demo/hello.txt
curl http://127.0.0.1:5004/s3/demo/hello.txt
```

## Python SDK

Notebook-first SDK (preview) for data scientists and engineers:

- Time-series write/query helpers
- Vector upsert/query helpers
- SSE change-stream consumption
- Dataframe conversions (pandas, polars, pyarrow)

Install and run example:

```bash
pip install -r sdk/python/requirements.txt
python examples/data_science_quickstart.py
```

Files:

- `sdk/python/minikv_client.py`
- `sdk/python/README.md`
- `examples/data_science_quickstart.py`

## Core Features

Distribution and consistency:

- Raft consensus: leader election, persistent log replication, majority commit
- Two-phase commit between coordinators and volume servers, with replicated blobs checked by BLAKE3
- Linearizable reads on every coordinator (ReadIndex)
- 256 virtual shards and placement management
- Multi-key operations and transaction endpoints
- Cross-DC replication primitives and conflict policies

Storage and query paths:

- Pluggable backends: RocksDB, Sled, in-memory
- WAL, compaction, and integrity tooling
- Time-series engine with aggregation and downsampling
- Vector similarity endpoints with cosine top-k search

Security and tenancy:

- API keys (Argon2), JWT, RBAC
- AES-256-GCM encryption at rest
- Tenant quotas and request rate limiting
- Audit logging

APIs:

- HTTP REST and S3-compatible endpoints (PUT, GET, DELETE)
- WebSocket/SSE watch endpoints
- gRPC internal communication (Raft between coordinators, 2PC with volumes)

## Operations and Release Engineering

Observability:

- Prometheus and OpenTelemetry integration
- Grafana dashboard provisioning
- Alert rules in `opentelemetry/prometheus-alerts.yml`

Kubernetes:

- Operator manifests under `k8s/`
- Helm chart under `k8s/helm/minikv/`

Runbooks:

- Backup/restore: `docs/ops-backup-restore.md`
- Release process: `docs/release-engineering-v1.0.0.md`

Preflight commands:

```bash
make release-preflight
make release-preflight-full
```

## Roadmap

v2.1.0:

- Raft log compaction and snapshots
- Automatic re-replication when a volume is lost
- Write forwarding from followers to the leader
- Kafka Connect sink/source templates for CDC
- Read replicas for analytical traffic
- Vector index acceleration (HNSW/PQ)
- Better analytics query ergonomics

v2.2.0:

- Dynamic cluster membership (adding and removing coordinators)
- Distributed transactions scope expansion
- Multi-region active-passive with explicit failover
- Point-in-time recovery (PITR)
- Policy-driven data lifecycle automation

Future:

- Multi-region active-active
- WebAssembly UDF sandbox
- Global secondary indexes
- Expanded SQL layer

## Development

```bash
make build
make test
make fmt
make clippy
make verify
```

End-to-end cluster test (3 coordinators, 3 volumes, leader failover):

```bash
cargo test --release --test distributed_cluster -- --nocapture
```

The time-series integration tests expect a coordinator on port 8000, as in CI:

```bash
cargo run --release --bin minikv-coord -- serve --id 1
```

Project layout:

```text
src/
  bin/          # minikv, minikv-coord, minikv-volume
  common/       # auth, backup, cdc, metrics, replication, timeseries, ...
  coordinator/  # Raft, metadata, placement, HTTP/gRPC APIs
  volume/       # volume node storage and APIs
  ops/          # integrity, compact, repair tooling
k8s/            # operator manifests and Helm chart
opentelemetry/  # Prometheus/Grafana/Jaeger stack
sdk/python/     # notebook-first Python client preview
docs/           # runbooks and release engineering docs
```

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for coding, testing, and PR workflow.

## License

MIT. See [LICENSE](LICENSE).