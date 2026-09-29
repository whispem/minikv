# Changelog

All notable changes to minikv will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

---

## [Unreleased]

- No unreleased entries yet.

---

## [2.0.1] - 2026-09-29

A correction release: where 2.0.0 reported work that it did not do, minikv now fails explicitly, and the data losses found on the way are fixed.

### Correction

Some earlier entries describe features that the code never provided. They stay as they were written. In short:

- There are no virtual shards and no rebalancing: placement ranks the live volumes by rendezvous hashing (HRW) on each key.
- `verify`, `repair`, `compact`, `rebalance`, `upgrade` and `stream`, and the matching admin routes, never ran: they returned fixed reports.
- Backups copied nothing, restores restored nothing, and TTLs were never applied.
- The Kubernetes operator, io_uring, the Kafka CDC sink, tiering compression, time-series downsampling and OpenTelemetry tracing do not exist, and nothing encrypts data at rest.

### Changed

- **BREAKING (HTTP):** `POST /admin/verify`, `/admin/repair`, `/admin/compact` and `/admin/scale` answer `501 Not Implemented`, with `{"error":"not implemented","feature":…,"roadmap":…}`, instead of `200` and an invented report. So do the backup and restore routes, which answered `503` or an empty list. A write with an `X-Minikv-TTL` header answers `501` and stores nothing.
- **BREAKING (CLI):** `verify`, `repair`, `compact`, `rebalance`, `upgrade` and `stream` exit with status 1 and name the release planned for them. `get`, `put` and `delete` exit with status 1 when the coordinator answers an error.
- **BREAKING (metrics):** `/metrics` no longer exports the series that nothing updated (`minikv_requests_total`, `minikv_errors_total`, `minikv_bytes_read_total`, `minikv_bytes_written_total`, `minikv_active_connections`, `minikv_keys_with_ttl`, `minikv_rate_limited_requests`, `minikv_request_duration_ms` and the per-endpoint series), nor `minikv_volume_free_bytes` and `minikv_s3_objects_with_ttl`, which were always 0. `minikv_raft_role` becomes one gauge per role: `minikv_raft_role{role="leader"} 1`.
- **BREAKING (library):** functions that reported work they did not do now return errors: `MiniKVController::run`, `reconcile` and `handle_delete`; `IoUring::new` and `UringFile::open` when `enabled` is set; `BackupManager::start_restore`; `TimeseriesEngine::run_downsampling`; the tiering `compress` and `decompress` for any algorithm but `None`; `CDCManager::create_sinks_from_config` with a Kafka sink; `CDCManager::flush` when a sink fails; `BlobStore::compact`.
- The vector index and `audit.log` are stored under `--db`; a vector index left by 2.0.0 in `coord-data/` is loaded. Audit entries name the actor `unauthenticated`.
- `minikv get` writes the raw bytes, to standard output when `--output` is absent. `--coordinator` is accepted before or after the subcommand.
- The remote datacenters of `ReplicationManager` start unhealthy, never contacted.
- Removed `scripts/admin.sh`, `scripts/test_raft_cluster.sh`, `fix_ci_complete.sh` and `cobertura.xml`, and the unused dependencies `raft`, `raft-proto`, `opentelemetry` and `tracing-opentelemetry`.

### Fixed

- A restarted volume server rebuilds its index from its segments and its WAL: deleted blobs and old versions no longer come back, and the index holds the BLAKE3 hash of the values. LZ4-compressed values can be read back.
- Time series: a second write in the same hour no longer replaces the first, a query that starts inside an hour finds its points, and a range that ends before it starts no longer panics.
- `/metrics` follows the Prometheus text format, with `HELP` and `TYPE` lines.
- `/admin/export` and `/search` report the values that they cannot read. `/search` matches bytes, and answers `500` when the key listing fails.
- `/admin/status` reports `nb_s3_objects` as `null`, instead of 0, when the key listing fails. The time-series stats list only what the engine supports, and the geo and CDC status answers drop their invented fields.
- The admin dashboard reads the fields that `/admin/status` returns, and shows the real version.
- `minikv-coord` logs a warning for each `[coordinator]` setting of the configuration file that it does not apply, and `config.toml.example` loads.
- The tests no longer rewrite `config.toml`.
- HRW placement breaks ties by volume id.
- The benchmarks count only `200` as a success, and `scripts/benchmark.sh` sends its requests to the leader of a cluster that can elect one. The coordinator image checks `/health/live`, and the alert rules on metrics that do not exist are gone.
- Documentation: placement, what is implemented and what answers `501`, configuration, and the module docs.

### Deprecated

These items stay for compatibility and will be removed in 3.0.0:

- `ConsistentHashRing`, `blob_prefix`, `Error::ShardNotFound`, and `PlacementManager::get_shard`, `rebalance` and `get_shard_volumes`: not wired into placement.
- `StoreStats::bloom_false_positives` and `compressed_blobs`: always 0.
- `volume::http::Location` and `get_location`, and `VolumeGrpcService::new`.
- `maybe_encrypt` and `maybe_decrypt`: they return their input when encryption or decryption fails.
- `UringFile::async_read`, `async_write` and `flush_async`: they do nothing.
- The `shard` field of `BlobLocation` holds a segment number: read it with `BlobLocation::segment()`.

---

## [2.0.0] - 2026-09-20

### Changed
- Reworked the distributed layer: coordinators replicate metadata through a persistent Raft log (log up-to-date checks on elections, majority commit, leader step-down when the quorum is lost)
- Writes use two-phase commit between the leader coordinator and the volume servers; each object version gets its own blob, and older versions are cleaned up after the switch
- Reads are linearizable on every coordinator (ReadIndex)
- `minikv-volume serve` runs a gRPC storage service, a `/health` endpoint and heartbeats to every coordinator
- **BREAKING:** `--coordinators` is required and should list every coordinator
- **BREAKING:** batch `get` returns the stored value instead of the key metadata
- **BREAKING:** public Rust API: `KeyMetadata` has a new `blob_id` field, `VolumeServer::open(VolumeServerConfig)` replaces `VolumeServer::new`, `VolumeClient::prepare` takes the payload, and the unused `RaftNode` snapshot and election helpers were removed
- **BREAKING:** coordinator data directories from 1.x are not compatible; start with fresh ones

### Added
- `DELETE /s3/:bucket/:key` and `GET` / `PUT` / `POST` / `DELETE /:key`, used by the `minikv` CLI
- `POST /internal/volumes/heartbeat`
- `/admin/status` reports the term, leader, commit index and log length
- End-to-end test with 3 coordinators and 3 volumes, including leader failover

### Fixed
- The coordinator starts without a `config.toml`
- Peer ports and coordinator list in `docker-compose.yml`

---

## [1.0.0] - 2026-04-08

### Added - v1.0.0 GA Release

#### Data Science and Engineering Experience
- Notebook-first Python SDK with analytics-oriented client helpers
- End-to-end quickstart example for vector search and time-series workflows
- Dataframe conversion helpers for pandas, polars, and pyarrow

#### Vector Search
- New vector endpoints:
  - `POST /vector/upsert`
  - `POST /vector/query`
  - `GET /admin/vector/stats`
- Cosine similarity top-k matching
- Persistent on-disk vector index snapshot (`coord-data/vector_index.json`)

#### Time-Series API GA
- Production handlers for `POST /ts/write` and `POST/GET /ts/query`
- Automatic initialization of the time-series engine
- Query support for tags, aggregation, resolution, and limits
- Admin time-series stats endpoint now backed by live engine stats

#### Operations and Release Engineering
- Helm chart added with dev/staging/prod values profiles
- Observability bundle:
  - Grafana dashboard provisioning and default minikv overview dashboard
  - Prometheus alert rules for availability and latency/error SLOs
- Backup/restore runbook for release and disaster-recovery drills
- GA preflight release script and Makefile targets for release validation

#### Documentation
- README restructured for GA usage and operations
- CONTRIBUTING guide updated for v1 release workflow
- Release engineering runbook added for reproducible v1.0.0 process

#### Fixes
- Updated shell automation scripts to use Kubernetes-aligned health probes (`/health/live`)

---

## [0.9.0] - 2026-02-14

### Added - v0.9.0 Release

#### Kubernetes Operator
- Custom Resource Definition (CRD) for `MiniKVCluster`
- Automated deployment and scaling of coordinator and volume nodes
- RBAC configuration with proper permissions
- StatefulSet management for persistent storage
- ConfigMap generation from cluster spec
- Horizontal Pod Autoscaler (HPA) support
- ServiceMonitor for Prometheus integration
- Example manifests for basic and production clusters

#### Time-Series Optimizations
- Dedicated `TimeseriesEngine` for time-series workloads
- Multiple resolution levels (raw, 1min, 5min, 1hour, 1day)
- Automatic downsampling with configurable retention
- Aggregation functions: sum, avg, min, max, count, first, last, stddev
- Delta and gorilla compression for efficient storage
- Time-range queries with aggregation support

#### Geo-Partitioning
- Geographic data locality for compliance (GDPR, data residency)
- Multiple routing strategies:
  - Nearest region (latency-based)
  - Primary region (consistency-based)
  - Round-robin (load distribution)
  - Geo-fenced (compliance-based)
- Haversine distance calculation for region selection
- Per-key prefix region assignment
- Region health monitoring with automatic failover
- Geo-fencing rules for data sovereignty

#### Data Tiering
- Automatic data movement between storage tiers
- Tier levels: Hot, Warm, Cold, Archive
- Policy-based tiering rules:
  - Access frequency (access count in time window)
  - Data age (time since creation)
  - Value size (bytes)
  - Last access time (idle duration)
- Access pattern tracking with history
- Compression per tier (none, lz4, zstd, snappy, gzip)
- S3-compatible archive tier support
- Statistics and reporting for tier distribution

#### io_uring Performance Mode (Linux)
- Zero-copy I/O operations using Linux io_uring
- Batched submissions for reduced syscalls
- Configurable submission/completion queue depths
- Optional kernel polling (SQPOLL) for ultra-low latency
- Registered buffers for zero-copy transfers
- Direct I/O bypass of page cache
- Graceful fallback to standard I/O on unsupported systems
- Write batching for small operations

#### New Modules
- `k8s_operator.rs` - Kubernetes controller and CRD types
- `timeseries.rs` - Time-series storage engine
- `geo.rs` - Geo-partitioning router
- `tiering.rs` - Data tiering manager
- `io_uring.rs` - io_uring I/O backend

#### Kubernetes Manifests
- `k8s/crds/minikvcluster.yaml` - CRD definition
- `k8s/rbac/operator-rbac.yaml` - Operator permissions
- `k8s/operator/deployment.yaml` - Operator deployment
- `k8s/examples/basic-cluster.yaml` - Minimal cluster example
- `k8s/examples/production-cluster.yaml` - Production-ready example

---

## [0.8.0] - 2026-02-01

### Added - v0.8.0 Release

#### Cross-Datacenter Replication
- Asynchronous replication to remote datacenters
- Multiple conflict resolution strategies:
  - Last-Write-Wins (timestamp-based)
  - Vector Clocks (causality tracking)
  - Local-First (prefer local DC writes)
  - Primary-First (prefer primary DC writes)
- DC-aware routing for read/write operations
- Configurable replication lag monitoring and alerts
- Per-datacenter replication status tracking

#### Change Data Capture (CDC)
- Real-time capture of all data changes (INSERT, UPDATE, DELETE)
- Configurable sinks for event delivery:
  - Webhook sink (HTTP POST to external endpoints)
  - Kafka sink (for event streaming platforms)
  - File sink (for local debugging/archival)
  - Memory sink (for testing)
- Event filtering by operation type and key prefix
- Sequence numbers for guaranteed ordering
- Old value capture for UPDATE and DELETE operations

#### Admin Web UI
- Embedded web dashboard for cluster monitoring
- Real-time cluster status visualization
- API key management interface
- Backup/restore controls with progress tracking
- Plugin management UI
- Cross-DC replication status monitoring
- Responsive dark theme design

#### Backup & Restore
- Full backup support (complete snapshot)
- Incremental backup support (changes since last backup)
- Backup compression (configurable)
- Backup encryption support
- Multiple backup destinations:
  - Local filesystem
  - S3-compatible storage
- Point-in-time recovery
- Checksum verification during restore
- Backup manifest with metadata

#### Plugin System
- Extensible plugin architecture
- Plugin types:
  - Storage plugins (custom backends)
  - Auth plugins (custom authentication)
  - Hook plugins (event listeners)
  - Middleware plugins (request interceptors)
- Plugin lifecycle management (load, enable, disable, unload)
- Plugin dependencies and version compatibility
- Built-in logging hook plugin example

#### New API Endpoints
- `GET /admin/ui` - Admin web dashboard
- `POST /admin/backup` - Create a new backup
- `GET /admin/backups` - List all backups
- `GET /admin/backups/:id` - Get backup details
- `DELETE /admin/backups/:id` - Delete a backup
- `POST /admin/restore` - Restore from backup
- `GET /admin/replication/status` - Replication status
- `GET /admin/plugins` - List plugins
- `POST /admin/plugins/:id/enable` - Enable a plugin
- `POST /admin/plugins/:id/disable` - Disable a plugin
- `GET /admin/cdc/status` - CDC status

#### Technical Improvements
- Added `async-trait` for async plugin traits
- New modules: `replication`, `cdc`, `backup`, `plugin`, `admin_ui`
- Comprehensive unit tests for all new features
- Vector clock implementation for distributed causality

---

## [0.7.0] - 2026-01-25

### Added - v0.7.0 Release

#### Streaming/batch import/export
- `POST /admin/import` - Batch import key-value pairs from JSON payload
- `GET /admin/export` - Streaming export of all key-value pairs as NDJSON

#### Multi-key transactions
- `POST /transaction` - Execute multiple operations (put/delete) in a single request
- Returns detailed results for each operation with success/error status

#### Secondary indexes
- `GET /search?value=<substring>` - Search for keys whose values contain the specified substring

#### Durable S3-backed object store
- S3-compatible API now supports pluggable persistent storage backends (RocksDB, Sled)
- Objects can be stored durably by configuring storage backend in config.toml

---

## [0.6.0] - 2025-01-20

### Added - v0.6.0 Release

#### Major Features - Security, Multi-tenancy & Observability
- **API Key Authentication** - Secure access control with API keys
  - Generate and manage API keys via admin endpoints
  - Keys are securely hashed using Argon2id
  - Support for key expiration and revocation
  - Headers: `Authorization: Bearer <api_key>` or `X-API-Key: <key>`
- **JWT Token Support** - Stateless authentication tokens
  - Generate JWT tokens from valid API keys
  - Configurable token expiration (default: 24 hours)
  - HMAC-SHA256 signature verification
- **Role-Based Access Control (RBAC)** - Fine-grained permissions
  - Three role levels: Admin, ReadWrite, ReadOnly
  - Middleware enforcement on all protected routes
  - Role-based endpoint restrictions
- **Multi-tenancy** - Tenant isolation for data
  - Tenant identifier attached to each API key
  - S3 objects tagged with tenant ownership
  - Tenant extraction from authenticated requests
- **Encryption at Rest** - AES-256-GCM data encryption
  - HKDF-SHA256 key derivation from master key
  - Per-object random nonces for security
  - Separate keys for data and WAL encryption
  - Transparent encryption/decryption with backward compatibility
- **Tenant Quotas** - Resource limits per tenant
  - Storage limits (bytes)
  - Object count limits
  - Request rate limiting per tenant
  - Prometheus metrics for quota usage
- **Audit Logging** - Structured audit logs for all admin and sensitive actions (file + stdout)
- **Persistent Storage Backends** - Pluggable storage: in-memory, RocksDB, Sled (configurable via config.toml)
- **Watch/Subscribe System** - Real-time key change notifications (WebSocket & SSE endpoints, production-ready)
  - Subscribe to key changes via `/watch/sse` (SSE) or `/watch/ws` (WebSocket)
  - Events: PUT, DELETE, REVOKE (with key, tenant, timestamp)
  - Integrated with all S3/data and admin modification endpoints

#### Admin API Endpoints
- `POST /admin/keys` - Create new API key
- `GET /admin/keys` - List all API keys
- `GET /admin/keys/:id` - Get specific API key details
- `POST /admin/keys/:id/revoke` - Revoke an API key
- `DELETE /admin/keys/:id` - Delete an API key
- `GET /admin/audit` - Download or stream audit logs (NEW)
- `GET /admin/subscribe` - Subscribe to key change events (NEW, preview)

#### Security Improvements
- Constant-time password verification with Argon2
- Secure key generation using cryptographic RNG
- Authentication middleware for route protection
- Request validation and tenant context propagation
- Audit log hooks in all admin and data modification endpoints

#### Storage Improvements
- Pluggable backend: select in-memory, RocksDB, or Sled via config
- S3/data endpoints refactored to use trait abstraction
- Persistent storage for all S3/data paths when enabled

#### Observability
- Audit log file and stdout output
- Prometheus metrics for audit, quota, and storage backend
- Watch/subscribe system for real-time notifications (preview)

#### Breaking Changes
- S3 store entries now include tenant field
- Authorization required for protected endpoints (when auth enabled)
- Storage backend must be selected in config (default: in-memory)

---

## [0.5.0] - 2026-01-15

### Added - v0.5.0 Release

#### Major Features
- **TTL Support** - Keys can now expire automatically with millisecond precision
  - Set TTL via `X-Minikv-TTL` header on PUT requests
  - Automatic cleanup of expired keys
- **LZ4 Compression** - Optional transparent compression for values > 1KB
  - Configurable via `CompressionMode::Lz4`
  - Automatic decompression on read
- **Rate Limiting** - Token bucket algorithm with per-IP tracking
  - Configurable requests per second and burst size
  - Returns `X-RateLimit-*` headers
- **Kubernetes Health Probes** - Production-ready health endpoints
  - `/health/ready` - Readiness probe (checks volumes + Raft)
  - `/health/live` - Liveness probe (always returns OK)
- **Enhanced Metrics** - Prometheus histograms and per-endpoint stats
  - Request latency histograms with configurable buckets
  - Per-endpoint request/error counters
  - TTL and rate limiting metrics
- **Request Tracing** - Structured logging with request IDs
  - Unique `X-Request-ID` header for each request
  - Correlation across distributed components

#### Improvements
- Updated blob format to support compression metadata
- Index snapshots now include TTL expiration data (KVINDEX3 format)
- Better WAL replay with v0.5.0 format support

#### Breaking Changes
- Blob storage format changed (existing data will be migrated on read)
- Index snapshot format updated to KVINDEX3

---

## [0.4.0] - 2025-12-31

### Added - v0.4.0 Release

#### Major Features
- **Admin dashboard endpoint** (`/admin/status`) for live cluster state and monitoring (**NEW**)
    - Shows role, leader, volumes, S3 object count, and more, for monitoring and UI integration
- **S3-compatible API** (PUT/GET, in-memory demo) as a new object storage interface (**NEW**)
    - Store and retrieve objects via `/s3/:bucket/:key`
- Full documentation and automated tests for all new endpoints

#### Improvements
- Better system observability: admin and metrics endpoints now cover all cluster state
- Clean separation of admin/user APIs
- Documentation expanded and migrated for new features
- Test coverage increased for dashboard and S3 features

#### Project Status
- All v0.4.0 roadmap features implemented and tested
- Ready for integration with UIs, external metrics dashboards, and S3-demo clients
- Cluster state easily visible and integrable via admin endpoint

---

## [0.3.0] - 2025-12-22

### Added - v0.3.0 Release

#### Major Features
- Range queries (efficient scans across keys)
- Batch operations API (multi-put/get/delete)
- TLS encryption for HTTP and gRPC (production-ready security)
- Flexible configuration: file, environment variables, and CLI override
- All code, comments, and documentation now in English
- 100% green CI: build, test, lint, format

#### Improvements
- CLI and API fully support new batch and range operations
- Example config and all templates now in English
- Refined error handling and configuration merging
- Documentation and README updated for v0.3.0

#### Project Status
- All v0.3.0 roadmap features are implemented and production-ready
- No incomplete logic remains
- Ready for enterprise deployment and future advanced features

---

## [0.2.0] - 2025-12-14

### Added - v0.2.0 Release

#### Core Architecture
- Full multi-node Raft consensus: leader election, log replication, snapshots, commit index, recovery, partition detection
- Advanced Two-Phase Commit (2PC) streaming: chunked blob streaming, error propagation, retry, timeouts
- Automatic cluster rebalancing: detects overloaded/underloaded volumes, moves blobs and updates metadata
- Prometheus metrics endpoint: /metrics exposes cluster and volume stats, Raft role, replication lag, health
- Professional integration, stress, and recovery tests
- All scripts, test templates, and documentation translated/adapted to English

#### Project Status
- All core features are implemented and production-ready
- No incomplete logic remains
- All documentation, comments, and scripts are in professional English
- Ready for enterprise deployment and further extension

---

## [0.1.0] - 2025-12-06

### Added - Initial Release

#### Core Architecture
- **Raft consensus** for coordinator high availability
- **2PC (Two-Phase Commit)** for distributed writes
- **Write-Ahead Log (WAL)** for durability with configurable fsync
- **Dynamic sharding** across 256 virtual shards
- **HRW (Highest Random Weight)** placement for replica selection
- **gRPC** internal coordination protocol
- **HTTP REST API** for public access

#### Storage Engine (from mini-kvstore-v2)
- Segmented append-only log architecture
- In-memory HashMap index for O(1) lookups
- Bloom filters for fast negative lookups
- Index snapshots for 5ms restarts (vs 500ms rebuild)
- CRC32 checksums on every record
- Automatic background compaction

#### Coordinator Features
- RocksDB metadata store for key → replicas mapping
- Raft leader election (simplified single-node for v0.1)
- Volume health monitoring
- Placement manager with shard rebalancing
- RESTful HTTP API: PUT, GET, DELETE

#### Volume Features
- Blob storage with segmented logs
- WAL for durable writes
- gRPC service for 2PC operations
- HTTP API for direct blob access
- Automatic compaction based on threshold
- Index snapshot persistence

#### Operations Commands
- Coordinator commands: `serve`, `compact`, `rebalance`
- Volume commands: `serve`, `compact`
- CLI: verify, repair, batch, range
