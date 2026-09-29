# Backup and Restore Runbook

> **Not implemented in minikv 2.0.1.** Every backup and restore route answers `501 Not Implemented`, for example `{"error":"not implemented","feature":"backup","roadmap":"unscheduled"}`, and no release plans them yet. None of the steps below can run today: they describe the drill for when these routes exist.

## Goal

Verify that backups are usable and recovery is repeatable.

## Preconditions

- Coordinator is reachable on http://localhost:8080
- BACKUP_MANAGER is initialized by the running process

## 1. Create a full backup

```bash
curl -s -X POST http://localhost:8080/admin/backup \
  -H 'Content-Type: application/json' \
  -d '{"type":"full"}'
```

Expected: status started and a backup_id.

## 2. List backups

```bash
curl -s http://localhost:8080/admin/backups
```

Expected: backup_id is present in the backup list.

## 3. Restore backup

```bash
curl -s -X POST http://localhost:8080/admin/restore \
  -H 'Content-Type: application/json' \
  -d '{"backup_id":"<backup_id>","target_path":"./restore-drill"}'
```

Expected: status started and a restore_id.

## 4. Validate result

- Confirm restore directory exists and contains restored files.
- Compare key metadata count before and after if available.
- Track operation in audit logs and metrics.

## 5. Drill cadence

- Run at least once per sprint for staging.
- Run before each release candidate in production-like environment.

## Failure handling

- If backup manager is unavailable, abort release and investigate initialization.
- If restore fails checksum validation, do not reuse backup artifacts.
- Open incident if RTO exceeds SLO.
