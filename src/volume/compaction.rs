//! Background compaction for blob storage

use crate::common::Result;
use crate::volume::blob::BlobStore;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::time;

/// Compaction manager
pub struct CompactionManager {
    store: Arc<Mutex<BlobStore>>,
    interval: Duration,
    threshold: usize,
}

impl CompactionManager {
    pub fn new(store: Arc<Mutex<BlobStore>>, interval_secs: u64, threshold: usize) -> Self {
        Self {
            store,
            interval: Duration::from_secs(interval_secs),
            threshold,
        }
    }

    /// Start background compaction task
    pub fn start(self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = time::interval(self.interval);

            loop {
                interval.tick().await;

                if let Err(e) = self.maybe_compact().await {
                    tracing::error!("Compaction failed: {}", e);
                }
            }
        })
    }

    /// Check if compaction is needed and run it
    async fn maybe_compact(&self) -> Result<()> {
        let should_compact = {
            let store = self.store.lock().unwrap();
            let stats = store.stats();
            stats.total_keys >= self.threshold
        };

        if should_compact {
            tracing::info!("Starting compaction (threshold reached)");
            let start = std::time::Instant::now();

            let mut store = self.store.lock().unwrap();
            store.compact()?;

            let elapsed = start.elapsed();
            tracing::info!("Compaction completed in {:.2}s", elapsed.as_secs_f64());
        }

        Ok(())
    }
}
