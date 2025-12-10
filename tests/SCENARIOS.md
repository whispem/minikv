# Professional Test Scenarios – minikv v0.2.0 (Current Release)

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
1. Démarrer le cluster, insérer des données.
2. Appeler `/admin/compact` et `/admin/repair`.
3. Vérifier la disponibilité et la cohérence des données après chaque opération.

**Critères de succès :**
- Le cluster reste disponible pendant et après l’opération.
- Les données sont compactées/réparées sans perte.

---

> These scenarios should be executed manually, with result logging and metrics capture for each step. They guarantee professional-grade validation for minikv v0.2.0.
