use crate::volume::blob::BlobStore;

pub struct VolumeGrpcService {
    #[allow(dead_code)]
    store: BlobStore,
}

impl VolumeGrpcService {
    pub fn new(store: BlobStore) -> Self {
        VolumeGrpcService { store }
    }
}
