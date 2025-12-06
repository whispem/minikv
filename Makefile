.PHONY: help build test bench clean run-coord run-volume proto fmt clippy docs

help:
	@echo "minikv - Makefile targets:"
	@echo ""
	@echo "Build & Test:"
	@echo "  make build        - Build release binaries"
	@echo "  make test         - Run all tests"
	@echo "  make integration  - Run integration tests"
	@echo "  make bench        - Run Criterion benchmarks"
	@echo "  make benchmark    - Run k6 HTTP benchmarks"
	@echo ""
	@echo "Development:"
	@echo "  make proto        - Generate protobuf code"
	@echo "  make fmt          - Format code"
	@echo "  make clippy       - Run lints"
	@echo "  make docs         - Generate documentation"
	@echo "  make pre-commit   - Run all checks"
	@echo ""
	@echo "Run:"
	@echo "  make serve        - Start local cluster (3 coord + 3 volumes)"
	@echo "  make run-coord    - Start single coordinator"
	@echo "  make run-volume   - Start single volume"
	@echo "  make smoke        - Run smoke tests"
	@echo "  make verify       - Verify cluster integrity"
	@echo ""
	@echo "Docker:"
	@echo "  make docker-build - Build Docker images"
	@echo "  make docker-up    - Start Docker cluster"
	@echo "  make docker-down  - Stop Docker cluster"
	@echo "  make docker-logs  - View Docker logs"
	@echo ""
	@echo "Observability:"
	@echo "  make otel-up      - Start Jaeger/Prometheus/Grafana"
	@echo "  make otel-down    - Stop observability stack"
	@echo ""
	@echo "Benchmarks:"
	@echo "  make bench-all    - Run all k6 scenarios"
	@echo "  make bench-write  - Run write-heavy scenario"
	@echo "  make bench-read   - Run read-heavy scenario"
	@echo ""
	@echo "Cleanup:"
	@echo "  make clean        - Clean build artifacts"
	@echo ""

build:
	cargo build --release

test:
	cargo test --all --release

bench:
	cargo bench

proto:
	cargo build

fmt:
	cargo fmt --all

clippy:
	cargo clippy --all-targets --all-features -- -D warnings

clean:
	cargo clean
	rm -rf coord-data/ vol*-data/ vol*-wal/

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

benchmark:
	./scripts/benchmark.sh

docker-build:
	docker build -f Dockerfile.coordinator -t minikv-coord:latest .
	docker build -f Dockerfile.volume -t minikv-volume:latest .

docker-up:
	docker-compose up -d

docker-down:
	docker-compose down -v

docker-logs:
	docker-compose logs -f

integration:
	cargo test --test integration

pre-commit: fmt clippy test
	@echo "✓ Pre-commit checks passed!"

# Serve
serve:
	./scripts/serve.sh 3 3

smoke:
	./scripts/smoke_test.sh

verify:
	./scripts/verify.sh

# Observability
otel-up:
	cd opentelemetry && docker-compose up -d

otel-down:
	cd opentelemetry && docker-compose down -v

# Benchmark scenarios
bench-all:
	./bench/run_all.sh

bench-write:
	k6 run bench/scenarios/write-heavy.js

bench-read:
	k6 run bench/scenarios/read-heavy.js

docs:
	cargo doc --no-deps --open
