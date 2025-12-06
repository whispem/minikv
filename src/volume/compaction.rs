use crate::volume::blob::BlobStore;
use std::sync::MutexGuard;

pub fn compact_store(store: &mut MutexGuard<'_, BlobStore>) -> Result<(), String> {
    store.compact()?;
    Ok(())
}
