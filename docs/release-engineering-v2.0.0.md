# Release Engineering - v2.0.0

## Objective

Ship v2.0.0 with the same checks CI runs, then publish it to crates.io.

## 1. Versioning

- Set the Cargo version to 2.0.0. The distributed layer has breaking changes.
- Refresh `Cargo.lock` (the root package version changes).
- Add the v2.0.0 entry to `CHANGELOG.md`.

## 2. Toolchain

CI builds with the latest stable Rust. Match it locally first, so a new lint does not show up only on GitHub:

```bash
rustup update stable
rustc --version
```

## 3. Preflight Checks

Same checks as `.github/workflows/ci.yml`, in the same order:

```bash
make ci
```

It runs `cargo fmt --all -- --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build --release`, and the tests with a coordinator listening on port 8000.

End-to-end cluster test (3 coordinators, 3 volumes, leader failover):

```bash
make smoke
```

## 4. API Smoke Validation

Start a local cluster with `make serve`, then check:

- Coordinator liveness: `GET /health/live`
- Coordinator readiness: `GET /health/ready`
- Cluster state (role, term, leader, commit index, live volumes): `GET /admin/status`
- Write on the leader, read from every coordinator: `PUT /s3/<bucket>/<key>`, `GET /s3/<bucket>/<key>`
- A write sent to a follower answers `503` with an `x-minikv-leader` header
- Time-series write/query: `POST /ts/write`, `POST /ts/query`
- Vector index: `POST /vector/upsert`, `POST /vector/query`, `GET /admin/vector/stats`
- Metrics endpoint: `GET /metrics`

## 5. Operational Readiness

- Validate observability stack startup under `opentelemetry/`.
- Validate alert rules are loaded in Prometheus.
- Validate Grafana dashboard provisioning.
- Run the backup/restore drill following the runbook: `docs/ops-backup-restore.md`

## 6. Release Artifacts

```bash
cargo build --release
```

Optional Docker build:

```bash
make docker-build
```

## 7. Tag and Publish

The `admin_status` test rewrites `config.toml`, so restore it first. The working tree must be clean, otherwise `cargo publish` refuses to run.

```bash
git restore config.toml
git status
git tag v2.0.0
git push origin v2.0.0
cargo publish --dry-run
cargo publish
```

Tag only once CI is green on the release commit. Publishing is final: a version can be yanked, never deleted.

## 8. Go/No-Go Criteria

Go:
- `make ci` and `make smoke` green on the latest stable Rust
- CI green on the release commit
- critical smoke endpoints green
- backup/restore drill green
- no critical open bugs

No-Go:
- failing fmt, clippy, build or tests
- data-loss or restore failure
- failing readiness/liveness under normal operation
- a leader failover that does not converge in the cluster test