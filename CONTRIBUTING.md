# Contributing to minikv 🦀

Thank you for your interest in **minikv**! Contributions — bug reports, code, docs, or design & performance feedback — are always welcome.

---

## Quick Start

```bash
# Fork and clone
git clone https://github.com/whispem/minikv
cd minikv

# Build & test
cargo build --release
cargo test             # All unit & integration tests

# Format & lint (must pass before PR)
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

---

## How to Contribute

- **Bug report**: Open an [issue](https://github.com/whispem/minikv/issues) with detail (stacktrace, repro steps, expected/actual).
- **Feature request / UX / roadmap discussion**: Open an issue or new PR — all proposals are welcome.
- **Code**: Fork, branch, commit, PR! See workflow below.
- **Docs**: PRs on typos, missing info, or improved clarity always welcome.

---

## Scope & Current Status

All **core distributed features** for v0.5.0 are implemented and production-ready:

- Multi-node Raft consensus (leader, replication, snapshot, recovery)
- Two-Phase Commit (2PC) protocol for distributed atomic writes
- Automatic cluster rebalancing (shards, blob migration, metadata updates)
- WAL for durability; O(1) indexing, CRC32, fast crash recovery
- HTTP REST API, gRPC for internode, CLI for ops (verify, repair, compact, rebalance, batch, range)
- Range queries, batch multi-key operations
- TLS encryption for HTTP & gRPC
- Flexible config: file, env, CLI
- Prometheus `/metrics`, distributed tracing
- **Admin dashboard endpoint** (`/admin/status`) for cluster monitoring
- **S3-compatible API** (PUT/GET, in-memory demo)
- **TTL support** - Keys can expire automatically (**NEW in v0.5.0**)
- **LZ4 compression** - Transparent compression for large values (**NEW in v0.5.0**)
- **Rate limiting** - Token bucket with per-IP tracking (**NEW in v0.5.0**)
- **Kubernetes health probes** - `/health/ready`, `/health/live` (**NEW in v0.5.0**)
- **Enhanced metrics** - Histograms, per-endpoint stats (**NEW in v0.5.0**)
- **Request tracing** - Structured logging with request IDs (**NEW in v0.5.0**)
- Extensive automated tests & documentation

**If something doesn’t work as documented, please open an issue!**

---

## What’s Next? (Contributions welcome)

The next big priorities (v0.6.0+) include:

- **Authentication & Multi-tenancy** - API keys, JWT, namespace isolation
- **Encryption at rest** - Data encryption on disk
- **RBAC** - Role-based access control
- Cross-datacenter and multi-region replication
- Change Data Capture (CDC) - Stream of modifications
- Transactions - Multi-key atomic operations
- Secondary indexes - Query by metadata/tags
- Tiered storage - Hot/warm/cold data tiers
- Real admin web UI/dashboard

> *Interested in one of these, or have new ideas? Open a discussion or issue!*

---

## Development Workflow

1. **Branch**
   ```bash
   git checkout -b feature/my-feature
   ```
2. **Make changes** (clear code & comments, handle errors, add/update tests)
3. **Test**
   ```bash
   cargo test
   ```
4. **Lint & format**
   ```bash
   cargo fmt --all
   cargo clippy --all-targets -- -D warnings
   ```
5. **Commit** (Conventional commits encouraged)
   ```bash
   git commit -m "feat(api): add S3 object PUT endpoint"
   ```
6. **Push & open Pull Request** — fill the PR template and/or describe the fix/feature!

---

## Code Style

- **4-space indentation**
- **Comprehensive doc comments** (`///`, `//!`) and code comments (`// Why?`)
- **Pass all tests & lints**
- **Update or add relevant docs**
- **Prefer English for all code, comments, and docs**

---

## Project Values

- **Simplicity first** — clear documentation, code, and UX
- **Welcoming, inclusive** — newcomers, experts, and all backgrounds welcome!
- **Curiosity & learning** — PR drafts, tutorials, design brainstorms all valued

---

## Need Help? Have Questions?

- Open an [issue](https://github.com/whispem/minikv/issues)
- Discussion tab (if enabled)
- Contact: [@whispem](https://github.com/whispem)

---

**Thanks for helping build an open, robust distributed system — open to anyone curious! 🚀**
