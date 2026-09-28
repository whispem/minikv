//! The coordinator stores each key on the volume that rendezvous hashing (HRW)
//! ranks first for that key: the highest first 8 bytes (little endian) of
//! BLAKE3 of the key followed by the volume id. The cluster runs with one
//! replica, so each write lands on exactly one volume.

mod support;

use reqwest::StatusCode;
use support::{json, Cluster};

/// HRW computed from its definition, independently of minikv's code.
fn hrw_first(key: &str, volumes: &[String]) -> String {
    volumes
        .iter()
        .max_by_key(|id| {
            let digest = blake3::hash(format!("{}{}", key, id).as_bytes());
            u64::from_le_bytes(digest.as_bytes()[..8].try_into().unwrap())
        })
        .unwrap()
        .clone()
}

/// The number of keys each volume holds, from its own `/health` endpoint.
async fn keys_per_volume(cluster: &Cluster) -> Vec<u64> {
    let mut counts = Vec::new();
    for url in &cluster.volume_urls {
        let response = cluster
            .client
            .get(format!("{}/health", url))
            .send()
            .await
            .unwrap();
        counts.push(json(response).await["total_keys"].as_u64().unwrap());
    }
    counts
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn each_key_goes_to_the_volume_that_hrw_ranks_first() {
    let cluster = Cluster::start(3);
    cluster.wait_ready(3).await;
    let ids: Vec<String> = (0..3).map(|i| format!("vol-{}", i)).collect();
    let mut keys_per_expected_volume = vec![0; ids.len()];

    for i in 0..60 {
        let key = format!("placement-{}", i);
        let before = keys_per_volume(&cluster).await;
        let response = cluster
            .client
            .put(cluster.url(&format!("/{}", key)))
            .body("value")
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK, "PUT {}", key);
        let after = keys_per_volume(&cluster).await;

        let written: Vec<&String> = ids
            .iter()
            .zip(before.iter().zip(&after))
            .filter(|(_, (before, after))| after > before)
            .map(|(id, _)| id)
            .collect();
        let expected = hrw_first(&key, &ids);
        assert_eq!(
            written,
            vec![&expected],
            "{} was written on {:?}, HRW ranks {} first",
            key,
            written,
            expected
        );
        keys_per_expected_volume[ids.iter().position(|id| *id == expected).unwrap()] += 1;
    }

    // Each volume was the first choice of some keys.
    assert!(
        keys_per_expected_volume.iter().all(|&count| count > 0),
        "{:?}",
        keys_per_expected_volume
    );
}
