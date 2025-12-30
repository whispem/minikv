//! Test the dashboard endpoint /admin/status
use reqwest::Client;
use serde_json::Value;

#[tokio::test]
async fn test_admin_status() {
    let client = Client::new();
    let url = "http://localhost:5000/admin/status";
    let resp = client.get(url).send().await.unwrap();
    assert!(resp.status().is_success(), "status endpoint failed");
    let text = resp.text().await.unwrap();
    let json: Value = serde_json::from_str(&text).unwrap();
    assert!(json.get("role").is_some());
    assert!(json.get("is_leader").is_some());
    assert!(json.get("nb_peers").is_some());
    assert!(json.get("nb_volumes").is_some());
    assert!(json.get("nb_s3_objects").is_some());
}
