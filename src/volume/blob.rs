use bloomfilter::Bloom;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::PathBuf;

pub struct BlobStore {
    pub path: PathBuf,
    pub bloom: Bloom<[u8; 32]>,
}

impl BlobStore {
    pub fn new(path: PathBuf) -> Self {
        let bloom = Bloom::new(1000, 100).expect("Failed to create bloom filter");
        BlobStore { path, bloom }
    }

    pub fn compact(&mut self) -> Result<(), String> {
        Ok(())
    }
}

#[derive(Debug)]
pub enum StoreError {
    IoError(String),
    NotFound,
}
