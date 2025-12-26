# Learning Rust: My Journey from Literature to Distributed Systems

*Hi, I’m Emilie (but everyone calls me Em’). 
Here’s how I went from literature and languages to building real systems with Rust, starting (almost) from scratch.*



## Where I Come From

Before 2025, my world was literature, linguistics, and foreign languages—not computers. 
When I saw “code” it looked like mysterious acronyms and a tangle of semicolons!

But curiosity (and maybe a hint of madness) pushed me to give programming a real shot.



## Early 2025: The Apple Foundation Program

**January/February 2025: I joined the Apple Foundation Program (AFP).**  
My first real encounter with code: Swift, UI/UX, Xcode, iOS apps. 
Everything felt visual and playful—constructing, clicking, deploying.  
Suddenly, variables, loops, functions started to make sense, almost like learning a new natural language for making things instead of just analyzing them.

But most importantly: *I realized I could actually learn to code*.



## Spring–Summer 2025: Learning on My Own

After the AFP, I kept tinkering with Swift projects on my own—just for fun, but really leveling up my logic and creativity.  
Bit by bit, the basics settled in, and I chased the thrill of “it works!”  
Still, one big question circled in my mind: *What really happens behind the scenes?* 
What do computers do with memory and files? How do real systems work?



## Autumn 2025: The Leap Into Rust

**Timeline:**
- **Started Rust:** October 27, 2025 (at exactly 00:27 UTC+1 I ran my first ‘Hello World’ in Rust—a super basic program, but it was magic)
- **Shipped mini-kvstore-v2:** November 21, 2025
- **Released minikv (distributed):** December 2025 (`v0.3.0` on December 22)

After hearing:
- “Rust is way too hard.”
- “Beware the borrow checker!”
- “It’s not for beginners.”

… I had zero formal tech background, but I wanted to *really* learn how systems worked and challenge myself with “low level” code.



## First Impressions

- **The compiler is strict but a true teacher:** error messages are detailed, and sometimes even confessional—pointing right to a solution.
- **Ownership and borrowing:** I thought I got “ownership” from literature, but Rust forces you to internalize it in a new way.
- **Everything’s explicit:** Who owns what, who can change or just borrow, and for how long.
- **The Rust community? Genuinely welcoming**, even to newbies.



## What Helped Me Along the Way

- **The Rust Book:** Everyone says it, because it’s true (especially Chapter 4—ownership!).
- **Clippy:** My favorite code reviewer, even when its feedback stings.
- **Keeping notes:** Writing down every concept, compiler message, and solution helped me not get overwhelmed.
- **Building side projects:** Practice makes you grow. Even “failing” is progress.



## My Non-Tech Background: Actually an Advantage

- Loops, structure, types… they all remind me of literary analysis—except this time the meaning is for the machine.
- Close reading (“is this reference mutable or immutable?”) and not skipping details were skills that transferred perfectly.
- Patience with ambiguity, digging deep until understanding—the same in both worlds!
- UI/UX taught me to design for people. Rust taught me to design for people *and* computers.



## What I Wish I'd Known Earlier

- *You don’t need to be “technical” to start.* Curiosity is the real prerequisite.
- *Don’t optimize too soon:* get it working, then get it right.
- *Testing can’t be too early.*
- *Learning isn’t linear.* There are setbacks, there are victories. Stick with it!



## A Few Tips From Me

1. **Start before you feel “ready”**—you only get ready by doing.
2. **Read error messages like you’d read between the lines of a text**—all the clues are there.
3. **Celebrate every little win**—your first compiling program is a scoreboard moment!
4. **Don’t be afraid to ask for help** (Discord, Reddit, Rust forums, etc.).
5. **Have fun**: enjoying the ride is the secret fuel.



## About minikv: What It Can Do (v0.3.0)

**Distributed Core:**
- Multi-node Raft consensus (leader election, log replication, snapshots, recovery, partition detection)
- Advanced Two-Phase Commit (2PC) for distributed writes: chunked transfers, error handling, retries, timeouts
- Configurable N-way replication (default: 3 replicas)
- High Random Weight (HRW) placement for even distribution
- 256 virtual shards for horizontal scaling
- Automatic cluster rebalancing (load detection, blob migration, metadata updates)
- Range queries (efficient scans across keys)
- Batch operations API (multi-put/get/delete)
- TLS encryption for HTTP and gRPC (production-ready security)
- Flexible configuration: file, env, CLI override

**Storage Engine:**
- Segmented, append-only log structure
- In-memory HashMap indexing for O(1) key lookups
- Bloom filters for fast negative queries
- Instant index snapshots (5ms restarts)
- CRC32 checksums on every record
- Automatic background compaction and space reclamation

**Durability:**
- Write-Ahead Log (WAL) for safety
- Configurable fsync policy (always, interval, never)
- Fast crash recovery via WAL replay

**APIs:**
- gRPC for internal communication (coordinator ↔ volume)
- HTTP REST API for clients
- CLI for cluster ops (verify, repair, compact, rebalance, batch, range)

**Infrastructure:**
- Docker Compose setup for dev/test
- GitHub Actions for CI/CD
- k6 benchmarks for multiple scenarios
- Distributed tracing via OpenTelemetry & Jaeger
- Prometheus metrics endpoint (/metrics)

**Testing & Internationalization:**
- Professional integration, stress, and recovery tests
- All code/scripts/docs in English



## My Takeaway

> “If you can read and express an idea, you can code. Patience, curiosity, and a love of learning are everything!”



*Written by Em' (@whispem), proud Rust beginner—and living proof that you truly learn by building… even distributed key-value stores, even when you start from square one.*

*"Structure determines meaning. You learn by writing — and by building."*
