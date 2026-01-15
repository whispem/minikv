//! BlobStore implementation
//! This module provides the main storage engine for data volumes.
//! It uses a log-structured, append-only design for durability and performance.
//! The in-memory HashMap index enables fast lookups, while a Bloom filter accelerates negative lookups.
//! All operations are logged to a Write-Ahead Log (WAL) for crash recovery.
//!
//! Features in v0.5.0:
//! - TTL (Time-To-Live) support for automatic key expiration
//! - LZ4 compression for efficient storage
//! - Background cleanup task for expired keys

use crate::common::{blake3_hash, crc32, Result, WalSyncPolicy};
use crate::volume::index::{BlobLocation, Index};
use crate::volume::wal::{Wal, WalEntry, WalOp};
use bloomfilter::Bloom;
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const BLOB_MAGIC: [u8; 4] = [0x42, 0x4C, 0x4F, 0x42];
/// Magic bytes for compressed blobs (v0.5.0)
const BLOB_MAGIC_COMPRESSED: [u8; 4] = [0x42, 0x4C, 0x4F, 0x43]; // BLOC
const SEGMENT_SIZE: u64 = 64 * 1024 * 1024;
const MAX_SEGMENTS: u64 = 1000;
/// Minimum size for compression (smaller blobs are stored uncompressed)
const COMPRESSION_THRESHOLD: usize = 128;

#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_keys: usize,
    pub total_bytes: u64,
    pub active_segments: usize,
    pub index_size: usize,
    pub bloom_false_positives: u64,
    /// Number of keys with TTL set
    pub keys_with_ttl: usize,
    /// Number of compressed blobs
    pub compressed_blobs: u64,
}

/// Compression configuration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionMode {
    /// No compression
    #[default]
    None,
    /// LZ4 compression (fast)
    Lz4,
}

/// BlobStore manages the log-structured storage for a volume.
/// It maintains an in-memory index and a Bloom filter for fast lookups.
/// All changes are recorded in a WAL for durability and recovery.
pub struct BlobStore {
    data_path: PathBuf,

    /// In-memory index for key lookups
    index: Index,
    /// Bloom filter for fast negative lookups
    bloom: Bloom<[u8; 32]>,
    /// Write-Ahead Log for durability
    wal: Wal,
    /// Current segment number in log-structured storage
    current_segment: u64,
    /// Current offset in the active segment
    current_offset: u64,
    /// WAL sync policy
    sync_policy: WalSyncPolicy,
    /// Compression mode (v0.5.0)
    compression: CompressionMode,
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

            index,
            bloom,
            wal,
            current_segment,
            current_offset,
            sync_policy,
            compression: CompressionMode::None,
        })
    }

    /// Open BlobStore with compression enabled (v0.5.0)
    pub fn open_with_compression(
        data_path: &Path,
        wal_path: &Path,
        sync_policy: WalSyncPolicy,
        compression: CompressionMode,
    ) -> Result<Self> {
        let mut store = Self::open(data_path, wal_path, sync_policy)?;
        store.compression = compression;
        Ok(store)
    }

    /// Set compression mode at runtime
    pub fn set_compression(&mut self, mode: CompressionMode) {
        self.compression = mode;
    }

    /// Get current compression mode
    pub fn compression_mode(&self) -> CompressionMode {
        self.compression
    }

    /// Put a key-value pair with optional TTL (v0.5.0)
    /// If ttl_ms is Some, the key will expire after the specified milliseconds.
    pub fn put_with_ttl(&mut self, key: &str, value: &[u8], ttl_ms: Option<u64>) -> Result<()> {
        self.wal.append_put(key, value)?;
        let hash = blake3_hash(key.as_bytes());
        let hash_vec: Vec<u8> = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
        let hash_bytes: [u8; 32] = hash_vec.try_into().unwrap_or([0u8; 32]);
        self.bloom.set(&hash_bytes);

        let mut location = self.write_blob(key, value)?;

        // Set expiration if TTL is provided
        if let Some(ttl) = ttl_ms {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;
            location.expires_at = Some(now + ttl);
        }

        self.index.insert(key.to_string(), location);
        Ok(())
    }

    pub fn put(&mut self, key: &str, value: &[u8]) -> Result<()> {
        self.put_with_ttl(key, value, None)
    }

    pub fn get(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let hash = blake3_hash(key.as_bytes());
        let hash_vec: Vec<u8> = hex::decode(&hash).unwrap_or_else(|_| vec![0u8; 32]);
        let hash_bytes: [u8; 32] = hash_vec.try_into().unwrap_or([0u8; 32]);

        if !self.bloom.check(&hash_bytes) {
            return Ok(None);
        }

        // Use get_if_valid to respect TTL (v0.5.0)
        match self.index.get_if_valid(key) {
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
                let (location, bytes_written) =
                    self.write_blob_to_segment(&temp_path, new_segment, new_offset, key, &value)?;
                new_index.insert(key.clone(), location);
                new_offset += bytes_written;
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

    /// Clean up expired keys (v0.5.0)
    /// Returns the number of keys removed.
    pub fn cleanup_expired(&mut self) -> usize {
        self.index.cleanup_expired()
    }

    /// Get TTL remaining for a key in milliseconds (v0.5.0)
    /// Returns None if key doesn't exist or has no TTL.
    pub fn get_ttl(&self, key: &str) -> Option<u64> {
        if let Some(loc) = self.index.get(key) {
            if let Some(expires_at) = loc.expires_at {
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_millis() as u64;
                if now < expires_at {
                    return Some(expires_at - now);
                }
            }
        }
        None
    }

    /// Check if a key exists (respecting TTL) (v0.5.0)
    pub fn exists(&self, key: &str) -> bool {
        self.index.get_if_valid(key).is_some()
    }

    pub fn stats(&self) -> StoreStats {
        let total_bytes: u64 = self.index.iter().map(|(_, loc)| loc.size).sum();
        let keys_with_ttl = self.index.keys_with_ttl().len();
        StoreStats {
            total_keys: self.index.len(),
            total_bytes,
            active_segments: (self.current_segment + 1) as usize,
            index_size: self.index.len(),
            bloom_false_positives: 0,
            keys_with_ttl,
            compressed_blobs: 0, // TODO: track number of compressed blobs
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

        let (location, bytes_written) = self.write_blob_to_segment(
            &self.data_path,
            self.current_segment,
            self.current_offset,
            key,
            value,
        )?;
        self.current_offset = location.offset + bytes_written;
        Ok(location)
    }

    /// Write a blob to a segment file.
    /// Returns (BlobLocation, total_bytes_written)
    fn write_blob_to_segment(
        &self,
        base_path: &Path,
        segment: u64,
        offset: u64,
        key: &str,
        value: &[u8],
    ) -> Result<(BlobLocation, u64)> {
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

        // Compress value if compression is enabled and size is above threshold (v0.5.0)
        let (write_value, is_compressed) =
            if self.compression == CompressionMode::Lz4 && value.len() >= COMPRESSION_THRESHOLD {
                match lz4::block::compress(value, None, true) {
                    Ok(compressed) if compressed.len() < value.len() => (compressed, true),
                    _ => (value.to_vec(), false), // Fallback to uncompressed if compression doesn't help
                }
            } else {
                (value.to_vec(), false)
            };

        // Use different magic for compressed blobs
        let magic = if is_compressed {
            BLOB_MAGIC_COMPRESSED
        } else {
            BLOB_MAGIC
        };
        writer.write_all(&magic)?;

        // Store original size for decompression
        writer.write_all(&(key.len() as u32).to_le_bytes())?;
        writer.write_all(&(write_value.len() as u64).to_le_bytes())?;
        // Store original size for compressed blobs
        writer.write_all(&(value.len() as u64).to_le_bytes())?;
        writer.write_all(key.as_bytes())?;
        writer.write_all(&write_value)?;

        let mut checksum_data = Vec::new();
        checksum_data.extend_from_slice(&(key.len() as u32).to_le_bytes());
        checksum_data.extend_from_slice(&(write_value.len() as u64).to_le_bytes());
        checksum_data.extend_from_slice(&(value.len() as u64).to_le_bytes());
        checksum_data.extend_from_slice(key.as_bytes());
        checksum_data.extend_from_slice(&write_value);
        let checksum = crc32(&checksum_data);
        writer.write_all(&checksum.to_le_bytes())?;
        writer.flush()?;

        if self.sync_policy == WalSyncPolicy::Always {
            file.sync_all()?;
        }

        // Calculate total bytes written:
        // MAGIC(4) + KEY_LEN(4) + VAL_LEN(8) + ORIG_LEN(8) + KEY + VALUE + CHECKSUM(4)
        let bytes_written = 4 + 4 + 8 + 8 + key.len() as u64 + write_value.len() as u64 + 4;

        let blake3 = blake3_hash(value);
        Ok((
            BlobLocation {
                shard: segment,
                offset,
                size: value.len() as u64,
                blake3,
                expires_at: None, // TTL is set by put_with_ttl, not here
            },
            bytes_written,
        ))
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

        // Check for both compressed and uncompressed magic (v0.5.0)
        let is_compressed = magic == BLOB_MAGIC_COMPRESSED;
        if magic != BLOB_MAGIC && !is_compressed {
            return Err(crate::Error::Corrupted("Invalid blob magic".into()));
        }

        let mut key_len_bytes = [0u8; 4];
        reader.read_exact(&mut key_len_bytes)?;
        let key_len = u32::from_le_bytes(key_len_bytes) as usize;

        let mut val_len_bytes = [0u8; 8];
        reader.read_exact(&mut val_len_bytes)?;
        let val_len = u64::from_le_bytes(val_len_bytes) as usize;

        // Read original size (v0.5.0)
        let mut orig_len_bytes = [0u8; 8];
        reader.read_exact(&mut orig_len_bytes)?;
        let orig_len = u64::from_le_bytes(orig_len_bytes) as usize;

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
        checksum_data.extend_from_slice(&orig_len_bytes);
        checksum_data.extend_from_slice(&key_bytes);
        checksum_data.extend_from_slice(&value);
        let computed_checksum = crc32(&checksum_data);

        if computed_checksum != stored_checksum {
            return Err(crate::Error::ChecksumMismatch {
                expected: format!("{:08x}", stored_checksum),
                actual: format!("{:08x}", computed_checksum),
            });
        }

        // Decompress if needed (v0.5.0)
        if is_compressed {
            match lz4::block::decompress(&value, Some(orig_len as i32)) {
                Ok(decompressed) => Ok(Some(decompressed)),
                Err(_) => Err(crate::Error::Corrupted("LZ4 decompression failed".into())),
            }
        } else {
            Ok(Some(value))
        }
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

            // Support both compressed and uncompressed magic (v0.5.0)
            let is_compressed = magic == BLOB_MAGIC_COMPRESSED;
            if magic != BLOB_MAGIC && !is_compressed {
                break;
            }

            let mut key_len_bytes = [0u8; 4];
            reader.read_exact(&mut key_len_bytes)?;
            let key_len = u32::from_le_bytes(key_len_bytes) as usize;

            let mut val_len_bytes = [0u8; 8];
            reader.read_exact(&mut val_len_bytes)?;
            let val_len = u64::from_le_bytes(val_len_bytes) as usize;

            // Read original size (v0.5.0)
            let mut orig_len_bytes = [0u8; 8];
            reader.read_exact(&mut orig_len_bytes)?;
            let orig_len = u64::from_le_bytes(orig_len_bytes);

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
                    size: orig_len, // Use original size, not compressed size
                    blake3: hash,
                    expires_at: None, // Legacy entries don't have TTL
                },
            );

            // Updated offset calculation: MAGIC(4) + KEY_LEN(4) + VAL_LEN(8) + ORIG_LEN(8) + KEY + VALUE + CHECKSUM(4)
            offset += 4 + 4 + 8 + 8 + key_len as u64 + val_len as u64 + 4;
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
