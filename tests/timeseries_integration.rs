//! Integration tests for the time-series endpoints.
//!
//! The first two tests expect a coordinator on port 8000, as in CI. The last
//! one starts its own.

mod support;

use reqwest::Client;
use serde_json::{json, Value};
use support::Cluster;

#[tokio::test]
async fn test_timeseries_write() {
    let client = Client::new();
    let payload = serde_json::json!({
        "metric": "cpu.usage",
        "tags": {
            "host": "node-1"
        },
        "points": [
            {"timestamp": 1700000000000i64, "value": 42.0},
            {"timestamp": 1700000060000i64, "value": 43.0}
        ]
    });
    let resp = client
        .post("http://localhost:8000/ts/write")
        .body(serde_json::to_string(&payload).unwrap())
        .header("Content-Type", "application/json")
        .send()
        .await
        .expect("Failed to write timeseries data");
    assert!(resp.status().is_success());
    let body = resp.text().await.expect("Failed to read response body");
    assert!(body.contains("success"));
}

#[tokio::test]
async fn test_timeseries_query() {
    let client = Client::new();

    let seed_payload = serde_json::json!({
        "metric": "cpu.usage",
        "tags": {
            "host": "node-1"
        },
        "points": [
            {"timestamp": 1700000000000i64, "value": 42.0},
            {"timestamp": 1700000060000i64, "value": 43.0}
        ]
    });
    let seed = client
        .post("http://localhost:8000/ts/write")
        .body(serde_json::to_string(&seed_payload).unwrap())
        .header("Content-Type", "application/json")
        .send()
        .await
        .expect("Failed to seed timeseries data");
    assert!(seed.status().is_success());

    let query_payload = serde_json::json!({
        "metric": "cpu.usage",
        "start": "2023-11-14T22:13:20Z",
        "end": "2023-11-14T22:15:00Z",
        "tags": {
            "host": "node-1"
        }
    });

    let resp = client
        .post("http://localhost:8000/ts/query")
        .body(serde_json::to_string(&query_payload).unwrap())
        .header("Content-Type", "application/json")
        .send()
        .await
        .expect("Failed to query timeseries data");
    assert!(resp.status().is_success());
    let body = resp.text().await.expect("Failed to read response body");
    assert!(body.contains("points"));

    // The range starts at the first point, in the middle of an hour: both
    // points are in the answer, maybe more than once, since the other test
    // writes the same points.
    let body: Value = serde_json::from_str(&body).expect("the answer is not JSON");
    let values: Vec<f64> = body["points"]
        .as_array()
        .expect("no list of points")
        .iter()
        .map(|point| point["value"].as_f64().unwrap())
        .collect();
    assert!(values.contains(&42.0), "{}", body);
    assert!(values.contains(&43.0), "{}", body);
}

async fn post(cluster: &Cluster, path: &str, body: Value) -> Value {
    let response = cluster
        .client
        .post(cluster.url(path))
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success(), "{}", path);
    support::json(response).await
}

/// Two writes in the same hour are both kept, and a query that starts in the
/// middle of that hour finds them. Version 2.0.0 kept the last write only,
/// and missed the hour.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn writes_in_the_same_hour_are_all_read_back() {
    let cluster = Cluster::start(0);
    cluster.wait_ready(0).await;
    // 2023-11-14T22:00:00Z, the start of an hour.
    let hour = 1_699_999_200_000i64;
    let minute = 60_000i64;

    for points in [
        json!([
            { "timestamp": hour + 10 * minute, "value": 1.0 },
            { "timestamp": hour + 30 * minute, "value": 2.0 },
        ]),
        json!([{ "timestamp": hour + 40 * minute, "value": 3.0 }]),
    ] {
        post(
            &cluster,
            "/ts/write",
            json!({ "metric": "cpu", "points": points }),
        )
        .await;
    }

    let answer = post(
        &cluster,
        "/ts/query",
        json!({
            "metric": "cpu",
            "start": "2023-11-14T22:20:00Z",
            "end": "2023-11-14T23:00:00Z",
        }),
    )
    .await;
    let points: Vec<(i64, f64)> = answer["points"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["timestamp"].as_i64().unwrap(),
                p["value"].as_f64().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        points,
        [(hour + 30 * minute, 2.0), (hour + 40 * minute, 3.0)],
        "{}",
        answer
    );
}
