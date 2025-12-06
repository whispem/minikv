.PHONY: help build test bench clean run-coord run-volume proto fmt clippy

help:
	@echo "minikv - Makefile targets:"
	@echo ""
	@echo "  make build        - Build release binaries"
	@echo "  make test         - Run all tests"
	@echo "  make bench        - Run benchmarks"
	@echo "  make proto        - Generate protobuf code"
	@echo "  make fmt          - Format code"
	@echo "  make clippy       - Run lints"
	@echo "  make clean        - Clean build artifacts"
	@echo "  make run-coord    - Start coordinator"
	@echo "  make run-volume   - Start volume server"
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
