//! A volume keeps exactly its live blobs across restarts: deleted keys stay
//! deleted, the last write of a key wins, writes made after an index snapshot
//! are not lost, and a record cut short by a crash does not stop the restart.

mod support;

use minikv::common::WalSyncPolicy;
use minikv::volume::blob::BlobStore;
use reqwest::StatusCode;
use std::fs::{self, OpenOptions};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use support::{json, Cluster};
use tempfile::TempDir;

struct Paths {
    data: PathBuf,
    wal: PathBuf,
    _dir: TempDir,
}

fn paths() -> Paths {
    let dir = TempDir::new().unwrap();
    Paths {
        data: dir.path().join("data"),
        wal: dir.path().join("wal"),
        _dir: dir,
    }
}

fn open(paths: &Paths) -> BlobStore {
    BlobStore::open(&paths.data, &paths.wal, WalSyncPolicy::Always).unwrap()
}

fn value(store: &BlobStore, key: &str) -> Option<Vec<u8>> {
    store.get(key).unwrap()
}

#[test]
fn deleted_keys_stay_deleted_after_a_restart() {
    let paths = paths();
    {
        let mut store = open(&paths);
        for key in ["a", "b", "c"] {
            store.put(key, key.as_bytes()).unwrap();
        }
        store.delete("b").unwrap();
    }

    for _ in 0..2 {
        let store = open(&paths);
        assert_eq!(value(&store, "a").unwrap(), b"a");
        assert_eq!(value(&store, "b"), None);
        assert_eq!(value(&store, "c").unwrap(), b"c");
        assert_eq!(store.stats().total_keys, 2);
    }
}

#[test]
fn the_last_write_of_a_key_wins_after_a_restart() {
    let paths = paths();
    {
        let mut store = open(&paths);
        store.put("key", b"first").unwrap();
        store.put("key", b"second").unwrap();
        store.delete("key").unwrap();
        store.put("key", b"third").unwrap();
    }

    let store = open(&paths);
    assert_eq!(value(&store, "key").unwrap(), b"third");
    assert_eq!(store.stats().total_keys, 1);
}

#[test]
fn writes_after_a_snapshot_survive_a_restart() {
    let paths = paths();
    {
        let mut store = open(&paths);
        store.put("before", b"written before the snapshot").unwrap();
        store.put("rewritten", b"old").unwrap();
        store.save_snapshot().unwrap();
        store.put("after", b"written after the snapshot").unwrap();
        store.put("rewritten", b"new").unwrap();
        store.delete("before").unwrap();
    }

    let store = open(&paths);
    assert_eq!(value(&store, "before"), None);
    assert_eq!(
        value(&store, "after").unwrap(),
        b"written after the snapshot"
    );
    assert_eq!(value(&store, "rewritten").unwrap(), b"new");
    assert_eq!(store.stats().total_keys, 2);
}

#[test]
fn a_snapshot_keeps_the_expiry_of_unchanged_keys() {
    let paths = paths();
    let hour = 3_600_000;
    {
        let mut store = open(&paths);
        store
            .put_with_ttl("expiring", b"value", Some(hour))
            .unwrap();
        store
            .put_with_ttl("rewritten", b"value", Some(hour))
            .unwrap();
        store.put("plain", b"value").unwrap();
        store.save_snapshot().unwrap();
        store
            .put("rewritten", b"new value, without expiry")
            .unwrap();
    }

    let store = open(&paths);
    let ttl = store.get_ttl("expiring").unwrap();
    assert!(ttl > 0 && ttl <= hour, "{}", ttl);
    assert_eq!(store.get_ttl("plain"), None);
    assert_eq!(store.get_ttl("rewritten"), None);
    assert_eq!(
        value(&store, "rewritten").unwrap(),
        b"new value, without expiry"
    );
}

#[test]
fn a_record_cut_short_by_a_crash_does_not_stop_the_restart() {
    let paths = paths();
    {
        let mut store = open(&paths);
        store.put("kept", b"kept value").unwrap();
        store.put("cut", b"value cut short by the crash").unwrap();
    }
    // The crash happens while the second record is being written.
    let segment = paths.data.join("00/00/seg_0000.blob");
    let length = fs::metadata(&segment).unwrap().len();
    OpenOptions::new()
        .write(true)
        .open(&segment)
        .unwrap()
        .set_len(length - 10)
        .unwrap();

    {
        let mut store = open(&paths);
        assert_eq!(value(&store, "kept").unwrap(), b"kept value");
        assert_eq!(value(&store, "cut"), None);
        store.put("after", b"written after the restart").unwrap();
    }

    let store = open(&paths);
    assert_eq!(value(&store, "kept").unwrap(), b"kept value");
    assert_eq!(value(&store, "cut"), None);
    assert_eq!(
        value(&store, "after").unwrap(),
        b"written after the restart"
    );
}

/// The number of blobs a volume holds, from its `/health` endpoint, once it
/// answers.
async fn volume_keys(cluster: &Cluster, volume: usize) -> u64 {
    let url = format!("{}/health", cluster.volume_urls[volume]);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Ok(response) = cluster.client.get(&url).send().await {
            return json(response).await["total_keys"].as_u64().unwrap();
        }
        assert!(
            Instant::now() < deadline,
            "the volume does not answer:\n{}",
            cluster.volumes[volume].log()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_restarted_volume_server_keeps_only_its_live_blobs() {
    let mut cluster = Cluster::start(1);
    cluster.wait_ready(1).await;

    for i in 0..5 {
        let response = cluster
            .client
            .put(cluster.url(&format!("/restart-{}", i)))
            .body(format!("value {}", i))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    for i in [1, 3] {
        let response = cluster
            .client
            .delete(cluster.url(&format!("/restart-{}", i)))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }
    assert_eq!(volume_keys(&cluster, 0).await, 3);

    cluster.volumes[0].restart();
    assert_eq!(volume_keys(&cluster, 0).await, 3);
    cluster.wait_ready(1).await;

    for i in [0, 2, 4] {
        let response = cluster
            .client
            .get(cluster.url(&format!("/restart-{}", i)))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "GET restart-{}", i);
        assert_eq!(response.text().await.unwrap(), format!("value {}", i));
    }
    for i in [1, 3] {
        let response = cluster
            .client
            .get(cluster.url(&format!("/restart-{}", i)))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "GET restart-{}",
            i
        );
    }
}
