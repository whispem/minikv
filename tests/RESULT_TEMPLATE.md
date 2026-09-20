# Professional Test Report Template - minikv v2.0.0

Use this template to record results for each manual scenario execution.
Complete all sections to ensure traceability and reproducibility.

## General Information

- Date:
- Tester:
- Scenario ID and Name:
- Build/Version (`git rev-parse --short HEAD`):
- Environment (local, Docker, k8s):
- Cluster configuration (coordinator, volumes, replicas):

## Objective

- Objective:
- Scope:
- Preconditions:

## Steps Executed

1. 
2. 
3. 

## Commands Used

```bash
# Paste exact commands used during execution
```

## Verification Points

- 
- 
- 

## Observed Results

- 
- 
- 

## Metrics and Logs

- Metrics endpoint sample (`GET /metrics`):
- Health endpoints (`GET /health/live`, `GET /health/ready`):
- Coordinator logs:
- Volume logs:
- Additional evidence:

## Scenario Status

- [ ] Pass
- [ ] Fail
- Notes:

## Feature-Specific Checklist

### Kubernetes Operator and Cloud-Native

- [ ] CRD applied and recognized
- [ ] Operator reconciliation successful
- [ ] StatefulSet, Services, ConfigMaps, RBAC validated
- [ ] Scaling behavior verified

### Time-Series

- [ ] `POST /ts/write` validated
- [ ] `POST /ts/query` validated
- [ ] Aggregation/filter behavior verified

### Vector Search

- [ ] `POST /vector/upsert` validated
- [ ] `POST /vector/query` validated
- [ ] `GET /admin/vector/stats` validated
- [ ] Persistence across restart verified

### Geo-Partitioning

- [ ] Routing strategies validated
- [ ] Failover validated
- [ ] Geo-fencing validated (if configured)

### Data Tiering

- [ ] Tier transitions validated
- [ ] Readability after movement validated

### io_uring (Linux)

- [ ] io_uring path active when enabled
- [ ] Fallback path validated

### Reliability and Consistency

- [ ] Node failure recovery validated
- [ ] Split-brain resistance validated
- [ ] Consistency across replicas validated

### Operations and Security

- [ ] Compaction and repair safety validated
- [ ] Audit logging validated
- [ ] Persistent backend restart durability validated

### Watch and Subscribe

- [ ] `/watch/ws` validated
- [ ] `/watch/sse` validated
- [ ] Event payload and ordering validated

## Attachments

- Screenshots:
- Log extracts:
- Metrics snapshots:
- Additional artifacts:

## Final Notes

- Risks identified:
- Follow-up actions:
- Owner:
- Target date:
