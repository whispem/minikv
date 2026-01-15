# Professional Test Scenarios – minikv v0.4.0 (Current Release)

This document describes manual test scenarios to validate the robustness, resilience, and consistency of the minikv cluster. Each scenario includes context, detailed steps, commands to execute, verification points, and success criteria.

---

## 1. Node Failure

**Context:** A volume or the coordinator crashes unexpectedly.

**Steps:**
1. Start a full cluster (coordinator + 3 volumes).
2. Insert 1000 keys distributed across all volumes.
3. Forcefully stop a volume (`docker stop minikv-volume-1`).
4. Check cluster availability via `/health` and `/metrics`.
5. Read keys (including some on the stopped volume).
6. Restart the volume (`docker start minikv-volume-1`).
7. Verify automatic recovery and data synchronization.

**Success Criteria:**
- The cluster remains available (read/write on other volumes).
- Keys on the stopped volume are inaccessible during the outage, but recover after restart.
- Metrics reflect the volume state (down/up).

---

## 2. Split-brain

**Context:** Network partition between two groups of nodes.

**Steps:**
1. Start the cluster.
2. Simulate a network partition (iptables or docker network disconnect) between the coordinator and one volume.
3. Attempt writes and reads on all volumes.
4. Observe Raft role (leader/follower) and replication.
5. Repair the partition.
6. Verify data convergence and Raft log consistency.

**Success Criteria:**
- No persistent split-brain (only one leader, no data divergence).
- Writes to the isolated volume are rejected or queued.
- After repair, the volume catches up with the log and data is consistent.

---

## 3. Recovery After Failure

**Context:** Coordinator or volume crash, then restart.

**Steps:**
1. Start the cluster, insert data.
2. Stop the coordinator (`docker stop minikv-coordinator`).
3. Attempt reads/writes (should fail).
4. Restart the coordinator.
5. Verify service recovery and data consistency.

**Success Criteria:**
- The coordinator resumes its leader or follower role.
- Data is intact and accessible.
- Metrics reflect recovery.

---

## 4. Stress Test (High Load)

**Context:** High read/write load on the cluster.

**Steps:**
1. Start the cluster.
2. Run the script `bench/run_all.sh` to generate high load.
3. Monitor `/metrics` for lag, latency, throughput.
4. Check for absence of errors or timeouts.

**Success Criteria:**
- The cluster sustains the load without crashing.
- Metrics show expected throughput and latency.
- No data or operation loss.

---

## 5. Consistency Verification

**Context:** Ensure all keys are replicated and consistent after operations.

**Steps:**
1. Insert keys with known values.
2. Read all keys on each volume (via API or CLI).
3. Compare values and Raft logs.

**Success Criteria:**
- All keys are present and identical on each volume.
- Raft logs are synchronized.

---

## 6. Recovery After Compaction/Repair

**Context:** Force a compaction or repair, then verify recovery.

**Steps:**
1. Start the cluster, insert data.
2. Call `/admin/compact` and `/admin/repair`.
3. Check availability and data consistency after each operation.

**Success Criteria:**
- The cluster remains available during and after the operation.
- Data is compacted/repaired with no loss.

---

## 7. Audit Logging

**Context:** All admin and sensitive actions are logged for compliance and traceability.

**Steps:**
1. Start the cluster with audit logging enabled (default).
2. Perform admin actions (create/revoke/delete API keys, change quotas, etc.).
3. Perform S3/data operations (PUT, GET, DELETE).
4. Download or view the audit log file and stdout output.

**Success Criteria:**
- All admin and data modification actions are logged with correct event type, actor, and details.
- Audit log file and stdout output are consistent and complete.

---

## 8. Persistent Storage Backend

**Context:** Data is stored in RocksDB or Sled instead of in-memory.

**Steps:**
1. Configure the cluster to use RocksDB or Sled backend in config.toml.
2. Start the cluster and insert data via S3 API.
3. Stop and restart the cluster.
4. Verify data persists across restarts.

**Success Criteria:**
- Data is durable and survives process restarts.
- No data loss or corruption.

---

## 9. Watch/Subscribe System

**Context:** Clients can subscribe to key or prefix changes and receive real-time notifications.

**Steps:**
1. Start the cluster with watch/subscribe enabled.
2. Open a WebSocket or SSE connection to `/admin/subscribe` or similar endpoint.
3. Perform key changes (PUT, DELETE) on subscribed keys/prefixes.
4. Observe notifications received by the client.

**Success Criteria:**
- Clients receive timely and accurate notifications for all relevant key changes.
- No missed or duplicate events.

---

## 10. Real-time Watch/Subscribe Notifications

**Context:** Clients subscribe to key change events via WebSocket or SSE endpoints.

**Steps:**
1. Start the cluster (coordinator + volumes).
2. Open a WebSocket connection to `/watch/ws` or an SSE connection to `/watch/sse`.
3. Perform PUT, DELETE, and REVOKE operations via the API.
4. Observe the events received by the client (should include event type, key, tenant, timestamp).

**Success Criteria:**
- Each key change triggers a real-time event to all subscribers.
- Events are correctly formatted and delivered via both WebSocket and SSE.
- No missed or duplicate events for single operations.

---

> These scenarios should be executed manually, with result logging and metrics capture for each step. They guarantee professional-grade validation for minikv v0.4.0.
