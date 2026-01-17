# 🦀 minikv

**A distributed, multi-tenant key-value & object store written in Rust**

minikv provides strong consistency (Raft + 2PC), durability (WAL), and production-grade observability, security, and multi-tenancy — all in a modern Rust codebase.

Built in public as a learning-by-doing project — now evolved into a complete, reference implementation of distributed systems in Rust.

[![Repo](https://img.shields.io/badge/github-whispem%2Fminikv-blue)](https://github.com/whispem/minikv)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange.svg)](https://rustup.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Production Grade](https://img.shields.io/badge/status-production_grade-success)](https://github.com/whispem/minikv)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](.github/workflows/ci.yml)

---

## 🚦 What's New in v0.7.0

minikv v0.7.0 brings advanced data management and query capabilities :

- **Secondary indexes :** Search keys by value content with `GET /search?value=<substring>`
- **Multi-key transactions :** Execute multiple operations atomically with `POST /transaction`
- **Streaming/batch import/export :** Bulk data operations with `POST /admin/import` & `GET /admin/export`
- **Durable S3-backed object store :** Persistent storage for S3-compatible API via pluggable backends

Previous highlights (v0.6.0) : enterprise security, multi-tenancy, encryption at rest, quotas, audit logging, persistent backends, watch/subscribe system.

---

## 📚 Table of Contents

- [What is minikv?](#what-is-minikv)
- [Tech Stack](#tech-stack)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [Performance](#performance)
- [Features](#features)
- [Roadmap](#roadmap)
- [Story](#story)
- [Documentation](#documentation)
- [Development](#development)
- [Contributing](#contributing)
- [Contact](#contact)

---

## 🤔 What is minikv?

minikv is a distributed key-value store written in [Rust](https://www.rust-lang.org/), designed for simplicity, speed, and reliability.

**Who is this for ?**  
minikv is for engineers learning distributed systems, teams experimenting with Rust-based infrastructure, and anyone curious about consensus, durability, and system trade-offs.

- **Clustered :** Raft consensus and 2PC for transactional writes
- **Virtual Sharding :** 256 vshards for elastic scaling & balancing
- **WAL :** Write-ahead log for durability
- **gRPC** for node communication, **HTTP REST & S3 API** for clients
- **Bloom filters, snapshots, watch/subscribe** for performance & reactivity

---

## 🛠 Tech Stack

- **Rust** – core logic
- **Shell** – orchestration/automation
- **JavaScript** – benchmarks, tools
- **Makefile** – build flows

---

## ⚡ Quick Start

```bash
git clone https://github.com/whispem/minikv.git
cd minikv
cargo build --release

# Start a node
cargo run -- --config config.example.toml

# API examples
curl localhost:8080/health/ready   # readiness
curl localhost:8080/metrics        # Prometheus metrics
curl localhost:8080/admin/status   # admin dashboard

# Create API key (admin)
curl -X POST http://localhost:8080/admin/keys -d '{"role":"ReadWrite","tenant_id":"acme"}'

# S3 (demo)
curl -X PUT localhost:8080/s3/mybucket/mykey -d 'hello minikv!'
curl localhost:8080/s3/mybucket/mykey
```
For cluster setup and advanced options, see the [documentation](#documentation).

---

## 📐 Architecture

- **Raft**: consensus and leader election
- **2PC**: atomic distributed/batch writes
- **Virtual Shards**: scale and rebalance across 256 partitions
- **Pluggable Storage**: in-memory, RocksDB, Sled
- **Admin API**: HTTP endpoints for status, metrics and config
- **Config**: via environment, file or CLI flags

---

## 🚀 Performance

- Write throughput : over 50,000 operations/sec (single node, in-memory)
- Sub-millisecond read latency
- Cluster tested (3–5 nodes, commodity VMs)
- Built-in Prometheus metrics

---

## 🌟 Features

### Distributed Core
- Raft consensus (multi-node, strong consistency)
- Two-phase commit (2PC) for atomic multi-key transactions
- 256 virtual shards for cluster scaling and rebalancing
- Write-ahead log (WAL) for durability
- Auto-rebalancing, graceful leader failover, hot-join and node removal

### Data Management
- Time-To-Live keys (TTL)
- LZ4 compression (configurable)
- Bloom filters and index snapshots
- Pluggable and persistent storage: in-memory, RocksDB, Sled
- Batch & range operations, prefix queries

### API
- HTTP REST (CRUD, batch, range, admin)
- S3-compatible API (with TTL extensions)
- gRPC (internal)
- WebSocket and SSE endpoints for real-time watch/subscribe events

### Security & Multi-tenancy
- API keys (Argon2) and JWT authentication
- Role-based access control (RBAC) and audit logging
- Multi-tenant isolation
- AES-256-GCM encryption at rest
- Per-tenant quotas (storage, requests, rate limits)
- TLS (HTTP & gRPC)

### Observability
- Admin dashboard
- Prometheus metrics (counters, histograms)
- Request and endpoint statistics
- Structured logging and tracing spans
- Kubernetes health probes

### Production-grade Design
- Memory-safe Rust
- Test suite, automated CI
- Documentation and sample config
- Single static binary

---

## 🗺️ Roadmap

### v0.7.0 (latest)
- [x] Secondary indexes
- [x] Multi-key transactions
- [x] Durable S3-backed object store
- [x] Batch import/export

### Next (v0.8.0+)
- [ ] Cross-datacenter replication
- [ ] Change Data Capture (CDC)
- [ ] Admin Web UI
- [ ] Backup & Restore
- [ ] Plugin system

---

## 📖 Story

minikv started as a 24-hour challenge by a Rust learner (42 days into the language!). It now serves as both a playground and a reference for distributed systems, demonstrating curiosity, learning-by-doing, and robust engineering.

---

## 📚 Documentation

- **Example config :** [`config.example.toml`](config.example.toml)
- **Cluster, API, usage :** see [`docs/`](docs)
- **Certificate generation :** [`certs/README.md`](certs/README.md)

---

## 🛠️ Development

```bash
cargo test           # Run all tests
cargo clippy --fix   # Lint and fix
cargo fmt            # Format code
```

Continuous Integration runs on push & PR via [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

---

## 🤝 Contributing

Issues and PRs welcome! See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## 📬 Contact

- GitHub: [whispem/minikv](https://github.com/whispem/minikv)
- Email: via GitHub profile

---