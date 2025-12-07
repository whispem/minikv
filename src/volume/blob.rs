use crate::common::{blake3_hash, crc32, Result, WalSyncPolicy};
use crate::volume::index::{BlobLocation, Index};
use crate::volume::wal::{Wal, WalEntry, WalOp};
use bloomfilter::Bloom;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
// ...existing code...

const BLOB_MAGIC: [u8; 4] = [0x42, 0x4C, 0x4F, 0x42];
const SEGMENT_SIZE: u64 = 64 * 1024 * 1024;
const MAX_SEGMENTS: u64 = 1000;

#[derive(Debug)]
// ...existing code...
#[derive(Clone)]
pub struct StoreStats {
    pub total_keys: usize,
    pub total_bytes: u64,
    pub active_segments: usize,
    pub index_size: usize,
    pub bloom_false_positives: u64,
}

pub struct BlobStore {
    data_path: PathBuf,
    // ...existing code...
    index: Index,
    bloom: Bloom<[u8; 32]>,
    wal: Wal,
    current_segment: u64,
    current_offset: u64,
    sync_policy: WalSyncPolicy,
}

impl BlobStore {
    pub fn open(data_path: &Path, wal_path: &Path, sync_policy: WalSyncPolicy) -> Result<Self> {
        fs::create_dir_all(data_path)?;
        fs::create_dir_all(wal_path)?;

        let snapshot_path = data_path.join("index.snap");
        let mut index = if snapshot_path.exists() {
            Index::load_snapshot(&snapshot_path)?
        } else {
            Index::new()
        };

        let bloom_path = data_path.join("bloom.filter");
        let mut bloom = if bloom_path.exists() {
            let bytes = fs::read(&bloom_path)?;
            Bloom::from_bytes(bytes)
                .unwrap_or_else(|_: &str| Bloom::new_for_fp_rate(100_000, 0.01).unwrap())
        } else {
            Bloom::new_for_fp_rate(100_000, 0.01).unwrap()
        };

        let wal_file = wal_path.join("wal.log");
        let wal = Wal::open(&wal_file, sync_policy)?;

        Wal::replay(&wal_file, &mut |entry: WalEntry| {
            match entry.op {
                WalOp::Put { ref key, .. } => {
                    let hash = blake3_hash(key.as_bytes());
                    let hash_vec: Vec<u8> = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
                    let hash_bytes: [u8; 32] = hash_vec.try_into().unwrap_or([0u8; 32]);
                    bloom.set(&hash_bytes);
                }
                WalOp::Delete { ref key } => {
                    index.remove(key);
                }
            }
            Ok(())
        })?;

        if !snapshot_path.exists() {
            Self::rebuild_index_from_segments(&mut index, &mut bloom, data_path)?;
        } else {
            for key in index.keys() {
                let hash = blake3_hash(key.as_bytes());
                let hash_vec: Vec<u8> = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
                let hash_bytes: [u8; 32] = hash_vec.try_into().unwrap_or([0u8; 32]);
                bloom.set(&hash_bytes);
            }
        }

        let (current_segment, current_offset) = Self::find_current_position(data_path)?;

        Ok(Self {
            data_path: data_path.to_path_buf(),
            // ...existing code...
            index,
            bloom,
            wal,
            current_segment,
            current_offset,
            sync_policy,
        })
    }

    pub fn put(&mut self, key: &str, value: &[u8]) -> Result<()> {
        self.wal.append_put(key, value)?;
        let hash = blake3_hash(key.as_bytes());
        let hash_vec: Vec<u8> = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
        let hash_bytes: [u8; 32] = hash_vec.try_into().unwrap_or([0u8; 32]);
        self.bloom.set(&hash_bytes);
        let location = self.write_blob(key, value)?;
        self.index.insert(key.to_string(), location);
        Ok(())
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let hash = blake3_hash(key.as_bytes());
        let hash_vec: Vec<u8> = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
        let hash_bytes: [u8; 32] = hash_vec.try_into().unwrap_or([0u8; 32]);

        if !self.bloom.check(&hash_bytes) {
            return Ok(None);
        }

        match self.index.get(key) {
            Some(loc) => self.read_blob(loc),
            None => Ok(None),
        }
    }

    pub fn delete(&mut self, key: &str) -> Result<()> {
        self.wal.append_delete(key)?;
        self.index.remove(key);
        Ok(())
    }

    pub fn compact(&mut self) -> Result<()> {
        let temp_path = self.data_path.join("compact_temp");
        fs::create_dir_all(&temp_path)?;

        let mut new_index = Index::new();
        let mut new_segment = 0u64;
        let mut new_offset = 0u64;

        for (key, old_location) in self.index.iter() {
            if let Ok(Some(value)) = self.read_blob(old_location) {
                let location =
                    self.write_blob_to_segment(&temp_path, new_segment, new_offset, key, &value)?;
                new_index.insert(key.clone(), location.clone());
                new_offset = location.offset + location.size + 16;
                if new_offset > SEGMENT_SIZE {
                    new_segment += 1;
                    new_offset = 0;
                }
            }
        }

        let backup_path = self.data_path.join("compact_backup");
        fs::rename(&self.data_path, &backup_path)?;
        fs::rename(&temp_path, &self.data_path)?;

        self.index = new_index;
        self.current_segment = new_segment;
        self.current_offset = new_offset;

        self.save_snapshot()?;
        self.wal.truncate()?;
        fs::remove_dir_all(&backup_path)?;

        Ok(())
    }

    pub fn save_snapshot(&self) -> Result<()> {
        let snapshot_path = self.data_path.join("index.snap");
        self.index.save_snapshot(&snapshot_path)?;
        let bloom_path = self.data_path.join("bloom.filter");
        let mut f = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(&bloom_path)?;
        f.write_all(&self.bloom.to_bytes())?;
        f.sync_all()?;
        Ok(())
    }

    pub fn stats(&self) -> StoreStats {
        let total_bytes: u64 = self.index.iter().map(|(_, loc)| loc.size).sum();
        StoreStats {
            total_keys: self.index.len(),
            total_bytes,
            active_segments: (self.current_segment + 1) as usize,
            index_size: self.index.len(),
            bloom_false_positives: 0,
        }
    }

    fn write_blob(&mut self, key: &str, value: &[u8]) -> Result<BlobLocation> {
        if self.current_offset > SEGMENT_SIZE {
            self.current_segment += 1;
            self.current_offset = 0;
            if self.current_segment >= MAX_SEGMENTS {
                return Err(crate::Error::Internal("Max segments reached".into()));
            }
        }

        let location = self.write_blob_to_segment(
            &self.data_path,
            self.current_segment,
            self.current_offset,
            key,
            value,
        )?;
        // Header size: MAGIC(4) + KEY_LEN(4) + VAL_LEN(8) + KEY + VALUE + CHECKSUM(4) = 20 + key.len() + value.len()
        self.current_offset = location.offset + 20 + key.len() as u64 + location.size;
        Ok(location)
    }

    fn write_blob_to_segment(
        &self,
        base_path: &Path,
        segment: u64,
        offset: u64,
        key: &str,
        value: &[u8],
    ) -> Result<BlobLocation> {
        let segment_dir = base_path
            .join(format!("{:02}", segment % 100))
            .join(format!("{:02}", segment / 100));
        fs::create_dir_all(&segment_dir)?;
        let segment_file = segment_dir.join(format!("seg_{:04}.blob", segment));
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(false)
            .open(&segment_file)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut writer = BufWriter::new(&file);

        writer.write_all(&BLOB_MAGIC)?;
        writer.write_all(&(key.len() as u32).to_le_bytes())?;
        writer.write_all(&(value.len() as u64).to_le_bytes())?;
        writer.write_all(key.as_bytes())?;
        writer.write_all(value)?;

        let mut checksum_data = Vec::new();
        checksum_data.extend_from_slice(&(key.len() as u32).to_le_bytes());
        checksum_data.extend_from_slice(&(value.len() as u64).to_le_bytes());
        checksum_data.extend_from_slice(key.as_bytes());
        checksum_data.extend_from_slice(value);
        let checksum = crc32(&checksum_data);
        writer.write_all(&checksum.to_le_bytes())?;
        writer.flush()?;

        if self.sync_policy == WalSyncPolicy::Always {
            file.sync_all()?;
        }

        let blake3 = blake3_hash(value);
        Ok(BlobLocation {
            shard: segment,
            offset,
            size: value.len() as u64,
            blake3,
        })
    }

    fn read_blob(&self, location: &BlobLocation) -> Result<Option<Vec<u8>>> {
        let segment_file = self.data_path.join(format!(
            "{:02}/{:02}/seg_{:04}.blob",
            location.shard % 100,
            location.shard / 100,
            location.shard
        ));
        if !segment_file.exists() {
            return Ok(None);
        }

        let file = File::open(&segment_file)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(location.offset))?;

        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;
        if magic != BLOB_MAGIC {
            return Err(crate::Error::Corrupted("Invalid blob magic".into()));
        }

        let mut key_len_bytes = [0u8; 4];
        reader.read_exact(&mut key_len_bytes)?;
        let key_len = u32::from_le_bytes(key_len_bytes) as usize;

        let mut val_len_bytes = [0u8; 8];
        reader.read_exact(&mut val_len_bytes)?;
        let val_len = u64::from_le_bytes(val_len_bytes) as usize;

        let mut key_bytes = vec![0u8; key_len];
        reader.read_exact(&mut key_bytes)?;
        let mut value = vec![0u8; val_len];
        reader.read_exact(&mut value)?;

        let mut checksum_bytes = [0u8; 4];
        reader.read_exact(&mut checksum_bytes)?;
        let stored_checksum = u32::from_le_bytes(checksum_bytes);

        let mut checksum_data = Vec::new();
        checksum_data.extend_from_slice(&key_len_bytes);
        checksum_data.extend_from_slice(&val_len_bytes);
        checksum_data.extend_from_slice(&key_bytes);
        checksum_data.extend_from_slice(&value);
        let computed_checksum = crc32(&checksum_data);

        if computed_checksum != stored_checksum {
            return Err(crate::Error::ChecksumMismatch {
                expected: format!("{:08x}", stored_checksum),
                actual: format!("{:08x}", computed_checksum),
            });
        }

        Ok(Some(value))
    }

    fn rebuild_index_from_segments(
        index: &mut Index,
        bloom: &mut Bloom<[u8; 32]>,
        data_path: &Path,
    ) -> Result<()> {
        for entry in fs::read_dir(data_path)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }

            for subentry in fs::read_dir(entry.path())? {
                let subentry = subentry?;
                if !subentry.path().is_dir() {
                    continue;
                }

                for file_entry in fs::read_dir(subentry.path())? {
                    let file_entry = file_entry?;
                    let path = file_entry.path();
                    if path.extension().and_then(|s| s.to_str()) == Some("blob") {
                        Self::scan_segment(index, bloom, &path)?;
                    }
                }
            }
        }
        Ok(())
    }

    fn scan_segment(index: &mut Index, bloom: &mut Bloom<[u8; 32]>, path: &Path) -> Result<()> {
        let file = File::open(path)?;
        let mut reader = BufReader::new(file);
        let mut offset = 0u64;
        let segment = path
            .file_stem()
            .and_then(|s| s.to_str())
            .and_then(|s| s.strip_prefix("seg_"))
            .and_then(|s| s.parse::<u64>().ok())
            .unwrap_or(0);

        loop {
            let mut magic = [0u8; 4];
            match reader.read_exact(&mut magic) {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
                Err(e) => return Err(e.into()),
            }

            if magic != BLOB_MAGIC {
                break;
            }

            let mut key_len_bytes = [0u8; 4];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u32::from_le_bytes(key_len_bytes) as usize;

            let mut val_len_bytes = [0u8; 8];
            reader.read_exact(&mut val_len_bytes)?;
            let val_len = u64::from_le_bytes(val_len_bytes) as usize;

            let mut key_bytes = vec![0u8; key_len];
            reader.read_exact(&mut key_bytes)?;
            let key = String::from_utf8_lossy(&key_bytes).to_string();

            reader.seek(SeekFrom::Current(val_len as i64))?;
            let mut checksum_bytes = [0u8; 4];
            reader.read_exact(&mut checksum_bytes)?;

            let hash = blake3_hash(key.as_bytes());
            let hash_vec: Vec<u8> = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
            let hash_bytes: [u8; 32] = hash_vec.try_into().unwrap_or([0u8; 32]);
            bloom.set(&hash_bytes);

            index.insert(
                key,
                BlobLocation {
                    shard: segment,
                    offset,
                    size: val_len as u64,
                    blake3: hash,
                },
            );

            offset += 4 + 4 + 8 + key_len as u64 + val_len as u64 + 4;
        }
        Ok(())
    }

    fn find_current_position(data_path: &Path) -> Result<(u64, u64)> {
        let mut max_segment = 0u64;
        let mut max_offset = 0u64;

        if !data_path.exists() {
            return Ok((0, 0));
        }

        for entry in fs::read_dir(data_path)? {
            let entry = entry?;
            if !entry.path().is_dir() {
                continue;
            }

            for subentry in fs::read_dir(entry.path())? {
                let subentry = subentry?;
                if !subentry.path().is_dir() {
                    continue;
                }

                for file_entry in fs::read_dir(subentry.path())? {
                    let file_entry = file_entry?;
                    let path = file_entry.path();

                    if path.extension().and_then(|s| s.to_str()) == Some("blob") {
                        let segment = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .and_then(|s| s.strip_prefix("seg_"))
                            .and_then(|s| s.parse::<u64>().ok())
                            .unwrap_or(0);
                        let metadata = fs::metadata(&path)?;
                        let size = metadata.len();

                        if segment > max_segment || (segment == max_segment && size > max_offset) {
                            max_segment = segment;
                            max_offset = size;
                        }
                    }
                }
            }
        }

        Ok((max_segment, max_offset))
    }
}
