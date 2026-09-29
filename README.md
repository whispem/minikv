# minikv

Distributed key-value and object store in Rust, with Raft-replicated metadata, two-phase commit to the volume servers, and write-ahead logs.

[![Repo](https://img.shields.io/badge/github-whispem%2Fminikv-blue)](https://github.com/whispem/minikv)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange.svg)](https://rustup.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![CI](https://github.com/whispem/minikv/actions/workflows/ci.yml/badge.svg)](https://github.com/whispem/minikv/actions/workflows/ci.yml)

## v2.0.1

v2.0.1 is a correction release. Where 2.0.0 reported work that it did not do, minikv now fails explicitly:

- The cluster operations (verify, repair, compact, scale), backups, restores and TTLs answer `501 Not Implemented`, and the matching CLI commands exit with status 1. See [Operations](#operations).
- `/metrics` follows the Prometheus text format and only exports what minikv measures.
- A volume server that restarts rebuilds its index from its segments and its WAL: deleted blobs no longer come back.
- Two time-series writes in the same hour are both kept.

See the [CHANGELOG](CHANGELOG.md), and its correction note on earlier entries.

## v2.0.0

v2.0.0 reworks the distributed layer.

- Coordinators replicate metadata through a persistent Raft log: elections with log up-to-date checks, majority commit, automatic failover.
- Writes use two-phase commit between the leader and the volume servers, with one blob per object version.
- Reads are linearizable on every coordinator (ReadIndex).
- Volume servers run a gRPC storage service and send heartbeats to every coordinator.
- A new end-to-end test runs 3 coordinators and 3 volumes, and kills the leader along the way.

The v1.0.0 time-series API, vector search and Python SDK are unchanged. The Helm chart does not start a working cluster (see [Operations and Release Engineering](#operations-and-release-engineering)).

## Table of Contents

- [What is minikv](#what-is-minikv)
- [How it works](#how-it-works)
- [Quick Start](#quick-start)
- [Python SDK](#python-sdk)
- [Core Features](#core-features)
- [Operations](#operations)
- [Operations and Release Engineering](#operations-and-release-engineering)
- [Roadmap](#roadmap)
- [Development](#development)
- [Contributing](#contributing)

## What is minikv

minikv is a distributed systems reference implementation and an extensible data platform.

- Strong consistency: Raft-replicated metadata, two-phase commit to the volume servers, linearizable reads.
- Durability: write-ahead logs, checksummed segments, persistent Raft log.
- Security building blocks: API keys/JWT, RBAC, quotas, encryption.
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

### Placement

Each key is stored on the `replicas` live volumes that rank highest for it. The weight of a volume is the first 8 bytes of BLAKE3(key followed by the volume id), read as a little-endian integer; on a tie, the smaller volume id wins. The choice depends only on the key and on the set of live volumes, not on their order. When fewer volumes are live than `replicas`, the write goes to all of them, and the leader logs a warning. Reads do not recompute the placement: they use the replicas recorded in the key's metadata.

There are no shards: the `num_shards` setting is reserved for the virtual shards planned for v2.2.0.

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
- Rendezvous hashing (HRW) placement across volume servers
- Batch and range endpoints (a batch is not atomic)

Storage and query paths:

- Volume storage engine: append-only segments, WAL, CRC32 checksums, bloom filters, and an index rebuilt from the segments and the WAL at startup
- RocksDB for coordinator metadata
- In-memory time-series engine with aggregation. The points live in the memory of the coordinator that received them: they are not replicated, and a restart loses them. There is no downsampling.
- Vector similarity endpoints with cosine top-k search

Security and tenancy building blocks:

- API keys (Argon2), JWT, RBAC
- AES-256-GCM encryption
- Tenant quotas and request rate limiting
- Audit logging for admin operations

These modules are implemented and tested on their own. Enforcing them on the HTTP API is planned for v2.1.0. The audit log already records the admin operations, with the actor `unauthenticated`.

Library modules that nothing in minikv uses yet: cross-datacenter replication, change data capture, data tiering, geo routing, plugins, the Kubernetes operator types and io_uring. Where one of them would report work that it does not do, it fails with a "not implemented" error.

APIs:

- HTTP REST and S3-compatible endpoints (PUT, GET, DELETE)
- WebSocket/SSE watch endpoints
- gRPC internal communication (Raft between coordinators, 2PC with volumes)

## Operations

Writes, reads and deletes, the S3-compatible routes, batch and range, leader failover, volume heartbeats, `/health/live`, `/health/ready`, `/admin/status` and `/metrics` work.

The routes below are not implemented. They answer `501 Not Implemented` with a JSON body that names the feature and the release planned for it, for example `{"error":"not implemented","feature":"verify","roadmap":"2.1.0"}`:

| Route | Feature | Planned for |
|---|---|---|
| `POST /admin/verify` | `verify` | 2.1.0 |
| `POST /admin/repair` | `repair` | 2.2.0 |
| `POST /admin/compact` | `compact` | 2.2.0 |
| `POST /admin/scale` | `scale` | 2.2.0 |
| `POST /admin/backup`, `GET /admin/backups`, `GET` and `DELETE /admin/backups/:id` | `backup` | unscheduled |
| `POST /admin/restore` | `restore` | unscheduled |
| `PUT /s3/:bucket/:key`, `PUT /:key` and `POST /:key` with an `X-Minikv-TTL` header | `ttl` | unscheduled |

`unscheduled` means that no release plans the feature yet. A write with an `X-Minikv-TTL` header stores nothing.

The `minikv` CLI exits with:

- `0` on success;
- `1` on failure: when the coordinator answers an error to `get`, `put` or `delete`, and always for `verify`, `repair`, `compact`, `rebalance`, `upgrade` and `stream`, which are not implemented. The message goes to standard error, for example `error: verify is not implemented (roadmap: 2.1.0)`;
- `2` when the arguments are invalid.

## Operations and Release Engineering

Observability:

- Prometheus metrics at `/metrics` (text format 0.0.4): live volumes, keys and bytes per volume, Raft role, term and commit index, uptime
- Grafana dashboard provisioning
- One alert rule in `opentelemetry/prometheus-alerts.yml`: no healthy volume for 2 minutes

Kubernetes:

- The Helm chart under `k8s/helm/minikv/` does not start a working cluster: its templates pass none of the settings that the images need. See its README.
- The manifests under `k8s/operator/`, `k8s/rbac/`, `k8s/crds/` and `k8s/examples/` are for a Kubernetes operator that minikv does not have.

Runbooks:

- Backup/restore: `docs/ops-backup-restore.md`, for when backups exist: they are not implemented
- Release process, as run for v2.0.0: `docs/release-engineering-v2.0.0.md`

Preflight commands:

```bash
make release-preflight
make release-preflight-full
```

## Roadmap

v2.1.0:

- Read-only cluster verification (`verify`)
- Authentication, RBAC, quotas and encryption enforced on the HTTP API
- Raft log compaction and snapshots
- Write forwarding from followers to the leader
- Kafka Connect sink/source templates for CDC
- Read replicas for analytical traffic
- Vector index acceleration (HNSW/PQ)
- Better analytics query ergonomics

v2.2.0:

- Re-replication of the replicas lost with a volume (`repair`)
- Compaction of the volume segments (`compact`)
- Dynamic cluster membership (adding and removing coordinators, `scale`)
- Virtual shards driving placement and rebalancing (`rebalance`)
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
```

End-to-end cluster test (3 coordinators, 3 volumes, leader failover):

```bash
cargo test --release --test distributed_cluster -- --nocapture
```

Two of the time-series integration tests expect a coordinator on port 8000, as in CI:

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
  ops/          # cluster operations, not implemented yet: each one returns an error
k8s/            # Helm chart and operator manifests, neither of which works yet
opentelemetry/  # Prometheus, Grafana and Jaeger containers (minikv exports no traces)
sdk/python/     # notebook-first Python client preview
docs/           # runbooks and release engineering docs
```

## Contributing

Contributions are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for coding, testing, and PR workflow.

## License

MIT. See [LICENSE](LICENSE).