//! The admin and query endpoints report what happened: unreadable values are
//! listed instead of skipped, status answers hold no invented fields, and the
//! coordinator's own files stay in its data directory.

mod support;

use reqwest::StatusCode;
use serde_json::{json, Value};
use std::fs;
use support::{json, Cluster};

async fn post_json(cluster: &Cluster, path: &str, body: Value) -> reqwest::Response {
    cluster
        .client
        .post(cluster.url(path))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap()
}

async fn search(cluster: &Cluster, query: &str) -> reqwest::Response {
    cluster
        .client
        .get(cluster.url("/search"))
        .query(&[("value", query)])
        .send()
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn export_and_search_report_the_values_they_cannot_read() {
    let mut cluster = Cluster::start(1);
    cluster.wait_ready(1).await;
    let values: [(&str, &[u8]); 2] = [
        ("text", b"alpha"),
        // Not UTF-8, and still searchable byte for byte.
        ("binary", &[0xff, 0xfe, b'b', b'e', b't', b'a']),
    ];
    for (key, value) in values {
        let response = cluster
            .client
            .put(cluster.url(&format!("/{}", key)))
            .body(value.to_vec())
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    let found = json(search(&cluster, "beta").await).await;
    assert_eq!(found["matching_keys"], json!(["binary"]));
    assert_eq!(found["unreadable"], json!([]));

    // With the only volume down, no value can be read.
    cluster.volumes[0].stop();

    let response = search(&cluster, "alpha").await;
    assert_eq!(response.status(), StatusCode::OK);
    let found = json(response).await;
    assert_eq!(found["matching_keys"], json!([]));
    let unreadable: Vec<&str> = found["unreadable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["key"].as_str().unwrap())
        .collect();
    assert_eq!(unreadable, ["binary", "text"]);

    let export = cluster
        .client
        .get(cluster.url("/admin/export"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let lines: Vec<Value> = export
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(lines.len(), 2, "{}", export);
    for line in &lines {
        assert!(line["error"].is_string(), "{}", line);
        assert!(line.get("value").is_none(), "{}", line);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_vector_index_and_the_audit_log_live_in_the_data_directory() {
    let cluster = Cluster::start(0);
    cluster.wait_ready(0).await;
    let dir = cluster.dir.path();
    // The coordinator runs in `dir`, with `--db dir/coord`. A 2.0.0
    // coordinator started there kept its vector index in `dir/coord-data`.
    fs::create_dir(dir.join("coord-data")).unwrap();
    fs::write(
        dir.join("coord-data/vector_index.json"),
        r#"{"old":{"id":"old","values":[1.0,0.0],"metadata":null,"updated_at":0}}"#,
    )
    .unwrap();

    let stats = json(
        cluster
            .client
            .get(cluster.url("/admin/vector/stats"))
            .send()
            .await
            .unwrap(),
    )
    .await;
    assert_eq!(stats["vectors"], 1, "the old index is loaded: {}", stats);
    let index = dir.join("coord").join("vector_index.json");
    assert_eq!(stats["path"], index.display().to_string());

    let response = post_json(
        &cluster,
        "/vector/upsert",
        json!({ "id": "new", "values": [0.0, 1.0] }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    let saved: Value = serde_json::from_str(&fs::read_to_string(&index).unwrap()).unwrap();
    let mut ids: Vec<&String> = saved.as_object().unwrap().keys().collect();
    ids.sort();
    assert_eq!(ids, ["new", "old"]);

    let response = post_json(
        &cluster,
        "/admin/keys",
        json!({ "name": "ops-key", "role": "admin" }),
    )
    .await;
    assert_eq!(response.status(), StatusCode::CREATED);
    let audit = fs::read_to_string(dir.join("coord").join("audit.log")).unwrap();
    let entry: Value = serde_json::from_str(audit.lines().last().unwrap()).unwrap();
    assert_eq!(entry["actor"], "unauthenticated", "{}", entry);
    assert!(!dir.join("audit.log").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn status_answers_list_only_what_exists() {
    let cluster = Cluster::start(0);
    cluster.wait_ready(0).await;
    let get = |path: &'static str| {
        let request = cluster.client.get(cluster.url(path));
        async move { json(request.send().await.unwrap()).await }
    };

    // Every resolution and aggregation listed is accepted by /ts/query.
    let stats = get("/admin/timeseries/stats").await;
    assert_eq!(stats["compression"], json!(["delta"]));
    assert!(stats["stats"].get("retention_days").is_none(), "{}", stats);
    for resolution in stats["resolutions"].as_array().unwrap() {
        for aggregation in stats["aggregations"].as_array().unwrap() {
            let response = cluster
                .client
                .get(cluster.url("/ts/query"))
                .query(&[
                    ("metric", "cpu"),
                    ("resolution", resolution.as_str().unwrap()),
                    ("aggregation", aggregation.as_str().unwrap()),
                ])
                .send()
                .await
                .unwrap();
            assert_eq!(
                response.status(),
                StatusCode::OK,
                "{} {}",
                resolution,
                aggregation
            );
        }
    }

    assert_eq!(
        get("/admin/geo/status").await,
        json!({ "enabled": false, "local_region": null, "remote_regions": [] })
    );
    assert_eq!(
        get("/admin/cdc/status").await,
        json!({ "enabled": false, "message": "CDC not configured" })
    );
}
