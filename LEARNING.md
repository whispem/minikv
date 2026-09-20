# Learning Rust: My Journey from Literature to Distributed Systems

*Hi, I’m Emilie (Em'). This document summarizes how I moved from literature and languages to systems programming with Rust, then into formal data science studies, starting almost from scratch.*

---

## Where I Come From

Before 2025, my world was literature, linguistics, and foreign languages, not computers.  
When I first saw code, it looked like a tangle of acronyms and semicolons.

Curiosity pushed me to give programming a serious try.

---

## Early 2025: The Apple Foundation Program

**January/February 2025: I joined the Apple Foundation Program (AFP).**  
My first real encounter with code: Swift, UI/UX, Xcode, iOS apps.  
Everything felt visual and concrete: constructing, testing, deploying.  
Variables, loops, and functions started to make sense, like learning a new language to build things rather than only analyze them.

Most importantly, I realized I could learn to code.

---

## Spring–Summer 2025: Learning on My Own

After the AFP, I kept building Swift projects on my own.  
Bit by bit, the basics became clearer.  
One question kept coming back: *What really happens behind the scenes?*  
What do computers do with memory and files? How do real systems work?

---

## Autumn 2025: The Leap Into Rust

**Timeline:**
- **Started Rust:** October 27, 2025 (at 00:27 UTC+1 I ran my first "Hello World" in Rust)
- **Shipped mini-kvstore-v2:** November 21, 2025
- **Released minikv (distributed):** December 2025 (`v0.3.0` on December 22, then `v0.4.0` on December 31 with the first real admin dashboard and S3 API)
- **Founded Rust Aix-Marseille (RAM):** December 2025, with our first online meetup on January 14, 2026
- **Started Whispem, my own programming language:** January 28, 2026
- **Started Data Science program at AMSE:** April 2, 2026 (Aix-Marseille School of Economics)
- **Released minikv v1.0.0:** April 8, 2026
- **Earned my DESU in Data Science at AMSE:** summer 2026
- **Released minikv v2.0.0:** September 2026, with a reworked distributed layer

After hearing:
- “Rust is way too hard.”
- “Beware the borrow checker!”
- “It’s not for beginners.”

I had no formal tech background, but I wanted to understand how systems worked and challenge myself with low-level code.

---

## First Impressions

- **The compiler is strict but a true teacher:** error messages are detailed, sometimes even confessional—pointing to a solution.
- **Ownership and borrowing:** I thought I got “ownership” from literature, but Rust forces you to *internalize* it.
- **Everything’s explicit:** Who owns what, who can change or just borrow, and for how long.
- **The Rust community:** Genuinely welcoming, even to beginners.

---

## What Helped Me Along the Way

- **The Rust Book:** Everyone says it, because it’s true (especially Chapter 4—ownership!).
- **Clippy:** My favorite code reviewer, even when it stings.
- **Keeping notes:** Writing down every concept, compiler message, and solution helped me not get overwhelmed.
- **Building side projects:** Practice drives progress, including failed attempts.

---

## My Non-Tech Background: Actually an Advantage

- Loops, structure, types… remind me of literary analysis—except here it’s the machine that reads.
- Close reading (“is this reference mutable or immutable?”) and not skipping details—skills that transferred perfectly.
- Patience with ambiguity, digging deep until understanding—the same in both worlds.
- UI/UX taught me to design for people. Rust taught me to design for people *and* computers.

---

## What I Wish I Had Known Earlier

- *You don’t need to be “technical” to start.* Curiosity is the real prerequisite.
- *Don’t optimize too soon:* get it working, then get it right.
- *Testing can’t be too early.*
- *Learning isn’t linear.* There are setbacks and victories. Stick with it!

---

## Practical Tips

1. **Start before you feel “ready”**—you only get ready by doing.
2. **Read error messages like you’d read between the lines of a text**—all the clues are there.
3. **Celebrate every small win**—your first compiling program matters.
4. **Don’t be afraid to ask for help** (Discord, Reddit, Rust forums, etc.).
5. **Keep it enjoyable**: consistency is easier when you like the process.

---

## About minikv: What It Can Do (as of v2.0.0)

**Distributed Core:**
- Multi-node Raft consensus: leader election with log up-to-date checks, persistent log and term, majority commit, leader step-down when the quorum is lost
- Linearizable reads on every coordinator (ReadIndex)
- Two-Phase Commit (2PC) between the leader and the volume servers: size and BLAKE3 checks at prepare, commit or rollback on every replica
- Configurable N-way replication (default: 3 replicas), with one blob per object version
- Highest Random Weight (HRW) placement across volume servers
- Automatic failover: the remaining coordinators elect a new leader within about a second
- Volume servers that register themselves through heartbeats
- Range queries and batch operations
- TLS encryption for HTTP and gRPC
- Flexible configuration: file, env, CLI override
- Admin status endpoint (`/admin/status`): role, term, leader, commit index, volumes
- S3-compatible API (PUT/GET/DELETE)
- Watch/subscribe system (WebSocket and SSE) for real-time key change notifications

**Time Series and Vectors:**
- Time-series write and query APIs for event and metric workloads
- Query-time filtering and aggregation for analytical use cases
- Vector upsert and similarity query endpoints (top-k)
- Persistent vector index on coordinator disk for restart durability

**Storage Engine:**
- Segmented, append-only log structure
- In-memory HashMap indexing for O(1) key lookups
- Bloom filters for fast negative queries
- Index snapshots
- CRC32 checksums on every record
- Compaction with space reclamation
- RocksDB for coordinator metadata

**Security & Multi-Tenancy building blocks** (implemented and tested as modules; enforcement on the HTTP API is on the roadmap):
- API Key authentication (Argon2)
- JWT token support
- Role-Based Access Control (Admin/ReadWrite/ReadOnly)
- AES-256-GCM encryption
- Per-tenant quotas (storage, objects, rate limits)
- Audit logging for admin operations

**Durability:**
- Write-Ahead Log (WAL) for safety
- Configurable fsync policy (always, interval, never)
- Fast crash recovery via WAL replay
- Raft log and term persisted with CRC32 checksums and fsync

**APIs:**
- gRPC for internal communication (Raft between coordinators, 2PC with volumes)
- HTTP REST API for clients
- CLI for put/get/delete
- WebSocket & SSE endpoints for real-time notifications

**Infrastructure and Operations:**
- Docker Compose setup for dev/test
- Helm chart with dev/staging/prod profiles
- GitHub Actions for CI/CD
- k6 benchmarks for real scenarios
- Distributed tracing via OpenTelemetry & Jaeger
- Prometheus metrics endpoint (`/metrics`) and alert rules
- Grafana dashboards for cluster visibility
- Backup and restore runbook for operations

**Testing and Quality:**
- End-to-end cluster test: 3 coordinators and 3 volumes, leader failover and restart
- Unit tests for Raft (votes, log conflicts, persistence), the 2PC staging, and placement
- Integration, stress, and recovery tests
- Release preflight checks (fmt, clippy, build, tests)
- All code, scripts, and docs in English

---

## Beyond minikv

- **Whispem:** a programming language whose compiler is written in Whispem and recompiles itself to a fixed point, running on a standalone C VM
- **learn-assembly-with-em:** x86-64 assembly from a 512-byte boot sector to miniasm, a small assembler that matches NASM's output on its subset
- **asm.fm:** a synthesizer written in pure x86-64 assembly, from raw waveforms to FM bells and a resonant filter
- **sussurro.cpp:** offline neural translation (English, Spanish, French, Italian) in C++ on ggml
- **dprism:** a data explorer that lives in the terminal

---

## Milestones & Accomplishments

- Learned the fundamentals of Rust: ownership, lifetimes, async/await
- Built a distributed storage engine with Raft, WAL, and 2PC
- Added API Key/JWT authentication, RBAC, quotas, and audit logging building blocks
- Implemented a real-time notification system (watch/subscribe via WebSocket and SSE) for key changes
- Added time-series APIs and vector similarity search
- Reached v1.0.0 with release engineering checks and updated documentation
- Reached v2.0.0: persistent Raft, 2PC with volume servers, linearizable reads, and an end-to-end failover test
- minikv passed 400 stars on GitHub
- Founded Rust Aix-Marseille (RAM), a Rust community open to every level
- Gave my first talk at Epitech Marseille, about my journey into tech and Rust
- minikv was featured in Programmez!, followed by a live talk at the Programmez! Meetup and a guest editorial
- Built Whispem, my own programming language
- Started a Data Science program at AMSE (Aix-Marseille School of Economics) on April 2, 2026, and earned my DESU in Data Science in summer 2026

---

## My Takeaway

> “If you can read and express an idea, you can code. Patience, curiosity, and a love of learning are everything!”

*Written by Em' (@whispem), Rust developer, learning by building, including distributed key-value systems.*

*"Structure determines meaning. You learn by writing and by building."*