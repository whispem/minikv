//! Blob storage for a volume server.
//!
//! Values are appended to numbered segment files of 64 MiB: segment 123 is
//! `23/01/seg_0123.blob` in the data directory. A record holds the key, the
//! value (compressed with LZ4 when compression is on and helps) and a CRC32
//! that every read checks. An in-memory index maps each key to its current
//! record, and a Bloom filter answers most lookups of absent keys.
//!
//! Every put and delete is first appended to the write-ahead log (WAL), which
//! the store never truncates. At startup, [`BlobStore::open`] rebuilds the
//! index from the segments and the WAL.
//!
//! Optional features, which the volume server does not use:
//! - TTL: [`BlobStore::put_with_ttl`] sets an expiry that reads respect and
//!   [`BlobStore::cleanup_expired`] applies. Expiries live in memory and in
//!   index snapshots only: after a restart, a key keeps its expiry only if its
//!   record has not changed since the last [`BlobStore::save_snapshot`].
//! - LZ4 compression, off by default ([`BlobStore::set_compression`]).
//!
//! Compaction is not implemented: [`BlobStore::compact`] fails, and the
//! segments and the WAL only grow.

use crate::common::{blake3_hash, Result, WalSyncPolicy};
use crate::ops::NotImplemented;
use crate::volume::index::{BlobLocation, Index};
use crate::volume::wal::{Wal, WalEntry, WalOp};
use bloomfilter::Bloom;
use std::collections::{HashMap, VecDeque};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

const BLOB_MAGIC: [u8; 4] = [0x42, 0x4C, 0x4F, 0x42];
const BLOB_MAGIC_COMPRESSED: [u8; 4] = [0x42, 0x4C, 0x4F, 0x43]; // BLOC
const SEGMENT_SIZE: u64 = 64 * 1024 * 1024;
const MAX_SEGMENTS: u64 = 1000;
const COMPRESSION_THRESHOLD: usize = 128;
/// Magic, key length, stored value length and original value length.
const HEADER_LEN: u64 = 4 + 4 + 8 + 8;
const CHECKSUM_LEN: u64 = 4;

#[derive(Debug, Clone)]
pub struct StoreStats {
    pub total_keys: usize,
    pub total_bytes: u64,
    pub active_segments: usize,
    pub index_size: usize,
    /// Not measured: always 0.
    #[deprecated(since = "2.0.1", note = "not measured: always 0")]
    pub bloom_false_positives: u64,
    pub keys_with_ttl: usize,
    /// Not measured: always 0.
    #[deprecated(since = "2.0.1", note = "not measured: always 0")]
    pub compressed_blobs: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CompressionMode {
    #[default]
    None,
    Lz4,
}

pub struct BlobStore {
    data_path: PathBuf,

    index: Index,
    bloom: Bloom<[u8; 32]>,
    wal: Wal,
    current_segment: u64,
    current_offset: u64,
    sync_policy: WalSyncPolicy,
    compression: CompressionMode,
}

/// A record found while scanning the segments.
struct ScannedRecord {
    location: BlobLocation,
    checksum: u32,
    compressed: bool,
}

impl ScannedRecord {
    /// Whether this record stores the value of a put, given the checksum that
    /// the put's record would have uncompressed and the size of its value. A
    /// compressed record can only be matched by its size.
    fn holds(&self, checksum: u32, size: u64) -> bool {
        if self.compressed {
            self.location.size == size
        } else {
            self.checksum == checksum
        }
    }
}

/// The records of each key, in the order they were written.
type Records = HashMap<String, VecDeque<ScannedRecord>>;

/// Where the scan of a segment stopped.
enum SegmentEnd {
    /// After its last record.
    Clean(u64),
    /// At a record cut short or unreadable.
    Damaged(u64),
}

impl BlobStore {
    /// Opens the store and rebuilds its index from the segments and the WAL.
    ///
    /// Each segment record comes from a put and is written right after the
    /// put's WAL entry, and the WAL is never truncated. Replaying the WAL
    /// therefore gives each put the next record of its key that holds its
    /// value, and each delete removes its key. A put without such a record was
    /// interrupted before its segment write and never acknowledged: its key
    /// keeps its previous state. Records that the WAL does not cover, because
    /// it was lost or damaged, are kept, the last one of each key winning,
    /// since dropping them could lose data. A segment that ends with a record
    /// cut short is read up to that record, and new records go to a new
    /// segment.
    ///
    /// An index snapshot, if one exists, only brings back the expiry of the
    /// keys whose record has not changed since it was saved.
    pub fn open(data_path: &Path, wal_path: &Path, sync_policy: WalSyncPolicy) -> Result<Self> {
        fs::create_dir_all(data_path)?;
        fs::create_dir_all(wal_path)?;

        let (mut records, (current_segment, current_offset)) = Self::scan_segments(data_path)?;

        let wal_file = wal_path.join("wal.log");
        let wal = Wal::open(&wal_file, sync_policy)?;

        // The state of every key the WAL mentions: `None` once deleted.
        let mut state: HashMap<String, Option<BlobLocation>> = HashMap::new();
        let mut interrupted = 0usize;
        let mut skipped = 0usize;
        Wal::replay(&wal_file, |entry: WalEntry| {
            match entry.op {
                WalOp::Put { key, value } => {
                    let size = value.len() as u64;
                    let checksum = record_checksum(key.as_bytes(), &value, size);
                    let record = records.get_mut(&key).and_then(|queue| {
                        let position = queue.iter().position(|r| r.holds(checksum, size))?;
                        // The earlier records of this key were written before
                        // this put's record, which supersedes them.
                        skipped += position;
                        queue.drain(..position);
                        queue.pop_front()
                    });
                    match record {
                        Some(record) => {
                            let mut location = record.location;
                            location.blake3 = blake3_hash(&value);
                            state.insert(key, Some(location));
                        }
                        None => interrupted += 1,
                    }
                }
                WalOp::Delete { key } => {
                    state.insert(key, None);
                }
            }
            Ok(())
        })?;

        let mut uncovered = 0usize;
        for (key, mut queue) in records {
            let Some(record) = queue.pop_back() else {
                continue;
            };
            uncovered += queue.len() + 1;
            let mut location = record.location;
            location.blake3 = match Self::read_record(data_path, &location) {
                Ok(Some(value)) => blake3_hash(&value),
                _ => String::new(),
            };
            state.insert(key, Some(location));
        }
        if interrupted > 0 {
            tracing::warn!(
                "{} put(s) of the WAL have no segment record: interrupted before their \
                 segment write, they are ignored",
                interrupted
            );
        }
        if skipped > 0 {
            tracing::warn!(
                "{} segment record(s) match no put of the WAL: ignored",
                skipped
            );
        }
        if uncovered > 0 {
            tracing::warn!(
                "{} segment record(s) are missing from the WAL: kept, the last one of \
                 each key winning",
                uncovered
            );
        }

        let snapshot_path = data_path.join("index.snap");
        if snapshot_path.exists() {
            match Index::load_snapshot(&snapshot_path) {
                Ok(snapshot) => {
                    for (key, saved) in snapshot.iter() {
                        if let Some(Some(location)) = state.get_mut(key) {
                            if location.segment() == saved.segment()
                                && location.offset == saved.offset
                            {
                                location.expires_at = saved.expires_at;
                            }
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Ignoring the index snapshot {}: {}",
                        snapshot_path.display(),
                        e
                    )
                }
            }
        }

        let mut index = Index::new();
        let mut bloom = Bloom::new_for_fp_rate(100_000, 0.01).unwrap();
        for (key, location) in state {
            if let Some(location) = location {
                bloom.set(&bloom_key(&key));
                index.insert(key, location);
            }
        }

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

    pub fn set_compression(&mut self, mode: CompressionMode) {
        self.compression = mode;
    }

    pub fn compression_mode(&self) -> CompressionMode {
        self.compression
    }

    pub fn put_with_ttl(&mut self, key: &str, value: &[u8], ttl_ms: Option<u64>) -> Result<()> {
        self.wal.append_put(key, value)?;
        self.bloom.set(&bloom_key(key));

        let mut location = self.write_blob(key, value)?;

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
        if !self.bloom.check(&bloom_key(key)) {
            return Ok(None);
        }

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

    /// Not implemented: fails without touching the data.
    ///
    /// Compaction must rewrite the live records and shorten the WAL in a way
    /// that [`open`](Self::open) can still rebuild the index from. It is
    /// planned with the other cluster operations.
    pub fn compact(&mut self) -> Result<()> {
        Err(NotImplemented::COMPACT.into())
    }

    /// Writes the index, with the expiries, and the Bloom filter to the data
    /// directory. [`open`](Self::open) only reads the expiries back.
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

    /// Removes the expired keys from the index. The removal is not logged:
    /// after a restart, such a key is back, without its expiry unless a
    /// snapshot restores it.
    pub fn cleanup_expired(&mut self) -> usize {
        self.index.cleanup_expired()
    }

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

    pub fn exists(&self, key: &str) -> bool {
        self.index.get_if_valid(key).is_some()
    }

    #[allow(deprecated)]
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
            compressed_blobs: 0,
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

    fn write_blob_to_segment(
        &self,
        base_path: &Path,
        segment: u64,
        offset: u64,
        key: &str,
        value: &[u8],
    ) -> Result<(BlobLocation, u64)> {
        let segment_file = segment_path(base_path, segment);
        if let Some(segment_dir) = segment_file.parent() {
            fs::create_dir_all(segment_dir)?;
        }
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .read(true)
            .truncate(false)
            .open(&segment_file)?;
        file.seek(SeekFrom::Start(offset))?;
        let mut writer = BufWriter::new(&file);

        let (write_value, is_compressed) =
            if self.compression == CompressionMode::Lz4 && value.len() >= COMPRESSION_THRESHOLD {
                match lz4::block::compress(value, None, true) {
                    Ok(compressed) if compressed.len() < value.len() => (compressed, true),
                    _ => (value.to_vec(), false), // Fallback to uncompressed if compression doesn't help
                }
            } else {
                (value.to_vec(), false)
            };

        let magic = if is_compressed {
            BLOB_MAGIC_COMPRESSED
        } else {
            BLOB_MAGIC
        };
        writer.write_all(&magic)?;

        writer.write_all(&(key.len() as u32).to_le_bytes())?;
        writer.write_all(&(write_value.len() as u64).to_le_bytes())?;
        writer.write_all(&(value.len() as u64).to_le_bytes())?;
        writer.write_all(key.as_bytes())?;
        writer.write_all(&write_value)?;

        let checksum = record_checksum(key.as_bytes(), &write_value, value.len() as u64);
        writer.write_all(&checksum.to_le_bytes())?;
        writer.flush()?;

        if self.sync_policy == WalSyncPolicy::Always {
            file.sync_all()?;
        }

        let bytes_written = HEADER_LEN + key.len() as u64 + write_value.len() as u64 + CHECKSUM_LEN;

        let blake3 = blake3_hash(value);
        Ok((
            BlobLocation {
                shard: segment,
                offset,
                size: value.len() as u64,
                blake3,
                expires_at: None,
            },
            bytes_written,
        ))
    }

    fn read_blob(&self, location: &BlobLocation) -> Result<Option<Vec<u8>>> {
        Self::read_record(&self.data_path, location)
    }

    /// The value of the record at `location`, once its CRC32 is checked.
    /// `None` when the segment file does not exist.
    fn read_record(data_path: &Path, location: &BlobLocation) -> Result<Option<Vec<u8>>> {
        let segment_file = segment_path(data_path, location.segment());
        if !segment_file.exists() {
            return Ok(None);
        }

        let file = File::open(&segment_file)?;
        let mut reader = BufReader::new(file);
        reader.seek(SeekFrom::Start(location.offset))?;

        let mut magic = [0u8; 4];
        reader.read_exact(&mut magic)?;

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
        let computed_checksum = record_checksum(&key_bytes, &value, orig_len as u64);

        if computed_checksum != stored_checksum {
            return Err(crate::Error::ChecksumMismatch {
                expected: format!("{:08x}", stored_checksum),
                actual: format!("{:08x}", computed_checksum),
            });
        }

        if is_compressed {
            // The compressed block starts with the value size, which
            // `write_blob_to_segment` asks LZ4 to prepend.
            match lz4::block::decompress(&value, None) {
                Ok(decompressed) if decompressed.len() == orig_len => Ok(Some(decompressed)),
                _ => Err(crate::Error::Corrupted("LZ4 decompression failed".into())),
            }
        } else {
            Ok(Some(value))
        }
    }

    /// Scans every segment, in segment order. Returns the records of each key
    /// and the segment and offset where the next record goes.
    fn scan_segments(data_path: &Path) -> Result<(Records, (u64, u64))> {
        let mut records = Records::new();
        let mut next = (0, 0);
        for (segment, path) in Self::segment_files(data_path)? {
            next = match Self::scan_segment(segment, &path, &mut records)? {
                SegmentEnd::Clean(end) => (segment, end),
                SegmentEnd::Damaged(at) => {
                    tracing::warn!(
                        "{} ends with a record cut short or unreadable at offset {}: \
                         the rest of the file is ignored",
                        path.display(),
                        at
                    );
                    (segment + 1, 0)
                }
            };
        }
        Ok((records, next))
    }

    /// The segment files of the data directory, sorted by segment number.
    fn segment_files(data_path: &Path) -> Result<Vec<(u64, PathBuf)>> {
        let mut files = Vec::new();
        for first in fs::read_dir(data_path)? {
            let first = first?.path();
            if !first.is_dir() {
                continue;
            }
            for second in fs::read_dir(&first)? {
                let second = second?.path();
                if !second.is_dir() {
                    continue;
                }
                for file in fs::read_dir(&second)? {
                    let path = file?.path();
                    let segment = path
                        .file_name()
                        .and_then(|name| name.to_str())
                        .and_then(|name| name.strip_prefix("seg_"))
                        .and_then(|name| name.strip_suffix(".blob"))
                        .and_then(|number| number.parse::<u64>().ok());
                    if let Some(segment) = segment {
                        files.push((segment, path));
                    }
                }
            }
        }
        files.sort();
        Ok(files)
    }

    /// Adds the records of one segment file to `records`.
    fn scan_segment(segment: u64, path: &Path, records: &mut Records) -> Result<SegmentEnd> {
        let file = File::open(path)?;
        let length = file.metadata()?.len();
        let mut reader = BufReader::new(file);
        let mut offset = 0u64;

        while offset < length {
            if length - offset < HEADER_LEN {
                return Ok(SegmentEnd::Damaged(offset));
            }
            let mut header = [0u8; HEADER_LEN as usize];
            reader.read_exact(&mut header)?;
            let magic: [u8; 4] = header[0..4].try_into().unwrap();
            let compressed = magic == BLOB_MAGIC_COMPRESSED;
            if magic != BLOB_MAGIC && !compressed {
                return Ok(SegmentEnd::Damaged(offset));
            }
            let key_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as u64;
            let stored_len = u64::from_le_bytes(header[8..16].try_into().unwrap());
            let original_len = u64::from_le_bytes(header[16..24].try_into().unwrap());

            let end = HEADER_LEN
                .checked_add(key_len)
                .and_then(|n| n.checked_add(stored_len))
                .and_then(|n| n.checked_add(CHECKSUM_LEN))
                .and_then(|n| n.checked_add(offset));
            let Some(end) = end.filter(|&end| end <= length) else {
                return Ok(SegmentEnd::Damaged(offset));
            };

            let mut key = vec![0u8; key_len as usize];
            reader.read_exact(&mut key)?;
            reader.seek_relative(stored_len as i64)?;
            let mut checksum = [0u8; 4];
            reader.read_exact(&mut checksum)?;

            records
                .entry(String::from_utf8_lossy(&key).into_owned())
                .or_default()
                .push_back(ScannedRecord {
                    location: BlobLocation {
                        shard: segment,
                        offset,
                        size: original_len,
                        blake3: String::new(),
                        expires_at: None,
                    },
                    checksum: u32::from_le_bytes(checksum),
                    compressed,
                });
            offset = end;
        }

        Ok(SegmentEnd::Clean(offset))
    }
}

/// Segment `n` is `{n % 100}/{n / 100}/seg_{n}.blob` under `base`, with
/// two-digit directory names and a four-digit file number.
fn segment_path(base: &Path, segment: u64) -> PathBuf {
    base.join(format!("{:02}", segment % 100))
        .join(format!("{:02}", segment / 100))
        .join(format!("seg_{:04}.blob", segment))
}

/// CRC32 of a record as laid out on disk: key length, stored value length,
/// original value length, key and stored value.
fn record_checksum(key: &[u8], stored: &[u8], original_len: u64) -> u32 {
    let mut hasher = crc32fast::Hasher::new();
    hasher.update(&(key.len() as u32).to_le_bytes());
    hasher.update(&(stored.len() as u64).to_le_bytes());
    hasher.update(&original_len.to_le_bytes());
    hasher.update(key);
    hasher.update(stored);
    hasher.finalize()
}

/// The Bloom filter entry of `key`: its BLAKE3 digest.
fn bloom_key(key: &str) -> [u8; 32] {
    *blake3::hash(key.as_bytes()).as_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::{tempdir, TempDir};

    fn open(dir: &TempDir) -> BlobStore {
        BlobStore::open(
            &dir.path().join("data"),
            &dir.path().join("wal"),
            WalSyncPolicy::Always,
        )
        .unwrap()
    }

    fn value(store: &BlobStore, key: &str) -> Option<Vec<u8>> {
        store.get(key).unwrap()
    }

    #[test]
    fn record_checksum_covers_the_on_disk_layout() {
        let (key, stored, original_len) = (b"key".as_slice(), b"stored".as_slice(), 42u64);
        let mut layout = Vec::new();
        layout.extend_from_slice(&(key.len() as u32).to_le_bytes());
        layout.extend_from_slice(&(stored.len() as u64).to_le_bytes());
        layout.extend_from_slice(&original_len.to_le_bytes());
        layout.extend_from_slice(key);
        layout.extend_from_slice(stored);
        assert_eq!(
            record_checksum(key, stored, original_len),
            crate::common::crc32(&layout)
        );
    }

    #[test]
    fn compressed_records_are_read_back_and_rebuilt() {
        let dir = tempdir().unwrap();
        let mut store = open(&dir);
        store.set_compression(CompressionMode::Lz4);
        let (old, new) = (vec![b'a'; 1000], vec![b'b'; 2000]);
        store.put("key", &old).unwrap();
        assert_eq!(value(&store, "key").unwrap(), old);
        store.put("key", &new).unwrap();
        store.put("deleted", &old).unwrap();
        store.delete("deleted").unwrap();
        drop(store);

        let store = open(&dir);
        assert_eq!(value(&store, "key").unwrap(), new);
        assert_eq!(value(&store, "deleted"), None);
        assert_eq!(store.index.get("key").unwrap().blake3, blake3_hash(&new));
    }

    #[test]
    fn the_last_record_of_a_key_wins_across_segments() {
        let dir = tempdir().unwrap();
        let mut store = open(&dir);
        // Segment 99 is 99/00/seg_0099.blob and segment 100 is
        // 00/01/seg_0100.blob: in path order, 100 would come first.
        store.current_segment = 99;
        store.put("key", b"in segment 99").unwrap();
        store.current_offset = SEGMENT_SIZE + 1;
        store.put("key", b"in segment 100").unwrap();
        assert_eq!(store.current_segment, 100);
        drop(store);

        let mut store = open(&dir);
        assert_eq!(value(&store, "key").unwrap(), b"in segment 100");
        assert_eq!(
            (store.current_segment, store.current_offset > 0),
            (100, true)
        );

        // New records go after the last one, and survive the next restart.
        store.put("other", b"after the restart").unwrap();
        drop(store);
        let store = open(&dir);
        assert_eq!(value(&store, "key").unwrap(), b"in segment 100");
        assert_eq!(value(&store, "other").unwrap(), b"after the restart");
    }

    #[test]
    fn the_index_keeps_the_blake3_of_each_value_after_a_restart() {
        let dir = tempdir().unwrap();
        let mut store = open(&dir);
        store.put("key", b"value").unwrap();
        store.put("other", b"other value").unwrap();
        drop(store);

        let store = open(&dir);
        assert_eq!(
            store.index.get("key").unwrap().blake3,
            blake3_hash(b"value")
        );
        assert_eq!(
            store.index.get("other").unwrap().blake3,
            blake3_hash(b"other value")
        );
    }

    #[test]
    fn a_put_interrupted_before_its_segment_write_leaves_the_key_as_it_was() {
        let dir = tempdir().unwrap();
        let mut store = open(&dir);
        store.put("key", b"old").unwrap();
        // The process stops between the WAL entry of a put and its segment
        // write.
        store.wal.append_put("key", b"lost").unwrap();
        drop(store);

        let mut store = open(&dir);
        assert_eq!(value(&store, "key").unwrap(), b"old");

        // The later writes of the key are not paired with the lost one.
        store.put("key", b"new").unwrap();
        store.delete("key").unwrap();
        store.put("key", b"newest").unwrap();
        drop(store);

        let store = open(&dir);
        assert_eq!(value(&store, "key").unwrap(), b"newest");
        assert_eq!(
            store.index.get("key").unwrap().blake3,
            blake3_hash(b"newest")
        );
    }

    #[test]
    fn records_missing_from_the_wal_are_kept() {
        let dir = tempdir().unwrap();
        let mut store = open(&dir);
        store.put("key", b"value").unwrap();
        store.put("deleted", b"value").unwrap();
        store.delete("deleted").unwrap();
        drop(store);
        fs::remove_file(dir.path().join("wal").join("wal.log")).unwrap();

        // Without the WAL, the deletion is unknown: both records come back,
        // with the BLAKE3 of their value.
        let store = open(&dir);
        assert_eq!(value(&store, "key").unwrap(), b"value");
        assert_eq!(value(&store, "deleted").unwrap(), b"value");
        assert_eq!(
            store.index.get("key").unwrap().blake3,
            blake3_hash(b"value")
        );
    }

    #[test]
    fn compact_fails_without_touching_the_data() {
        let dir = tempdir().unwrap();
        let mut store = open(&dir);
        store.put("key", b"value").unwrap();

        let error = store.compact().unwrap_err();
        assert_eq!(
            error.to_string(),
            "compact is not implemented (roadmap: 2.2.0)"
        );
        assert_eq!(value(&store, "key").unwrap(), b"value");
        assert!(!dir.path().join("data").join("compact_temp").exists());
    }
}
