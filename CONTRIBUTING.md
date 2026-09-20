# Contributing to minikv

Thanks for contributing. This guide is aligned with the v2.0.0 workflow.

## Ways to Contribute

- Report bugs with clear reproduction steps.
- Propose roadmap or UX improvements.
- Submit code, tests, docs, and runbook improvements.
- Improve observability, release engineering, and operator workflows.

Issues: https://github.com/whispem/minikv/issues

## Local Setup

Prerequisites:

- Rust 1.81 or newer
- The Protocol Buffers compiler (`protoc`): `brew install protobuf` on macOS, `sudo apt-get install protobuf-compiler` on Debian/Ubuntu
- A C/C++ toolchain, because RocksDB is built from source: Xcode Command Line Tools on macOS, `build-essential` and `clang` on Debian/Ubuntu

```bash
git clone https://github.com/whispem/minikv
cd minikv
make build
```

Recommended checks before opening a PR, in the same order as CI:

```bash
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
cargo test --release
```

The time-series integration tests expect a coordinator listening on port 8000, as in CI. Start one in a second terminal before `cargo test --release`, and stop it afterwards:

```bash
cargo run --release --bin minikv-coord -- serve --id 1
```

The `admin_status` test rewrites `config.toml`. Restore it before committing:

```bash
git restore config.toml
```

## Branch and Commit Workflow

1. Create a branch.

```bash
git checkout -b feat/short-description
```

2. Implement changes with tests.
3. Run formatting/lint/tests.
4. Update docs if behavior or APIs changed.
5. Commit with a clear message.

Examples:

- `feat(timeseries): add tag-filtered query validation`
- `fix(vector): persist index atomically`
- `docs(release): update preflight instructions`

## Pull Request Checklist

- Feature behavior is tested (unit and/or integration).
- `cargo fmt --all` is clean.
- `cargo clippy --all-targets --all-features -- -D warnings` passes.
- Relevant docs updated (`README.md`, `CHANGELOG.md`, runbooks).
- Any API/endpoint changes are documented with examples.

## Testing Expectations

Minimum for most PRs:

```bash
cargo test --lib
```

Changes to Raft, the coordinator, the volume servers or the read/write path must also pass the end-to-end cluster test. It starts 3 coordinators and 3 volumes, writes and reads through every node, kills the leader and restarts it:

```bash
cargo test --release --test distributed_cluster -- --nocapture
```

For API, storage, replication, or release-impacting changes, run:

```bash
make test
make release-preflight
```

For release-critical PRs, also run:

```bash
make release-preflight-full
```

## Documentation Requirements

Update docs when you change behavior in any of these areas:

- Public APIs/endpoints
- Operational procedures (backup/restore, observability, deployment)
- Release process
- Developer commands or workflows

Key files:

- `README.md`
- `CHANGELOG.md`
- `docs/ops-backup-restore.md`
- `docs/release-engineering-v2.0.0.md`

## Current Priority Areas

- Enforcing authentication, RBAC, quotas and encryption on the HTTP API
- Raft log compaction and snapshots
- Automatic re-replication when a volume is lost
- Write forwarding from followers to the leader
- Background compaction and cluster verify/repair tooling
- CDC integrations (Kafka Connect templates)
- Read replicas for analytics traffic
- Vector indexing acceleration (HNSW/PQ)
- PITR and disaster recovery hardening
- Multi-region failover automation

## Code Style

- Keep changes small and focused.
- Prefer explicit errors over hidden fallbacks.
- Write doc comments for non-trivial behavior.
- Preserve backward compatibility unless the PR clearly documents a breaking change.

## Community

- Be respectful and constructive.
- Assume good intent.
- Review code for correctness, maintainability, and operational risk.

Thanks for helping make minikv more reliable and more useful.