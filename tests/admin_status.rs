//! Test the dashboard endpoint /admin/status

use reqwest::Client;
use serde_json::Value;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::env;

/// Find a free TCP port on localhost
fn get_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

/// Launch the minikv-coord server in the background, returns (Child, http_port, grpc_port)
fn start_server() -> (Child, u16, u16) {
    // Free ports
    let http_port = get_free_port();
    let grpc_port = get_free_port();
    // Remove data directory to avoid RocksDB locks
    let _ = std::fs::remove_dir_all("coord-test-data");
    let _ = std::fs::create_dir_all("coord-test-data");
    // Write a minimal config.toml file
    std::fs::write(
        "config.toml",
        "node_id = 'coord-test'\nrole = 'coordinator'\n",
    )
    .expect("Failed to write config.toml");
    let mut cmd = Command::new(env::var("CARGO_BIN_EXE_minikv-coord").expect("CARGO_BIN_EXE_minikv-coord not set by cargo test"));
    cmd.args([
        "serve",
        "--id",
        "coord-test",
        "--bind",
        &format!("127.0.0.1:{}", http_port),
        "--grpc",
        &format!("127.0.0.1:{}", grpc_port),
        "--db",
        "./coord-test-data",
    ]);
    let log = std::fs::File::create("coord-test.log").expect("Failed to create log file");
    let log_err = log.try_clone().expect("Failed to clone log file");
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    let child = cmd.spawn().expect("Failed to launch minikv-coord server");
    (child, http_port, grpc_port)
}

/// Wait until the HTTP endpoint is ready (timeout 15s)
async fn wait_for_server(child: &mut Child, http_port: u16) {
    let client = Client::new();
    let url = format!("http://localhost:{}/admin/status", http_port);
    let start = Instant::now();
    loop {
        // If the server has exited, print the error
        if let Some(status) = child.try_wait().expect("Error waiting for server") {
            if let Some(mut stderr) = child.stderr.take() {
                use std::io::Read;
                let mut buf = String::new();
                let _ = stderr.read_to_string(&mut buf);
                panic!("minikv-coord server exited prematurely (exit code {status}):\n{buf}");
            } else {
                panic!("minikv-coord server exited prematurely (exit code {status})");
            }
        }
        if start.elapsed() > Duration::from_secs(15) {
            panic!("Timeout: server not ready at {url}");
        }
        if let Ok(resp) = client.get(&url).send().await {
            if resp.status().is_success() {
                break;
            }
        }
        sleep(Duration::from_millis(100));
    }
}

#[tokio::test]
async fn test_admin_status() {
    // Start the server
    let (mut server, http_port, _grpc_port) = start_server();
    // Wait until it is ready
    wait_for_server(&mut server, http_port).await;

    let client = Client::new();
    let url = format!("http://localhost:{}/admin/status", http_port);
    let resp = client
        .get(&url)
        .send()
        .await
        .expect("Status request failed");
    assert!(resp.status().is_success(), "status endpoint failed");
    let text = resp.text().await.expect("Failed to read response body");
    let json: Value = serde_json::from_str(&text).expect("Response is not valid JSON");
    assert!(json.get("role").is_some());
    assert!(json.get("is_leader").is_some());
    assert!(json.get("nb_peers").is_some());
    assert!(json.get("nb_volumes").is_some());
    assert!(json.get("nb_s3_objects").is_some());

    // Stop the server
    let _ = server.kill();
    let _ = server.wait();
}
