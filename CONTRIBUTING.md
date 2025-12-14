# Contributing to minikv 🦀

Thank you for your interest in **minikv**! Contributions — bug reports, code, docs, or design feedback — are more than welcome.


## Quick Start

```bash
# Fork and clone
git clone https://github.com/whispem/minikv
cd minikv

# Build & test
cargo build --release
cargo test          # All unit & integration tests

# Format & lint
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```


## How to Contribute

- **Bug report**: Open an [issue](https://github.com/whispem/minikv/issues) with as much detail as possible (stacktrace, reproduction, expected/actual).
- **Feature request/discussion**: Open an issue or Pull Request (PR) — all proposals are welcome.
- **Code**: Fork, branch, commit, PR! See **Development Workflow** below.
- **Docs**: Spotted a typo, missing info, or want to improve clarity? PRs always welcome.


## Current Scope & Status

All **core distributed features** for v0.2.0 are complete and production-ready:

- Multi-node Raft consensus (leader election, replication, snapshot, recovery)
- Two-Phase Commit (2PC) protocol for distributed writes
- Automatic cluster rebalancing (load detection, blob migration, metadata update)
- WAL for durability; O(1) HashMap indexing, CRC32, quick crash recovery
- HTTP API, gRPC for cluster internals, CLI for ops (verify, repair, compact, rebalance)
- Prometheus `/metrics`, distributed tracing, k6 benchmark suites
- Docs, scripts, templates — all in English

**If something doesn’t work as documented, please open an issue**!


## What’s Next? (You can Help!)

The next major priorities for minikv (planned v0.3.0+) are:
- Range queries & batch API (multi-put/get/delete)
- Cross-datacenter replication / topology changes
- S3-compatible API & Multi-tenancy
- Security: TLS, authentication, advanced ACLs
- Chaos testing, perf tuning (io_uring, zero-copy), more resilience
- Admin dashboard (web), smoother upgrades/rolling ops

*Want to work on any of these? Let’s sync in an issue or discussion!*


## Development Workflow

1. **Branch**
    ```bash
    git checkout -b feature/my-feature
    ```
2. **Make changes** (with clear comments, error handling, and tests if possible)
3. **Test**
    ```bash
    cargo test
    ```
4. **Lint & format**
    ```bash
    cargo fmt --all
    cargo clippy --all-targets -- -D warnings
    ```
5. **Commit (Conventional commits encouraged)**
    ```bash
    git commit -m "feat(raft): improve vote timeout randomization"
    ```
6. **Push & Open PR!**


## Code Style

- **4-space indentation**
- **Comprehensive doc comments** (///, //!) and code comments (`// Why?`)
- **Pass all tests & lints**
- **Include or update relevant docs**


## Project Values

- **Simplicity first** (documentation, code, UX)
- **Welcoming, inclusive** — newcomers, experts, all backgrounds welcome!
- **Curiosity and learning**: Even PR drafts/tutorials/brainstorms matter.


## Need Help? Have Questions?

- Open an [issue](https://github.com/whispem/minikv/issues)
- Discussion tab (if enabled)
- Contact: [@whispem](https://github.com/whispem)


**Thanks again for helping build an awesome distributed system, open to anyone curious! 🚀**
