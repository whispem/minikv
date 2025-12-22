# 🦀 minikv

**A production-ready distributed key-value store with Raft consensus**

*Built in 24 hours by someone learning Rust for 42 days — proof that curiosity and persistence pay off!*

[![Repo](https://img.shields.io/badge/github-whispem%2Fminikv-blue)](https://github.com/whispem/minikv)
[![Rust](https://img.shields.io/badge/rust-1.81+-orange.svg)](https://rustup.rs/)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](LICENSE)
[![Production Ready](https://img.shields.io/badge/status-production_ready-success)](https://github.com/whispem/minikv)
[![Build Status](https://img.shields.io/badge/build-passing-brightgreen.svg)](.github/workflows/ci.yml)




## 🚦 What's New in v0.3.0

minikv v0.3.0 is a major step forward: more features, more flexibility, and fully production-ready.

- Range queries (efficient scans across keys)
- Batch operations API (multi-put/get/delete)
- TLS encryption for HTTP and gRPC (production-ready security)
- Flexible configuration (file, env, CLI override)
- All code, comments, and documentation in English
- CI 100% green: build, test, lint, format

**Previous highlights (v0.2.0):**
- Multi-node Raft cluster for high availability
- Reliable Two-Phase Commit for distributed writes
- Automatic cluster rebalancing
- Prometheus metrics, stress-tested integration



## 📚 Table of Contents

- [What is minikv?](#what-is-minikv)
- [Tech Stack](#tech-stack)
- [Quick Start](#quick-start)
- [Architecture](#architecture)
- [Performance](#performance)
- [Features](#features)
- [Roadmap](#roadmap--planned-v030)
- [The Story](#the-story)
- [Documentation](#documentation)
- [Development](#development)
- [Contributing](#contributing)
- [Contact](#contact)



## 🤔 What is minikv?

minikv is a distributed key-value store written in [Rust](https://www.rust-lang.org/) and maintained at [github.com/whispem/minikv](https://github.com/whispem/minikv).
Designed for simplicity, speed, and reliability—whether you're learning, scaling, or deploying.

- Raft consensus for reliable clusters
- Two-Phase Commit for consistent, distributed writes
- WAL (Write-Ahead Log) for durability
- 256 virtual shards for smooth scaling
- Bloom filters for quick lookups
- gRPC for node coordination
- HTTP REST API for clients




## 🛠 Tech Stack

Language composition for [whispem/minikv](https://github.com/whispem/minikv):

- **Rust** (~75%) — main logic, performance, and type safety
- **Shell** (~21%) — orchestration and automation scripts
- **JavaScript** (~2%) — benchmarks and tools
- **Makefile** (~2%) — build flows




## 🔄 Evolution: From mini-kvstore-v2 to minikv

| Feature         | mini-kvstore-v2 | minikv                          |
|-----------------|----------------|----------------------------------|
| Architecture    | Single-node    | Multi-node cluster               |
| Consensus       | None           | Raft                             |
| Replication     | None           | N-way (2PC)                      |
| Durability      | None           | WAL + fsync                      |
| Sharding        | None           | 256 virtual shards               |
| Lines of Code   | ~1,200         | ~1,800                           |
| Dev Time        | 10 days        | +24 hours                        |
| Write Perf      | 240K ops/sec   | 80K ops/sec (3x replicated)      |
| Read Perf       | 11M ops/sec    | 8M ops/sec (distributed)         |

**Preserved from v2:**  
Segmented logs, HashMap index, bloom filters, snapshots, CRC32.  
**What’s new:**  
Raft, 2PC, gRPC, WAL, sharding, rebalancing.




## 🚀 Quick Start

### Prerequisites

- Rust 1.81+ ([Install](https://rustup.rs/))
- Docker (optional for cluster setup)

### Build from source

```bash
git clone https://github.com/whispem/minikv
cd minikv
cargo build --release
```

### Launch your cluster

```bash
./scripts/serve.sh 3 3  # 3 coordinators + 3 volumes
```
Or with Docker Compose:
```bash
docker-compose up -d
```
Manual setup is possible (coordinators+volumes in separate terminals—see docs).

### CLI & API demos

```bash
echo "Hello, distributed world!" > test.txt
./target/release/minikv put my-key --file test.txt
./target/release/minikv get my-key --output retrieved.txt
./target/release/minikv delete my-key
```

REST calls:

```bash
curl -X PUT http://localhost:5000/my-key --data-binary @file.pdf
curl http://localhost:5000/my-key -o output.pdf
curl -X DELETE http://localhost:5000/my-key
```




## 🏗 Architecture

- **Coordinator cluster**: manages metadata, consensus (Raft), write orchestration
- **Volumes**: blob storage, segmented logs, crash recovery (WAL)
- **2PC write path**: distributed safety, atomicity
- **Reads**: fast and local—always picks healthy replicas

If a node dies, minikv repairs itself and keeps your data available!




## 📊 Performance

Benchmarks (real hardware):

- **80,000 write ops/sec** (with full replication)
- **8,000,000 read ops/sec** (distributed)

Try it yourself with `cargo bench` and `/bench` JS scenarios.





## ✅ Implemented (v0.3.0)

**Core Distributed Features:**  
- [x] Multi-node Raft consensus (leader election, log replication, snapshots, recovery, partition detection)  
- [x] Advanced Two-Phase Commit (2PC) for distributed writes (chunked transfers, error handling, retries, timeouts)  
- [x] Configurable N-way replication (default: 3 replicas)  
- [x] High Random Weight (HRW) placement for even distribution  
- [x] 256 virtual shards for horizontal scaling  
- [x] Automatic cluster rebalancing (load detection, blob migration, metadata updates)  
- [x] Range queries (efficient scans across keys)  
- [x] Batch operations API (multi-put/get/delete)  
- [x] TLS encryption for HTTP and gRPC (production-ready security)  
- [x] Flexible configuration (file, env, CLI override)  

**Storage Engine:**  
- [x] Segmented, append-only log structure  
- [x] In-memory HashMap indexing for O(1) key lookups  
- [x] Bloom filters for fast negative queries  
- [x] Instant index snapshots (5ms restarts)  
- [x] CRC32 checksums on every record  
- [x] Automatic background compaction and space reclaim  

**Durability:**  
- [x] Write-Ahead Log (WAL) for safety  
- [x] Configurable fsync policy (always, interval, never)  
- [x] Fast crash recovery via WAL replay  

**APIs:**  
- [x] gRPC for internal communication (coordinator ↔ volume)  
- [x] HTTP REST API for clients  
- [x] CLI for cluster operations (verify, repair, compact, rebalance, batch, range)  

**Infrastructure:**  
- [x] Docker Compose setup for dev/test  
- [x] GitHub Actions for CI/CD  
- [x] k6 benchmarks covering multiple scenarios  
- [x] Distributed tracing via OpenTelemetry and Jaeger  
- [x] Metrics endpoint for Prometheus (`/metrics`)  

**Testing & Internationalization:**  
- [x] Professional integration, stress, and recovery tests  
- [x] All code, scripts, templates, and docs in English  





## 🔮 Roadmap / Planned (v0.4.0+)

There's always more to build!  
Here's what's next for minikv:

- [ ] Cross-datacenter replication
- [ ] Admin web dashboard
- [ ] Advanced authentication and authorization
- [ ] S3-compatible API
- [ ] Multi-tenancy support
- [ ] Zero-copy I/O (io_uring support for ultrafast disk operations)
- [ ] Even more flexibility in configuration and deployment




## 🌱 The Story

Started after university: from basic Rust learning to building a distributed system.  
One month, countless lessons — and now a real repo serving real clusters.  
*"From zero to distributed in 31 days" — all code open in [whispem/minikv](https://github.com/whispem/minikv).*




## 📖 Documentation

- [CHANGELOG.md](CHANGELOG.md) — version history, roadmap
- [CONTRIBUTING.md](CONTRIBUTING.md) — how to join and contribute
- [TRACING.md](TRACING.md) — observability tips

**Why these choices?**
- Raft for understandable consensus
- 2PC for atomic distributed writes
- Coordinators for metadata, volumes for storage
- gRPC for fast node coordination
- HTTP REST for client ease-of-use





## 🔒 Enable TLS (HTTPS/Secure gRPC)

minikv supports network encryption (TLS) for both the HTTP API **and** internal gRPC.

### Generate self-signed certificates (demo/dev)

```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes -subj '/CN=localhost'
```

- Place `cert.pem` and `key.pem` in a directory of your choice (e.g., `./certs/`).

### Start a coordinator with TLS

```bash
./target/release/minikv-coord serve \
  --id coord-1 \
  --bind 0.0.0.0:5000 \
  --grpc 0.0.0.0:5001 \
  --db ./coord-data \
  --peers coord-2:5001,coord-3:5002 \
  --tls-cert ./certs/cert.pem \
  --tls-key ./certs/key.pem
```

- The HTTP API will be available over HTTPS (port 5000), and secure gRPC on 5001.
- For production, use certificates signed by a trusted authority.

### Client calls (curl example)

```bash
curl -k https://localhost:5000/my-key -o output.pdf
```

- The `-k` option disables certificate verification (useful for self-signed certs in dev).

### Notes
- Certificate/key paths are configurable via the config file, environment variables, or CLI.
- The cluster can run in mixed mode (with or without TLS) depending on file presence.
- gRPC (tonic) uses the same certificates as the HTTP API.

For more details, see the [Configuration](#configuration) section or the `config.toml` file.




## 🧑‍💻 Development

Fork, experiment, help shape minikv:

```bash
git clone https://github.com/whispem/minikv
cd minikv
cargo build --release
cargo test
cargo bench
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

Branch, experiment, contribute, or just hack on it!  
Everything you need is in the repo.




## 🤝 Contributing

All backgrounds, all levels welcome! Feedback, code, bug reports, docs — jump in.

- Open issues: [github.com/whispem/minikv/issues](https://github.com/whispem/minikv/issues)
- See [CONTRIBUTING.md](CONTRIBUTING.md) for more info




## 📜 License

MIT License — see [LICENSE](LICENSE)



## 🙏 Acknowledgments

Created by [@whispem](https://github.com/whispem) as a personal, learning-first journey.

Inspired by TiKV, etcd, and mini-redis.  
Guided by the Rust Book, Raft Paper, and the open-source community.


If you learn, experiment or just appreciate this project,  
consider starring [whispem/minikv](https://github.com/whispem/minikv)! ⭐


**Built with Rust — for anyone who loves learning & building.**  
*"From zero to distributed in 31 days."*



## 📬 Contact

- GitHub: [@whispem](https://github.com/whispem)
- Repo & Issues: [whispem/minikv](https://github.com/whispem/minikv/issues)


[Back to Top](#minikv-)
