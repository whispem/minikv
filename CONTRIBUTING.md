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

All **core distributed features** for v0.4.0 are implemented and production-ready:

- Multi-node Raft consensus (leader, replication, snapshot, recovery)
- Two-Phase Commit (2PC) protocol for distributed atomic writes
- Automatic cluster rebalancing (shards, blob migration, metadata updates)
- WAL for durability; O(1) indexing, CRC32, fast crash recovery
- HTTP REST API, gRPC for internode, CLI for ops (verify, repair, compact, rebalance, batch, range)
- Range queries, batch multi-key operations
- TLS encryption for HTTP & gRPC
- Flexible config: file, env, CLI
- Prometheus `/metrics`, distributed tracing
- **Admin dashboard endpoint** (`/admin/status`) for cluster monitoring (**NEW in v0.4.0**)
- **S3-compatible API** (PUT/GET, in-memory demo) (**NEW in v0.4.0**)
- Extensive automated tests & documentation

**If something doesn’t work as documented, please open an issue!**

---

## What’s Next? (Contributions welcome)

The next big priorities (v0.5.0+) include:

- Persistent on-disk storage backends (e.g. RocksDB, Sled)
- Cross-datacenter and multi-region replication
- Durable S3-backed object store
- Advanced authentication & access control
- Real admin web UI/dashboard
- Chaos & resilience tooling, perf (io_uring/zero-copy), enhanced deployment options

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
