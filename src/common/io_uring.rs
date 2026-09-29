//! Configuration and request queue for an io_uring backend that minikv does
//! not have.
//!
//! Nothing in minikv uses this module, and it performs no asynchronous I/O:
//! [`IoUring::new`] and [`UringFile::open`] fail when `enabled` is set,
//! [`IoUring::flush`] answers every queued request with `-ENOSYS`, and
//! [`IoUring::is_enabled`] is always `false`. [`UringFile`] reads and writes
//! with standard blocking I/O.

use crate::common::{Error, Result};
use crate::ops::NotImplemented;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// Settings of the io_uring backend. Only `enabled` has an effect, and it
/// must stay `false`: minikv has no io_uring backend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoUringConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default = "default_sq_depth")]
    pub sq_depth: u32,

    #[serde(default = "default_cq_depth")]
    pub cq_depth: u32,

    #[serde(default)]
    pub kernel_poll: bool,

    #[serde(default = "default_poll_idle")]
    pub poll_idle_ms: u32,

    #[serde(default = "default_true")]
    pub registered_buffers: bool,

    #[serde(default = "default_buffer_count")]
    pub buffer_count: usize,

    #[serde(default = "default_buffer_size")]
    pub buffer_size: usize,

    #[serde(default)]
    pub direct_io: bool,

    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
}

fn default_sq_depth() -> u32 {
    256
}

fn default_cq_depth() -> u32 {
    512
}

fn default_poll_idle() -> u32 {
    1000
}

fn default_buffer_count() -> usize {
    64
}

fn default_buffer_size() -> usize {
    64 * 1024 // 64 KB
}

fn default_batch_size() -> usize {
    32
}

fn default_true() -> bool {
    true
}

impl Default for IoUringConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            sq_depth: default_sq_depth(),
            cq_depth: default_cq_depth(),
            kernel_poll: false,
            poll_idle_ms: default_poll_idle(),
            registered_buffers: true,
            buffer_count: default_buffer_count(),
            buffer_size: default_buffer_size(),
            direct_io: false,
            batch_size: default_batch_size(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoOpType {
    Read,
    Write,
    Fsync,
    Fdatasync,
}

#[derive(Debug)]
pub struct IoRequest {
    pub op: IoOpType,

    pub fd: i32,

    pub offset: u64,

    pub buffer: Vec<u8>,

    pub user_data: u64,
}

/// `ENOSYS` ("function not implemented") on Linux, the only system with
/// io_uring.
const ENOSYS: i32 = 38;

#[derive(Debug)]
pub struct IoResult {
    pub user_data: u64,

    /// What io_uring would return: the number of bytes transferred, or a
    /// negative error number. Always `-ENOSYS` (-38) here.
    pub result: i32,

    pub op: IoOpType,
}

/// A queue of I/O requests that are never executed: see the module
/// documentation.
pub struct IoUring {
    pending: VecDeque<IoRequest>,

    stats: Arc<IoUringStats>,
}

/// Counters of an [`IoUring`] queue.
#[derive(Debug, Default)]
pub struct IoUringStats {
    /// Requests queued by the `submit_*` methods.
    pub submissions: AtomicU64,

    /// Results returned by [`IoUring::flush`], all `-ENOSYS`.
    pub completions: AtomicU64,

    /// Never updated: no request reads anything.
    pub bytes_read: AtomicU64,

    /// Never updated: no request writes anything.
    pub bytes_written: AtomicU64,

    /// Never updated: nothing is submitted to the kernel.
    pub batched_submissions: AtomicU64,

    /// Never updated: nothing is submitted to the kernel.
    pub avg_batch_size: AtomicU64,

    /// Never updated: there is no kernel polling thread.
    pub poll_wakeups: AtomicU64,
}

impl IoUring {
    /// Creates an empty queue. Fails when `config.enabled` is set: minikv has
    /// no io_uring backend.
    pub fn new(config: IoUringConfig) -> Result<Self> {
        if config.enabled {
            return Err(NotImplemented::IO_URING.into());
        }

        Ok(Self {
            pending: VecDeque::new(),
            stats: Arc::new(IoUringStats::default()),
        })
    }

    /// Always `false`: minikv has no io_uring backend.
    pub fn is_enabled(&self) -> bool {
        false
    }

    pub fn submit_read(&mut self, fd: i32, offset: u64, len: usize, user_data: u64) {
        let request = IoRequest {
            op: IoOpType::Read,
            fd,
            offset,
            buffer: vec![0u8; len],
            user_data,
        };

        self.pending.push_back(request);
        self.stats.submissions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn submit_write(&mut self, fd: i32, offset: u64, data: Vec<u8>, user_data: u64) {
        let request = IoRequest {
            op: IoOpType::Write,
            fd,
            offset,
            buffer: data,
            user_data,
        };

        self.pending.push_back(request);
        self.stats.submissions.fetch_add(1, Ordering::Relaxed);
    }

    pub fn submit_fsync(&mut self, fd: i32, user_data: u64) {
        let request = IoRequest {
            op: IoOpType::Fsync,
            fd,
            offset: 0,
            buffer: vec![],
            user_data,
        };

        self.pending.push_back(request);
        self.stats.submissions.fetch_add(1, Ordering::Relaxed);
    }

    /// Empties the queue and answers every request with `-ENOSYS`, the error
    /// io_uring gives for an unsupported operation: nothing is read, written
    /// or synced.
    pub fn flush(&mut self) -> Vec<IoResult> {
        let results: Vec<IoResult> = self
            .pending
            .drain(..)
            .map(|request| IoResult {
                user_data: request.user_data,
                result: -ENOSYS,
                op: request.op,
            })
            .collect();

        self.stats
            .completions
            .fetch_add(results.len() as u64, Ordering::Relaxed);

        results
    }

    pub fn stats(&self) -> &IoUringStats {
        &self.stats
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }
}

/// A file read and written with standard blocking I/O.
#[allow(dead_code)]
pub struct UringFile {
    path: PathBuf,
    file: Option<File>,
    direct_io: bool,
}

impl UringFile {
    /// Opens or creates the file. Fails when `config.enabled` is set: minikv
    /// has no io_uring backend.
    pub fn open(path: PathBuf, config: &IoUringConfig) -> Result<Self> {
        if config.enabled {
            return Err(NotImplemented::IO_URING.into());
        }

        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&path)
            .map_err(|e| Error::Internal(format!("Failed to open file: {}", e)))?;

        Ok(Self {
            path,
            file: Some(file),
            direct_io: config.direct_io,
        })
    }

    pub fn read_at(&mut self, offset: u64, len: usize) -> Result<Vec<u8>> {
        if let Some(ref mut file) = self.file {
            let mut buffer = vec![0u8; len];
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| Error::Internal(format!("Seek failed: {}", e)))?;
            file.read_exact(&mut buffer)
                .map_err(|e| Error::Internal(format!("Read failed: {}", e)))?;
            Ok(buffer)
        } else {
            Err(Error::Internal("File not open".to_string()))
        }
    }

    pub fn write_at(&mut self, offset: u64, data: &[u8]) -> Result<()> {
        if let Some(ref mut file) = self.file {
            file.seek(SeekFrom::Start(offset))
                .map_err(|e| Error::Internal(format!("Seek failed: {}", e)))?;
            file.write_all(data)
                .map_err(|e| Error::Internal(format!("Write failed: {}", e)))?;
            Ok(())
        } else {
            Err(Error::Internal("File not open".to_string()))
        }
    }

    pub fn sync(&mut self) -> Result<()> {
        if let Some(ref file) = self.file {
            file.sync_all()
                .map_err(|e| Error::Internal(format!("Sync failed: {}", e)))?;
        }
        Ok(())
    }

    /// Does nothing: there is no asynchronous I/O.
    #[deprecated(
        since = "2.0.1",
        note = "does nothing: minikv has no io_uring backend; use read_at"
    )]
    pub fn async_read(&mut self, _offset: u64, _len: usize, _user_data: u64) {}

    /// Does nothing: there is no asynchronous I/O.
    #[deprecated(
        since = "2.0.1",
        note = "does nothing: minikv has no io_uring backend; use write_at"
    )]
    pub fn async_write(&mut self, _offset: u64, _data: Vec<u8>, _user_data: u64) {}

    /// Always empty: there is no asynchronous I/O.
    #[deprecated(since = "2.0.1", note = "always empty: minikv has no io_uring backend")]
    pub fn flush_async(&mut self) -> Vec<IoResult> {
        Vec::new()
    }
}

pub struct WriteBatcher {
    buffer: Vec<(u64, Vec<u8>)>, // (offset, data)
    max_entries: usize,
    max_bytes: usize,
    current_bytes: usize,
}

impl WriteBatcher {
    pub fn new(max_entries: usize, max_bytes: usize) -> Self {
        Self {
            buffer: Vec::with_capacity(max_entries),
            max_entries,
            max_bytes,
            current_bytes: 0,
        }
    }

    pub fn add(&mut self, offset: u64, data: Vec<u8>) -> bool {
        let data_len = data.len();

        if self.buffer.len() >= self.max_entries || self.current_bytes + data_len > self.max_bytes {
            return false;
        }

        self.buffer.push((offset, data));
        self.current_bytes += data_len;
        true
    }

    pub fn is_full(&self) -> bool {
        self.buffer.len() >= self.max_entries || self.current_bytes >= self.max_bytes
    }

    pub fn take(&mut self) -> Vec<(u64, Vec<u8>)> {
        self.current_bytes = 0;
        std::mem::take(&mut self.buffer)
    }

    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IoUringStatsSnapshot {
    pub submissions: u64,
    pub completions: u64,
    pub bytes_read: u64,
    pub bytes_written: u64,
    pub batched_submissions: u64,
    pub avg_batch_size: u64,
    pub poll_wakeups: u64,
}

impl From<&IoUringStats> for IoUringStatsSnapshot {
    fn from(stats: &IoUringStats) -> Self {
        Self {
            submissions: stats.submissions.load(Ordering::Relaxed),
            completions: stats.completions.load(Ordering::Relaxed),
            bytes_read: stats.bytes_read.load(Ordering::Relaxed),
            bytes_written: stats.bytes_written.load(Ordering::Relaxed),
            batched_submissions: stats.batched_submissions.load(Ordering::Relaxed),
            avg_batch_size: stats.avg_batch_size.load(Ordering::Relaxed),
            poll_wakeups: stats.poll_wakeups.load(Ordering::Relaxed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_io_uring_config() {
        let config = IoUringConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.sq_depth, 256);
        assert_eq!(config.buffer_size, 64 * 1024);
    }

    #[test]
    fn test_io_uring_creation() {
        let config = IoUringConfig::default();
        let uring = IoUring::new(config).unwrap();
        assert_eq!(uring.pending_count(), 0);
    }

    #[test]
    fn enabling_io_uring_fails() {
        let config = IoUringConfig {
            enabled: true,
            ..Default::default()
        };
        let message = "io_uring is not implemented (roadmap: unscheduled)";
        assert_eq!(
            IoUring::new(config.clone()).err().unwrap().to_string(),
            message
        );

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        let error = UringFile::open(path.clone(), &config).err().unwrap();
        assert_eq!(error.to_string(), message);
        assert!(!path.exists());
    }

    #[test]
    fn flush_answers_enosys_without_doing_any_io() {
        let mut uring = IoUring::new(IoUringConfig::default()).unwrap();
        assert!(!uring.is_enabled());
        uring.submit_read(3, 0, 4096, 1);
        uring.submit_write(3, 0, vec![1, 2, 3], 2);
        uring.submit_fsync(3, 3);

        let results = uring.flush();
        let answers: Vec<(u64, IoOpType, i32)> = results
            .iter()
            .map(|r| (r.user_data, r.op, r.result))
            .collect();
        assert_eq!(
            answers,
            [
                (1, IoOpType::Read, -38),
                (2, IoOpType::Write, -38),
                (3, IoOpType::Fsync, -38),
            ]
        );
        assert_eq!(uring.pending_count(), 0);

        let stats = IoUringStatsSnapshot::from(uring.stats());
        assert_eq!(stats.submissions, 3);
        assert_eq!(stats.completions, 3);
        assert_eq!(stats.bytes_read, 0);
        assert_eq!(stats.bytes_written, 0);
        assert_eq!(stats.batched_submissions, 0);
    }

    #[test]
    fn uring_file_reads_and_writes_with_blocking_io() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("file");
        let mut file = UringFile::open(path.clone(), &IoUringConfig::default()).unwrap();
        file.write_at(4, b"minikv").unwrap();
        file.sync().unwrap();

        assert_eq!(file.read_at(4, 6).unwrap(), b"minikv");
        assert_eq!(std::fs::read(&path).unwrap(), b"\0\0\0\0minikv");
    }

    #[test]
    fn test_write_batcher() {
        let mut batcher = WriteBatcher::new(10, 1024);

        assert!(batcher.add(0, vec![1, 2, 3]));
        assert!(batcher.add(100, vec![4, 5, 6]));
        assert_eq!(batcher.len(), 2);

        let batch = batcher.take();
        assert_eq!(batch.len(), 2);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_write_batcher_full() {
        let mut batcher = WriteBatcher::new(2, 1024);

        assert!(batcher.add(0, vec![1]));
        assert!(batcher.add(1, vec![2]));
        assert!(batcher.is_full());
        assert!(!batcher.add(2, vec![3]));
    }

    #[test]
    fn test_stats_snapshot() {
        let stats = IoUringStats::default();
        stats.submissions.store(100, Ordering::Relaxed);
        stats.completions.store(95, Ordering::Relaxed);
        stats.bytes_read.store(1024 * 1024, Ordering::Relaxed);

        let snapshot = IoUringStatsSnapshot::from(&stats);
        assert_eq!(snapshot.submissions, 100);
        assert_eq!(snapshot.completions, 95);
        assert_eq!(snapshot.bytes_read, 1024 * 1024);
    }
}
