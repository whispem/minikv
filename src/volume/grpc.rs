use crate::volume::blob::BlobStore;

pub struct VolumeGrpcService {
    store: BlobStore,
}

impl VolumeGrpcService {
    pub fn new(store: BlobStore) -> Self {
        VolumeGrpcService { store }
    }
}
