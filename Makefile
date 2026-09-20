.PHONY: help build test ci smoke integration bench benchmark proto fmt fmt-check clippy docs pre-commit release-preflight release-preflight-full serve run-coord run-volume docker-build docker-up docker-down docker-logs otel-up otel-down bench-all bench-write bench-read clean

help:
	@echo "minikv - Makefile targets:"
	@echo ""
	@echo "Build & Test:"
	@echo "  make build        - Build release binaries (release mode)"
	@echo "  make test         - Run all unit and integration tests (starts a coordinator on port 8000)"
	@echo "  make ci           - Run the same checks as GitHub CI: fmt, clippy, build, tests"
	@echo "  make smoke        - Run the end-to-end cluster test (3 coordinators + 3 volumes)"
	@echo "  make integration  - Run integration tests for cluster features"
	@echo "  make bench        - Run Criterion benchmarks for performance"
	@echo "  make benchmark    - Run k6 HTTP benchmarks for API"
	@echo ""
	@echo "Development:"
	@echo "  make proto        - Generate protobuf code for gRPC APIs"
	@echo "  make fmt          - Format Rust codebase"
	@echo "  make fmt-check    - Check formatting without changing files"
	@echo "  make clippy       - Run Rust lints"
	@echo "  make docs         - Generate Rust documentation"
	@echo "  make pre-commit   - Run all checks before commit"
	@echo "  make release-preflight      - Run fast GA preflight checks"
	@echo "  make release-preflight-full - Run full GA preflight checks"
	@echo ""
	@echo "Run:"
	@echo "  make serve        - Start local cluster (3 coordinators + 3 volumes)"
	@echo "  make run-coord    - Start a single coordinator node"
	@echo "  make run-volume   - Start a single volume node"
	@echo ""
	@echo "Docker:"
	@echo "  make docker-build - Build all Docker images for cluster"
	@echo "  make docker-up    - Start full Docker cluster"
	@echo "  make docker-down  - Stop Docker cluster"
	@echo "  make docker-logs  - View logs from all Docker containers"
	@echo ""
	@echo "Observability:"
	@echo "  make otel-up      - Start Jaeger, Prometheus, and Grafana stack"
	@echo "  make otel-down    - Stop observability stack"
	@echo ""
	@echo "Benchmarks:"
	@echo "  make bench-all    - Run all k6 benchmark scenarios"
	@echo "  make bench-write  - Run write-heavy benchmark scenario"
	@echo "  make bench-read   - Run read-heavy benchmark scenario"
	@echo ""
	@echo "Cleanup:"
	@echo "  make clean        - Clean build artifacts and local cluster data"
	@echo ""

build:
	cargo build --release

test:
	cargo build --release --bin minikv-coord
	@cargo run --release --bin minikv-coord -- serve --id 1 > coord-test-server.log 2>&1 & \
	COORD=$$!; \
	sleep 3; \
	cargo test --all --release; \
	STATUS=$$?; \
	{ kill $$COORD && wait $$COORD; } 2>/dev/null; \
	git restore config.toml 2>/dev/null || true; \
	exit $$STATUS

ci: fmt-check clippy build test

smoke:
	cargo test --release --test distributed_cluster -- --nocapture

integration:
	cargo test --test integration

bench:
	cargo bench

benchmark:
	bash ./scripts/benchmark.sh

proto:
	cargo build

fmt:
	cargo fmt --all

fmt-check:
	cargo fmt --all -- --check

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

docs:
	cargo doc --no-deps --open

pre-commit: fmt clippy test
	@echo "✓ Pre-commit checks passed!"

release-preflight:
	bash ./scripts/release_ga.sh --fast

release-preflight-full:
	bash ./scripts/release_ga.sh

serve:
	bash ./scripts/serve.sh 3 3

run-coord:
	cargo run --release --bin minikv-coord -- serve \
		--id coord-1 \
		--bind 0.0.0.0:5000 \
		--grpc 0.0.0.0:5001 \
		--db ./coord-data

run-volume:
	cargo run --release --bin minikv-volume -- serve \
		--id vol-1 \
		--bind 0.0.0.0:6000 \
		--grpc 0.0.0.0:6001 \
		--data ./vol1-data \
		--wal ./vol1-wal \
		--coordinators http://localhost:5000

docker-build:
	docker build -f Dockerfile.coordinator -t minikv-coord:latest .
	docker build -f Dockerfile.volume -t minikv-volume:latest .

docker-up:
	docker-compose up -d

docker-down:
	docker-compose down -v

docker-logs:
	docker-compose logs -f

otel-up:
	cd opentelemetry && docker-compose up -d

otel-down:
	cd opentelemetry && docker-compose down -v

bench-all:
	bash ./bench/run_all.sh

bench-write:
	k6 run bench/scenarios/write-heavy.js

bench-read:
	k6 run bench/scenarios/read-heavy.js

clean:
	cargo clean
	rm -rf coord-data/ vol*-data/ vol*-wal/ data/