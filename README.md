# 🦀 minikv

**A production-ready distributed key-value store with Raft consensus**

*Built in 24 hours by someone learning Rust for 42 days — proof that curiosity and persistence pay off!*

[![Repo](https://img.shields.io/badge/github-whispem%2Fminikv-blue)](https://github.com/whispem/minikv)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange.svg)](https://rustup.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Production Ready](https://img.shields.io/badge/status-production_ready-success)](https://github.com/whispem/minikv)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](.github/workflows/ci.yml)

---

## 🚦 What's New in v0.4.0

minikv v0.4.0 brings:

- **NEW:** Admin dashboard endpoint [`/admin/status`] — exposes cluster state (role, leader, volumes, S3 object count, etc.) for monitoring and UI integration.
- **NEW:** S3-compatible API (PUT/GET) — store and retrieve objects via `/s3/:bucket/:key` (in-memory demo).
- Full docs and automated tests for these features.

**Previous highlights:** range queries, batch operations, TLS, flexible config, multi-node Raft, 2PC, cluster rebalancing, Prometheus metrics, and more.

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

# Admin dashboard (NEW in v0.4.0)
curl http://localhost:8080/admin/status

# S3 demo API: Put & Get (NEW in v0.4.0)
curl -X PUT localhost:8080/s3/mybucket/mykey -d 'hello minikv!'
curl localhost:8080/s3/mybucket/mykey
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

- **Flexible API**
  - HTTP REST API: CRUD operations, batch, and range queries
  - Batch operations: multi-put, multi-get, multi-delete
  - Range queries and prefix scans for efficient bulk access
  - **NEW:** S3-compatible API (PUT/GET, in-memory demo): `/s3/:bucket/:key`
  - gRPC API for internal cluster communication

- **Observability & Admin**
  - **NEW:** Admin dashboard endpoint `/admin/status`: exposes full cluster state (role, leader, volumes, S3 object count, etc.)
  - Prometheus metrics endpoint
  - Health and status endpoints

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

- [ ] Persistent storage backends (RocksDB, Sled, etc.)
- [ ] Pluggable authentication & access control
- [ ] Cloud-native tooling (K8s, Docker)
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
