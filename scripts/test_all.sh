#!/bin/bash
set -e

# Run all unit and integration tests
cargo test --all --release

# Run advanced S3 tests separately
cargo test --test s3_api_extra --release

# Clean up test-generated files
rm -rf coord-* vol-* /tmp/minikv-config-*.toml *.log

echo "All tests passed and the workspace is clean."
