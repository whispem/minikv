# 🦀 minikv

**A production-ready distributed key-value store with Raft consensus**

*Built in 24 hours by someone learning Rust for 42 days — proof that curiosity and persistence pay off!*

[![Repo](https://img.shields.io/badge/github-whispem%2Fminikv-blue)](https://github.com/whispem/minikv)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange.svg)](https://rustup.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Production Ready](https://img.shields.io/badge/status-production_ready-success)](https://github.com/whispem/minikv)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](.github/workflows/ci.yml)

---

## 🚦 What's New in v0.6.0

minikv v0.6.0 brings enterprise-grade security and multi-tenancy:

- **NEW:** API Key Authentication — secure access with Argon2-hashed keys
- **NEW:** JWT Token Support — stateless auth with configurable expiration
- **NEW:** Role-Based Access Control (RBAC) — Admin, ReadWrite, ReadOnly roles
- **NEW:** Multi-tenancy — tenant isolation with per-tenant data tagging
- **NEW:** Encryption at Rest — AES-256-GCM with HKDF key derivation
- **NEW:** Tenant Quotas — storage, object count, and rate limits per tenant

**Previous highlights (v0.5.0):** TTL support, LZ4 compression, rate limiting, request tracing, enhanced metrics, Kubernetes health probes.

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
curl localhost:8080/s3/mybucket/mykey
curl localhost:8080/health/ready  # Kubernetes readiness
curl localhost:8080/health/live   # Kubernetes liveness
curl localhost:8080/metrics
cargo run -- --config config.example.toml

# Admin dashboard
curl http://localhost:8080/admin/status

# Create an API key (admin)
curl -X POST http://localhost:8080/admin/keys \
  -H "Authorization: Bearer <admin_api_key>" \
  -d '{"role":"ReadWrite","tenant_id":"acme"}'

# S3 API: Put & Get (with API key)
curl -X PUT localhost:8080/s3/mybucket/mykey \
  -H "Authorization: Bearer <api_key>" \
  -d 'hello minikv!'
curl -H "Authorization: Bearer <api_key>" localhost:8080/s3/mybucket/mykey

# TTL support: key expires in 60 seconds
curl -X PUT localhost:8080/s3/mybucket/temp-key \
  -H "Authorization: Bearer <api_key>" \
  -H "X-Minikv-TTL: 60" \
  -d 'this expires soon!'

# Health probes
curl localhost:8080/health/ready  # Kubernetes readiness
curl localhost:8080/health/live   # Kubernetes liveness

# Enhanced metrics
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


- **Data Management**
  - TTL (Time-To-Live) support for automatic key expiration
  - LZ4 compression for efficient storage (configurable)
  - Bloom filters for fast negative lookups
  - Index snapshots for fast restarts
  - **Pluggable storage backends:** In-memory, RocksDB, Sled (configurable, persistent)
  - **Persistent storage backends:** In-memory, RocksDB, Sled (configurable, production-ready)
  - **Real-time notifications:** Watch/Subscribe system (WebSocket & SSE endpoints) for key changes (PUT/DELETE/REVOKE)


- **Flexible API**
  - HTTP REST API: CRUD operations, batch, and range queries
  - Batch operations: multi-put, multi-get, multi-delete
  - Range queries and prefix scans for efficient bulk access
  - **NEW:** S3-compatible API with TTL support: `/s3/:bucket/:key`
  - gRPC API for internal cluster communication


- **Observability & Admin**
  - Admin dashboard endpoint `/admin/status`: full cluster state
  - Enhanced Prometheus metrics with latency histograms
  - Per-endpoint request/error counters
  - Kubernetes-ready health probes (`/health/ready`, `/health/live`)
  - Request ID tracking via `X-Request-ID` header
  - **Structured audit logging:** All admin and sensitive actions (file + stdout)
  - **Watch/Subscribe endpoints:** `/watch/sse` (SSE) and `/watch/ws` (WebSocket) for real-time key change events


- **Protection & Reliability**
  - Rate limiting with per-IP token bucket algorithm
  - Structured logging with tracing spans
  - TLS encryption for HTTP and gRPC endpoints
  - Graceful leader failure handling, node hot-join/removal

- **Security & Multi-tenancy (v0.6.0)**
  - **NEW:** API Key authentication with Argon2 hashing
  - **NEW:** JWT token support for stateless auth
  - **NEW:** Role-Based Access Control (Admin/ReadWrite/ReadOnly)
  - **NEW:** Multi-tenant data isolation
  - **NEW:** AES-256-GCM encryption at rest
  - **NEW:** Per-tenant quotas (storage, objects, rate limits)
  - **NEW:** Audit logging for all admin and data modification events
  - TLS encryption for HTTP and gRPC endpoints
  - Configurable via file, ENV, or CLI
  - Stateless binary (single static executable)
  - Easy deployment: works locally, on VMs, or containers

- **Reliability & Production-readiness**
  - Production-ready: memory-safe Rust core, test suite, automated CI
  - Graceful leader failure handling, node hot-join/removal
  - In-memory fast path and persistent storage backends
  - Comprehensive documentation (setup, API, integration)

- **Developer Experience**
  - Clean async/await Rust codebase
  - 100% English docs/code/comments
  - One-command local or multinode launch
  - Benchmarks and developer tooling included

---

## 🗺️ Roadmap

### Completed in v0.6.0 ✅
- [x] API Key authentication (Argon2)
- [x] JWT token support
- [x] Role-Based Access Control (RBAC)
- [x] Multi-tenancy (tenant isolation)
- [x] Encryption at rest (AES-256-GCM)
- [x] Per-tenant quotas (storage, objects, rate limits)
- [x] TTL (Time-To-Live) for automatic key expiration
- [x] LZ4 compression for storage efficiency
- [x] Rate limiting with token bucket algorithm
- [x] Request ID tracking and structured logging
- [x] Enhanced Prometheus metrics with histograms
- [x] Kubernetes health probes (readiness/liveness)
- [x] **Audit logging for admin and data events**
- [x] **Persistent storage backends (RocksDB, Sled, in-memory)**
- [x] **Watch/Subscribe system for real-time key change notifications (WebSocket/SSE)**

### Next Up (v0.7.0)
- [ ] Secondary indexes
- [ ] Multi-key transactions
- [ ] Durable S3-backed object store
- [ ] Streaming/batch import/export

### Future (v0.7.0+)
- [ ] Cross-datacenter replication
- [ ] Change Data Capture (CDC)
- [ ] Admin Web UI
- [ ] Backup & Restore
- [ ] Plugin system

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
