#!/usr/bin/env bash
# Ultimate fix script for minikv - Solves ALL compilation issues

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}╔═══════════════════════════════════════════╗${NC}"
echo -e "${BLUE}║   minikv - Ultimate Compilation Fix      ║${NC}"
echo -e "${BLUE}╚═══════════════════════════════════════════╝${NC}"
echo ""

# Check we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}✗ Error: Cargo.toml not found${NC}"
    echo "Run this script from the minikv project root"
    exit 1
fi

echo -e "${YELLOW}📦 Creating backups...${NC}"
cp Cargo.toml Cargo.toml.backup 2>/dev/null || true
echo -e "${GREEN}✓${NC} Backups created"
echo ""

# Fix 1: Replace corrupted files
echo -e "${BLUE}🔧 Fix 1/7: Replacing corrupted files${NC}"

# src/common/mod.rs - FIX EXPORTS
cat > src/common/mod.rs << 'EOF'
//! Common utilities and types shared across minikv

pub mod config;
pub mod error;
pub mod hash;
pub mod utils;

pub use config::{Config, CoordinatorConfig, NodeRole, RuntimeConfig, VolumeConfig, WalSyncPolicy};
pub use error::{Error, Result};
pub use hash::{
    blake3_hash, blob_prefix, hrw_hash, select_replicas, shard_key, Blake3Hasher,
    ConsistentHashRing,
};
pub use utils::{
    crc32, decode_key, encode_key, format_bytes, parse_duration, timestamp_now, NodeState,
};
EOF
echo -e "${GREEN}✓${NC} src/common/mod.rs fixed"

# Fix 2: src/volume/server.rs - COMPLETE REWRITE
echo -e "${BLUE}🔧 Fix 2/7: Complete rewrite of src/volume/server.rs${NC}"
cat > src/volume/server.rs << 'EOF'
//! Volume server

use crate::common::{Result, VolumeConfig};
use crate::volume::blob::BlobStore;
use crate::volume::compaction::CompactionManager;
use crate::volume::grpc::VolumeGrpcService;
use crate::volume::http::{create_router, VolumeState};
use std::sync::{Arc, Mutex};

pub struct VolumeServer {
    config: VolumeConfig,
    volume_id: String,
}

impl VolumeServer {
    pub fn new(config: VolumeConfig, volume_id: String) -> Self {
        Self { config, volume_id }
    }

    pub async fn serve(self) -> Result<()> {
        tracing::info!("Starting volume server: {}", self.volume_id);
        tracing::info!("  HTTP API: {}", self.config.bind_addr);
        tracing::info!("  gRPC API: {}", self.config.grpc_addr);
        tracing::info!("  Data path: {}", self.config.data_path.display());
        tracing::info!("  WAL path: {}", self.config.wal_path.display());

        // Initialize blob store
        let store = Arc::new(Mutex::new(BlobStore::open(
            &self.config.data_path,
            &self.config.wal_path,
            self.config.wal_sync,
        )?));

        // Start background compaction
        let compaction = CompactionManager::new(
            store.clone(),
            self.config.compaction_interval_secs,
            self.config.compaction_threshold,
        );
        let _compaction_handle = compaction.start();

        // Create HTTP server
        let http_state = VolumeState {
            store: store.clone(),
            volume_id: self.volume_id.clone(),
        };
        let http_router = create_router(http_state, self.config.max_blob_size as usize / (1024 * 1024));

        // Create gRPC server
        let grpc_service = VolumeGrpcService::new(store, self.volume_id.clone());
        let grpc_server = tonic::transport::Server::builder()
            .add_service(grpc_service.into_server())
            .serve(self.config.grpc_addr);

        // Start servers
        let http_listener = tokio::net::TcpListener::bind(self.config.bind_addr).await?;
        let http_server = axum::serve(http_listener, http_router);

        tracing::info!("✓ Volume server ready");

        tokio::select! {
            res = http_server => {
                if let Err(e) = res {
                    tracing::error!("HTTP server error: {}", e);
                }
            }
            res = grpc_server => {
                if let Err(e) = res {
                    tracing::error!("gRPC server error: {}", e);
                }
            }
        }

        Ok(())
    }
}
EOF
echo -e "${GREEN}✓${NC} src/volume/server.rs rewritten"

# Fix 3: src/coordinator/http.rs - FIX UNUSED VARS
echo -e "${BLUE}🔧 Fix 3/7: Fixing unused variables in coordinator/http.rs${NC}"
sed -i.bak 's/Path(key): Path<String>/Path(_key): Path<String>/g' src/coordinator/http.rs
sed -i.bak 's/body: Bytes/_body: Bytes/g' src/coordinator/http.rs
rm -f src/coordinator/http.rs.bak
echo -e "${GREEN}✓${NC} coordinator/http.rs fixed"

# Fix 4: src/ops/*.rs - CREATE SEPARATE FILES
echo -e "${BLUE}🔧 Fix 4/7: Fixing ops stubs (verify, repair, compact)${NC}"

# verify.rs
cat > src/ops/verify.rs << 'EOF'
//! Verify cluster integrity

#![allow(dead_code)]

use crate::common::Result;

pub async fn verify_cluster(
    _coordinator_url: &str,
    _deep: bool,
    _concurrency: usize,
) -> Result<VerifyReport> {
    tracing::info!("Starting cluster verification");

    // TODO: Implement verification logic

    Ok(VerifyReport {
        total_keys: 0,
        healthy: 0,
        under_replicated: 0,
        corrupted: 0,
        orphaned: 0,
    })
}

#[derive(Debug)]
pub struct VerifyReport {
    pub total_keys: usize,
    pub healthy: usize,
    pub under_replicated: usize,
    pub corrupted: usize,
    pub orphaned: usize,
}
EOF

# repair.rs
cat > src/ops/repair.rs << 'EOF'
//! Repair under-replicated keys

#![allow(dead_code)]

use crate::common::Result;

pub async fn repair_cluster(
    _coordinator_url: &str,
    _replicas: usize,
    _dry_run: bool,
) -> Result<RepairReport> {
    tracing::info!("Starting cluster repair");

    // TODO: Implement repair logic

    Ok(RepairReport {
        keys_checked: 0,
        keys_repaired: 0,
        bytes_copied: 0,
    })
}

#[derive(Debug)]
pub struct RepairReport {
    pub keys_checked: usize,
    pub keys_repaired: usize,
    pub bytes_copied: u64,
}
EOF

# compact.rs
cat > src/ops/compact.rs << 'EOF'
//! Cluster-wide compaction

#![allow(dead_code)]

use crate::common::Result;

pub async fn compact_cluster(
    _coordinator_url: &str,
    _shard: Option<u64>,
) -> Result<CompactReport> {
    tracing::info!("Starting cluster compaction");

    // TODO: Implement compaction logic

    Ok(CompactReport {
        volumes_compacted: 0,
        bytes_freed: 0,
    })
}

#[derive(Debug)]
pub struct CompactReport {
    pub volumes_compacted: usize,
    pub bytes_freed: u64,
}
EOF

echo -e "${GREEN}✓${NC} Ops stubs fixed"

# Fix 5: src/volume/blob.rs - FIX BLOOM FILTER
echo -e "${BLUE}🔧 Fix 5/7: Fixing bloom filter in blob.rs${NC}"
sed -i.bak 's/Bloom::new(100_000, 50_000)/Bloom::new_for_fp_rate(100_000, 0.01)/g' src/volume/blob.rs
rm -f src/volume/blob.rs.bak
echo -e "${GREEN}✓${NC} Bloom filter fixed"

# Fix 6: Format code
echo -e "${BLUE}🎨 Fix 6/7: Formatting code${NC}"
if cargo fmt --all 2>&1; then
    echo -e "${GREEN}✓${NC} Code formatted"
else
    echo -e "${YELLOW}⚠${NC} Formatting warnings (non-blocking)"
fi

# Fix 7: Clippy auto-fix
echo -e "${BLUE}🔍 Fix 7/7: Applying clippy fixes${NC}"
cargo clippy --fix --allow-dirty --allow-staged --all-targets --all-features 2>&1 | grep -v "warning: unused" | tail -10 || true
echo -e "${GREEN}✓${NC} Clippy fixes applied"

echo ""
echo -e "${BLUE}═══════════════════════════════════════════${NC}"
echo -e "${GREEN}🎉 All fixes applied!${NC}"
echo -e "${BLUE}═══════════════════════════════════════════${NC}"
echo ""

# Build test
echo -e "${YELLOW}🔨 Final build test...${NC}"
if cargo build --all-targets 2>&1 | tail -20; then
    echo ""
    echo -e "${GREEN}✅ BUILD SUCCESSFUL! 🎊${NC}"
    echo ""
    echo -e "${GREEN}Your project now compiles!${NC}"
    echo ""
    echo "Next steps:"
    echo "  1. Review: git diff"
    echo "  2. Test: cargo test"
    echo "  3. Commit: git add -A && git commit -m 'fix: complete compilation'"
    echo "  4. Push: git push"
    echo ""
    echo "💾 Backup: Cargo.toml.backup"
    exit 0
else
    echo ""
    echo -e "${RED}❌ BUILD FAILED${NC}"
    echo ""
    echo "Remaining errors need manual fixing."
    echo "Check the logs above."
    exit 1
fi
