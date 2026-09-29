# Professional Test Report Template - minikv v2.0.1

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

### Time-Series

- [ ] `POST /ts/write` validated
- [ ] `POST /ts/query` validated
- [ ] Aggregation/filter behavior verified

### Vector Search

- [ ] `POST /vector/upsert` validated
- [ ] `POST /vector/query` validated
- [ ] `GET /admin/vector/stats` validated
- [ ] Persistence across restart verified

### Reliability and Consistency

- [ ] Node failure recovery validated
- [ ] Split-brain resistance validated
- [ ] Consistency across replicas validated

### Operations and Security

- [ ] Admin operations that are not implemented answer `501`
- [ ] Audit logging validated
- [ ] Volume restart: deleted keys stay deleted

### Watch and Subscribe

- [ ] `/watch/ws` validated
- [ ] `/watch/sse` validated
- [ ] Event payload and ordering validated

### Not Implemented in 2.0.1

Nothing to validate: the Kubernetes operator, geo routing, data tiering, io_uring, and the verify, repair, compact, scale, backup and restore operations.

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
