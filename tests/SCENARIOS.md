# Test Scenarios - minikv v2.0.0

This document defines manual validation scenarios for minikv v2.0.0. Each scenario gives its context, its steps, its success criteria, and the automated test that covers it when there is one.

Unless a scenario says otherwise, start a local cluster with:

```bash
make serve
```

It runs 3 coordinators (HTTP on 5000, 5002, 5004) and 3 volumes (HTTP on 6000, 6002, 6004). On a coordinator, `GET /admin/status` reports its role, term, leader, commit index, applied index, log length, and the volumes it considers live.

The whole distributed path replays in one command:

```bash
make smoke
```

## 1. Distributed Write Path

Automated: `tests/distributed_cluster.rs`, with real processes.

Context: a write goes through the leader, reaches every replica, and becomes visible on every coordinator.

Steps:
1. Read `GET /admin/status` on 5000, 5002 and 5004 to find the leader.
2. `PUT /s3/demo/hello.txt` on the leader.
3. Read `GET /health` on the three volumes and compare `total_keys`.
4. `GET /s3/demo/hello.txt` on each coordinator.

Success criteria:
- The write answers `200` on the leader.
- The object lands on 3 volumes.
- Every coordinator returns the value right after the write, with no retry.

## 2. Leader Redirect

Automated: `tests/distributed_cluster.rs`.

Context: a follower refuses writes and names the leader.

Steps:
1. Send `PUT /s3/demo/hello.txt` to a coordinator that is not the leader.
2. Read the status code and the `x-minikv-leader` header.
3. Check the volumes' `total_keys`.

Success criteria:
- The follower answers `503`.
- The header carries the leader's address.
- Nothing was written on the volumes.

## 3. Leader Failover

Automated: `tests/distributed_cluster.rs`.

Context: the cluster elects a new leader when the current one disappears.

Steps:
1. Note the current term with `GET /admin/status`.
2. Kill the leader: `lsof -ti tcp:<leader port> -sTCP:LISTEN | xargs kill`.
3. Poll `GET /admin/status` on the two remaining coordinators.
4. Write a new object on the new leader, then read it back.

Success criteria:
- A new leader appears in about a second, with a higher term.
- Writes work again on the new leader.
- Objects written before the failure are still readable.

## 4. Coordinator Restart and Catch-Up

Automated: `tests/distributed_cluster.rs`.

Context: a coordinator that was down rejoins and replays what it missed.

Steps:
1. Stop one coordinator, then write two or three objects through the leader.
2. Restart the stopped coordinator with the same `--db` directory.
3. Poll its `GET /admin/status` until `last_applied` matches the leader's `commit_index`.
4. Read the objects written during the outage from that coordinator.

Success criteria:
- The restarted node recovers its term and its log from disk.
- It catches up without a manual step.
- Its reads return the same values as the leader's.

## 5. Overwrite and Blob Cleanup

Automated: `tests/distributed_cluster.rs`.

Context: each version of an object gets its own blob, and the old one is removed.

Steps:
1. `PUT` an object, then read `total_keys` on each volume.
2. `PUT` the same key with a different body.
3. Read the object from every coordinator.
4. Read `total_keys` again on each volume.

Success criteria:
- Every coordinator returns the new value.
- The key count comes back to its previous level once the old blob is deleted.

## 6. Delete Propagation

Automated: `tests/distributed_cluster.rs`.

Context: a delete is replicated and the blobs are removed.

Steps:
1. `DELETE /s3/demo/hello.txt` on the leader.
2. `GET` the same key on all three coordinators.
3. Check `total_keys` on the volumes.

Success criteria:
- The delete answers `204`.
- Every coordinator answers `404`.
- The blobs are gone from the volumes.

## 7. Volume Loss and Degraded Reads

Automated: `tests/distributed_cluster.rs` (it kills 2 of the 3 volumes).

Context: reads survive as long as one replica holds the blob.

Steps:
1. Write an object with 3 replicas.
2. Stop one volume, then a second one.
3. Read the object from every coordinator.
4. Read `GET /admin/status`: the dead volumes leave the live list after 5 seconds without a heartbeat.
5. Restart the volumes.

Success criteria:
- Reads keep working on the surviving replica, with the BLAKE3 hash checked.
- The status endpoint reflects the live volumes.
- A restarted volume registers itself again through its heartbeat.

Known limitation: minikv does not re-replicate a lost blob. Re-replication is on the v2.1.0 roadmap.

## 8. Raft State Machine

Automated: unit tests in `src/coordinator/raft_node.rs` and `src/coordinator/raft_storage.rs`, plus `tests/raft_cluster.rs`, `tests/node_failure.rs` and `tests/split_brain.rs`. These four run in a single process, without the network: they exercise the state machine, not a live cluster. A live cluster is `tests/distributed_cluster.rs`.

Context: votes, log conflicts and persistence behave as the Raft paper describes.

Steps:
1. `cargo test --lib coordinator::raft`
2. `cargo test --test raft_cluster --test node_failure --test split_brain`

Success criteria:
- A vote is refused to a candidate whose log is behind.
- A conflicting entry is replaced and applied once.
- A follower refuses stale terms and gaps.
- A reopened log recovers its term and drops a torn tail.

## 9. Durability After a Crash

Automated: `tests/recovery.rs` and `tests/integration.rs`, at the storage-engine level.

Context: a volume recovers its data after an abrupt stop.

Steps:
1. Write keys through a `BlobStore`, or through the cluster.
2. Stop the volume without a clean shutdown.
3. Restart it and read the keys back.

Success criteria:
- The WAL is replayed and the keys are readable.
- CRC32 checks pass on every record.
- A partially written record at the end of the log is dropped rather than read.

Known limitation: a key deleted before a restart comes back in the volume's index. Versioned blobs keep this invisible to clients, but it wastes space.

## 10. S3-Compatible API

Automated: `tests/s3_api.rs` and `tests/s3_api_extra.rs`, with real processes.

Context: the S3 routes behave like the plain key routes.

Steps:
1. `PUT /s3/<bucket>/<key>` with a binary body.
2. `GET` it back and compare the bytes.
3. `GET` a key that does not exist.
4. Overwrite a key, then write several objects in the same bucket.

Success criteria:
- The bytes come back unchanged.
- A missing key answers `404`.
- Overwrites and multiple objects behave as expected.

## 11. Time-Series Engine

Automated: `tests/timeseries_integration.rs`, against a coordinator on port 8000.

Context: ingest and query workflows.

Steps:
1. Write samples with `POST /ts/write`.
2. Query with `POST /ts/query`, with filters and a time window.
3. Check aggregation and downsampling behavior.
4. Read `GET /admin/timeseries/stats`.

Success criteria:
- Samples are persisted and queryable.
- Filters and aggregations return the expected values.

## 12. Vector Similarity Search

Manual.

Context: vector indexing and nearest-neighbor retrieval.

Steps:
1. Upsert vectors with `POST /vector/upsert`.
2. Query neighbors with `POST /vector/query`.
3. Check `GET /admin/vector/stats`.
4. Restart the coordinator and query again.

Success criteria:
- Upserted vectors are returned by similarity queries.
- `top_k` is respected.
- The index is still usable after a restart.

## 13. Watch and Subscribe Notifications

Manual.

Context: real-time change propagation.

Steps:
1. Open a WebSocket subscription (`/watch/ws`) and an SSE one (`/watch/sse`).
2. Trigger put and delete operations.
3. Check the payload format and the ordering.

Success criteria:
- Subscribers receive the expected events promptly.
- No systematic duplicates and no missed events.

## 14. Load

Automated: `tests/stress.rs`, at the storage-engine level (1000 writes and reads in one process).

Context: sustained load behavior.

Steps:
1. `cargo test --release --test stress`
2. For HTTP load, start a cluster and watch `GET /metrics` while a client writes and reads.

Success criteria:
- No crash loop, no unbounded memory growth.
- Latency and error counters stay stable in `/metrics`.

Known limitation: the k6 scenarios under `bench/` were written for the v1 API. They do not look for the leader and they count a `501` as a success, so they need an update before their numbers mean anything.

## 15. Kubernetes Deployment

Manual, manifests only.

Context: deploy a cluster from the manifests under `k8s/`.

Steps:
1. Apply the CRD and the examples under `k8s/examples/`.
2. Deploy the Helm chart from `k8s/helm/minikv/` with the dev values.
3. Check Services, ConfigMaps, RBAC and the monitoring resources.
4. Scale the volume workload up and down.

Success criteria:
- Pods reach a ready state and coordinators elect a leader.
- Volumes register themselves with every coordinator.

Known limitation: no operator process is shipped. `MiniKVClusterSpec` and `MiniKVController` in `src/common/k8s_operator.rs` are a modeled reconciliation loop with unit tests, not a running controller, so nothing reconciles the CRD on its own.

## Modules Not Yet Enforced on the HTTP API

These are implemented and covered by their own unit tests, but the HTTP API does not apply them yet. Validate them with `cargo test --lib` rather than through the API, and treat any end-to-end scenario for them as a v2.1.0 item:

- Authentication (API keys with Argon2, JWT) and RBAC
- Tenant quotas and request rate limiting
- AES-256-GCM encryption
- Audit logging
- Geo-partitioning: `GET /admin/geo/status` answers `enabled: false`
- Data tiering and io_uring
- `POST /admin/compact`, `POST /admin/repair` and `POST /admin/verify`, which call the placeholder tooling in `src/ops/`

## Execution Notes

- Record the commands, the timestamps and the environment.
- Capture logs (`./data/*.log` when the cluster comes from `make serve`) and `/metrics` output for any failure.
- Store outcomes in `tests/RESULT_TEMPLATE.md`.
- The `admin_status` test rewrites `config.toml`. Run `git restore config.toml` afterwards.