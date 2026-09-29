//! The dashboard endpoint `/admin/status`.
//!
//! The coordinator runs in a temporary directory: the test writes nothing in
//! the repository.

mod support;

use support::{json, Cluster};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_admin_status() {
    let cluster = Cluster::start(0);
    cluster.wait_ready(0).await;

    let resp = cluster
        .client
        .get(cluster.url("/admin/status"))
        .send()
        .await
        .expect("Status request failed");
    assert!(resp.status().is_success(), "status endpoint failed");
    let json = json(resp).await;
    assert!(json.get("role").is_some());
    assert!(json.get("is_leader").is_some());
    assert!(json.get("nb_peers").is_some());
    assert!(json.get("nb_volumes").is_some());
    assert!(json.get("nb_s3_objects").is_some());
}
