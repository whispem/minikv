# Changelog

All notable changes to minikv will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
- No TODOs, stubs, or incomplete logic remain
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
- No stubs, TODOs, or incomplete logic remain
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
