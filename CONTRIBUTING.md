# Contributing to minikv

Thank you for your interest in contributing to minikv! 🦀

## Quick Start

```bash
# Fork and clone
git clone https://github.com/YOUR_USERNAME/minikv
cd minikv

# Build
cargo build --release

# Run tests
cargo test
cargo test --test integration

# Format and lint
cargo fmt --all
cargo clippy --all-targets -- -D warnings
```

## Areas That Need Help

### 1. Complete Raft Implementation
Currently simplified for single-node testing. Need:
- Multi-node leader election
- Log replication
- Snapshot mechanism
- Membership changes

### 2. Full 2PC Streaming
- Coordinator → Volume data streaming
- Resume partial uploads
- Concurrent uploads optimization

### 3. Performance
- Zero-copy I/O with sendfile
- io_uring integration (Linux)
- Batch operations
- Compression (LZ4/Zstd)

### 4. Ops Commands
- Complete `verify` logic
- Complete `repair` logic
- Rebalancing automation

### 5. Testing
- More integration tests
- Fault injection tests
- Chaos engineering

## Development Workflow

### 1. Create a feature branch

```bash
git checkout -b feature/my-feature
```

### 2. Make changes

Follow the existing code style:
- 4-space indentation
- Clear comments
- Comprehensive error handling
- Unit tests for new code

### 3. Test

```bash
cargo test
cargo test --test integration
cargo clippy --all-targets
```

### 4. Commit

Use conventional commits:

```bash
git commit -m "feat: add Raft log replication"
git commit -m "fix: correct 2PC abort logic"
git commit -m "docs: update architecture diagram"
```

### 5. Push and create PR

```bash
git push origin feature/my-feature
```

Then create a Pull Request on GitHub.

## Code Style

- Use `cargo fmt` before committing
- Pass `cargo clippy` with no warnings
- Add tests for new features
- Update documentation

## Questions?

Open an issue or reach out to [@whispem](https://github.com/whispem).
