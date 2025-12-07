use crate::common::Result;
use crate::volume::blob::BlobStore;
use std::sync::MutexGuard;

pub fn compact_store(store: &mut MutexGuard<'_, BlobStore>) -> Result<()> {
    store.compact()
}
