//! Features that minikv does not implement fail explicitly: the coordinator
//! answers `501 Not Implemented` with the release planned for them, the
//! matching CLI commands exit with an error, and the admin dashboard shows
//! these answers instead of invented values.

mod support;

use reqwest::{Method, StatusCode};
use serde_json::{json, Value};
use support::{cli, json, stderr, Cluster};

/// Method, path, feature and roadmap of every route that is not implemented.
const NOT_IMPLEMENTED_ROUTES: [(&str, &str, &str, &str); 9] = [
    ("POST", "/admin/verify", "verify", "2.1.0"),
    ("POST", "/admin/repair", "repair", "2.2.0"),
    ("POST", "/admin/compact", "compact", "2.2.0"),
    ("POST", "/admin/scale", "scale", "2.2.0"),
    ("POST", "/admin/backup", "backup", "unscheduled"),
    ("GET", "/admin/backups", "backup", "unscheduled"),
    ("GET", "/admin/backups/any-id", "backup", "unscheduled"),
    ("DELETE", "/admin/backups/any-id", "backup", "unscheduled"),
    ("POST", "/admin/restore", "restore", "unscheduled"),
];

fn body(feature: &str, roadmap: &str) -> Value {
    json!({ "error": "not implemented", "feature": feature, "roadmap": roadmap })
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn unimplemented_admin_routes_answer_501() {
    let cluster = Cluster::start(0);
    cluster.wait_ready(0).await;

    for (method, path, feature, roadmap) in NOT_IMPLEMENTED_ROUTES {
        let method = Method::from_bytes(method.as_bytes()).unwrap();
        let response = cluster
            .client
            .request(method.clone(), cluster.url(path))
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_IMPLEMENTED,
            "{} {}",
            method,
            path
        );
        let answer = json(response).await;
        assert_eq!(answer, body(feature, roadmap), "{} {}", method, path);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ttl_header_is_rejected_and_nothing_is_stored() {
    let cluster = Cluster::start(1);
    cluster.wait_ready(1).await;

    for (method, path) in [
        (Method::PUT, "/s3/demo/short-lived.txt"),
        (Method::PUT, "/short-lived"),
        (Method::POST, "/short-lived-post"),
    ] {
        let response = cluster
            .client
            .request(method.clone(), cluster.url(path))
            .header("X-Minikv-TTL", "60")
            .body("value")
            .send()
            .await
            .unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_IMPLEMENTED,
            "{} {}",
            method,
            path
        );
        let answer = json(response).await;
        assert_eq!(answer, body("ttl", "unscheduled"));

        let read = cluster.client.get(cluster.url(path)).send().await.unwrap();
        assert_eq!(read.status(), StatusCode::NOT_FOUND, "{} was stored", path);
    }

    // The same write without the header succeeds: only the TTL is refused.
    let response = cluster
        .client
        .put(cluster.url("/s3/demo/kept.txt"))
        .body("value")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[test]
fn unimplemented_cli_commands_fail_with_their_roadmap() {
    // The commands never contact the coordinator: nothing listens on port 9.
    let cases: [(&[&str], &str); 7] = [
        (&["verify"], "verify is not implemented (roadmap: 2.1.0)"),
        (
            &["verify", "--deep", "--concurrency", "4"],
            "verify is not implemented (roadmap: 2.1.0)",
        ),
        (
            &["repair", "--replicas", "2", "--dry-run"],
            "repair is not implemented (roadmap: 2.2.0)",
        ),
        (
            &["compact", "--shard", "0"],
            "compact is not implemented (roadmap: 2.2.0)",
        ),
        (
            &["rebalance"],
            "rebalance is not implemented (roadmap: 2.2.0)",
        ),
        (
            &["upgrade"],
            "upgrade is not implemented (roadmap: unscheduled)",
        ),
        (
            &["stream", "--key", "big"],
            "stream is not implemented (roadmap: unscheduled)",
        ),
    ];
    for (args, message) in cases {
        let mut all = vec!["--coordinator", "http://127.0.0.1:9"];
        all.extend_from_slice(args);
        let output = cli(&all);
        let err = stderr(&output);
        assert_eq!(output.status.code(), Some(1), "{:?}: {}", args, err);
        assert!(
            err.contains(&format!("error: {}", message)),
            "{:?}: {}",
            args,
            err
        );
        assert!(!err.to_lowercase().contains("backtrace"), "{}", err);
        assert!(output.stdout.is_empty(), "{:?} printed a report", args);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dashboard_reads_fields_that_admin_status_returns() {
    let cluster = Cluster::start(0);
    cluster.wait_ready(0).await;

    let html = cluster
        .client
        .get(cluster.url("/admin/ui"))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let status = cluster.status().await.unwrap();

    let start = html.find("async function refreshStatus()").unwrap();
    let end = start + html[start..].find("async function refreshKeys()").unwrap();
    let refresh_status = &html[start..end];
    let fields: Vec<String> = refresh_status
        .match_indices("data.")
        .map(|(i, _)| {
            refresh_status[i + "data.".len()..]
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
                .collect()
        })
        .collect();

    assert!(fields.len() >= 6, "{:?}", fields);
    for field in fields {
        assert!(
            status.get(&field).is_some(),
            "the dashboard reads {}, which /admin/status does not return: {}",
            field,
            status
        );
    }
}
