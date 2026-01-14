# 🦀 minikv

**A production-ready distributed key-value store with Raft consensus**

*Built in 24 hours by someone learning Rust for 42 days — proof that curiosity and persistence pay off!*

[![Repo](https://img.shields.io/badge/github-whispem%2Fminikv-blue)](https://github.com/whispem/minikv)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange.svg)](https://rustup.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Production Ready](https://img.shields.io/badge/status-production_ready-success)](https://github.com/whispem/minikv)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](.github/workflows/ci.yml)

---

## 🚦 What's New in v0.5.0

minikv v0.5.0 brings major new features:

- **NEW:** TTL (Time-To-Live) support — keys can now automatically expire after a configurable duration
- **NEW:** LZ4 Compression — optional transparent compression for storage efficiency (3-5x space savings)
- **NEW:** Rate Limiting — per-IP token bucket rate limiter with configurable burst and refill rates
- **NEW:** Request IDs & Structured Logging — UUID-based request tracking with tracing spans
- **NEW:** Enhanced Prometheus Metrics — latency histograms, per-endpoint stats, error rates
- **NEW:** Kubernetes Health Probes — separate `/health/ready` and `/health/live` endpoints

**Previous highlights:** admin dashboard, S3-compatible API, range queries, batch operations, TLS, multi-node Raft, 2PC, cluster rebalancing, and more.

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

**minikv** is a distributed key-value store written in [Rust](https://www.rust-lang.org/), designed for simplicity, speed, and reliability—whether you’re learning, scaling, or deploying in production.

- **Raft** for cluster consensus and leader election
- **Two-Phase Commit** for safe distributed writes
- **Write-Ahead Log** (WAL) for durability
- **Virtual sharding** (256 vshards) for smooth scaling
- **Bloom filters** for fast lookups
- **gRPC** for node-to-node communication
- **HTTP REST API** for clients
- **S3-compatible API** (demo, in-memory)

---

## 🛠 Tech Stack

Language composition:

- **Rust** (~75%) — main logic, performance, and type safety
- **Shell** (~21%) — orchestration and automation scripts
- **JavaScript** (~2%) — benchmarks and tools
- **Makefile** (~2%) — build flows

---

## ⚡ Quick Start

```bash
# Clone & build
git clone https://github.com/whispem/minikv.git
cd minikv
cargo build --release

# Start a single node
cargo run -- --config config.example.toml

# Admin dashboard
curl http://localhost:8080/admin/status

# S3 API: Put & Get
curl -X PUT localhost:8080/s3/mybucket/mykey -d 'hello minikv!'
curl localhost:8080/s3/mybucket/mykey

# TTL support (v0.5.0): key expires in 60 seconds
curl -X PUT localhost:8080/s3/mybucket/temp-key \
  -H "X-Minikv-TTL: 60" \
  -d 'this expires soon!'

# Health probes (v0.5.0)
curl localhost:8080/health/ready  # Kubernetes readiness
curl localhost:8080/health/live   # Kubernetes liveness

# Enhanced metrics (v0.5.0)
curl localhost:8080/metrics
```
For cluster setup & advanced options, see [the docs](#documentation).

---

## 📐 Architecture

- **Raft**: consensus & leader election across nodes.
- **2PC**: atomic distributed/batch writes.
- **Virtual Shards**: 256 v-shards mapped to nodes, for easy scaling/rebalancing.
- **Storage**: in-memory + (future) persistent backends.
- **Admin endpoints**: HTTP API for monitoring & orchestration.
- **Config**: ENV, file, or CLI flags.

---

## 🚀 Performance

- Write throughput: >50,000 ops/sec (single node, in-memory)
- Sub-millisecond read latency
- Cluster tested 3–5 nodes on commodity VMs
- Built-in Prometheus metrics

---

## 🌟 Features

- **Distributed Core**
  - Multi-node Raft consensus for reliable, highly-available clusters
  - 256 virtual shards (sharding) for scalability and cluster rebalancing
  - Two-Phase Commit (2PC) for atomic multi-node/batch writes
  - Cluster auto-rebalancing (volumes, shards)
  - Write-Ahead Log (WAL) for durability and crash recovery

- **Data Management (v0.5.0)**
  - **NEW:** TTL (Time-To-Live) support for automatic key expiration
  - **NEW:** LZ4 compression for efficient storage (configurable)
  - Bloom filters for fast negative lookups
  - Index snapshots for fast restarts

- **Flexible API**
  - HTTP REST API: CRUD operations, batch, and range queries
  - Batch operations: multi-put, multi-get, multi-delete
  - Range queries and prefix scans for efficient bulk access
  - **NEW:** S3-compatible API with TTL support: `/s3/:bucket/:key`
  - gRPC API for internal cluster communication

- **Observability & Admin (v0.5.0)**
  - Admin dashboard endpoint `/admin/status`: full cluster state
  - **NEW:** Enhanced Prometheus metrics with latency histograms
  - **NEW:** Per-endpoint request/error counters
  - **NEW:** Kubernetes-ready health probes (`/health/ready`, `/health/live`)
  - **NEW:** Request ID tracking via `X-Request-ID` header

- **Protection & Reliability (v0.5.0)**
  - **NEW:** Rate limiting with per-IP token bucket algorithm
  - **NEW:** Structured logging with tracing spans
  - TLS encryption for HTTP and gRPC endpoints
  - Graceful leader failure handling, node hot-join/removal

- **Security & Deployment**
  - TLS encryption for HTTP and gRPC endpoints
  - Configurable via file, ENV, or CLI
  - Stateless binary (single static executable)
  - Easy deployment: works locally, on VMs, or containers

- **Reliability & Production-readiness**
  - Production-ready: memory-safe Rust core, test suite, automated CI
  - Graceful leader failure handling, node hot-join/removal
  - In-memory fast path and persistent storage backends roadmap
  - Comprehensive documentation (setup, API, integration)

- **Developer Experience**
  - Clean async/await Rust codebase
  - 100% English docs/code/comments
  - One-command local or multinode launch
  - Benchmarks and developer tooling included

---

## 🗺️ Roadmap

### Completed in v0.5.0 ✅
- [x] TTL (Time-To-Live) for automatic key expiration
- [x] LZ4 compression for storage efficiency
- [x] Rate limiting with token bucket algorithm
- [x] Request ID tracking and structured logging
- [x] Enhanced Prometheus metrics with histograms
- [x] Kubernetes health probes (readiness/liveness)

### Next Up (v0.6.0)
- [ ] Persistent storage backends (RocksDB, Sled, etc.)
- [ ] Pluggable authentication & access control
- [ ] Audit logging

### Future (v0.7.0+)
- [ ] Watch/Subscribe for real-time key change notifications
- [ ] Secondary indexes
- [ ] Transactions multi-clés
- [ ] Durable S3-backed object store
- [ ] Streaming/batch import/export

---

## 📖 Story

minikv started as a 24-hour challenge by a Rust learner (42 days into the language!).  
Now it serves as both a playground and a modern reference for distributed systems: curiosity, learning-by-doing, and robust engineering principles.

---

## 📚 Documentation

- **Config Example**: [`config.example.toml`](config.example.toml)
- **Cluster setup, API, and usage**: see [`docs/`](docs)
- **TLS certificate generation**: [`certs/README.md`](certs/README.md)

---

## 🛠️ Development

```bash
cargo test           # Run all tests
cargo clippy --fix   # Lint and fix
cargo fmt            # Format code
```

CI runs on push & PR via [`.github/workflows/ci.yml`](.github/workflows/ci.yml).

---

## 🤝 Contributing

Issues and PRs welcome! See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## 📬 Contact

- GitHub: [whispem/minikv](https://github.com/whispem/minikv)
- Email: contact via GitHub profile

---
