use crate::volume::blob::BlobStore;
use crate::common::Result;
use std::sync::MutexGuard;

pub fn compact_store(store: &mut MutexGuard<'_, BlobStore>) -> Result<()> {
    store.compact()
}
