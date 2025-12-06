use crate::volume::blob::BlobStore;

pub struct Location {
    pub size: usize,
    pub blake3: [u8; 32],
}

pub fn get_location(store: &BlobStore) -> Result<Location, String> {
    Ok(Location {
        size: 0,
        blake3: [0; 32],
    })
}
