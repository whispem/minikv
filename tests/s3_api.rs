//! Basic test for the S3-compatible API (PUT then GET)
use reqwest::Client;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use std::env;

fn get_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn start_coord(http_port: u16, grpc_port: u16) -> Child {
    let _ = std::fs::remove_dir_all("coord-s3-data");
    let _ = std::fs::create_dir_all("coord-s3-data");
    std::fs::write(
        "config.toml",
        "node_id = 'coord-s3'\nrole = 'coordinator'\nreplicas = 1\n",
    )
    .expect("Failed to write config.toml");
    let mut cmd = Command::new(env::var("CARGO_BIN_EXE_minikv-coord").expect("CARGO_BIN_EXE_minikv-coord not set by cargo test"));
    cmd.args([
        "serve",
        "--id",
        "coord-s3",
        "--bind",
        &format!("127.0.0.1:{}", http_port),
        "--grpc",
        &format!("127.0.0.1:{}", grpc_port),
        "--db",
        "./coord-s3-data",
    ]);
    let log = std::fs::File::create("coord-s3.log").expect("Failed to create log file");
    let log_err = log.try_clone().expect("Failed to clone log file");
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    cmd.spawn().expect("Failed to launch minikv-coord server")
}

fn start_volume(http_port: u16, grpc_port: u16, coord_http_port: u16) -> Child {
    let _ = std::fs::remove_dir_all("vol-s3-data");
    let _ = std::fs::remove_dir_all("vol-s3-wal");
    let _ = std::fs::create_dir_all("vol-s3-data");
    let _ = std::fs::create_dir_all("vol-s3-wal");
    let mut cmd = Command::new(env::var("CARGO_BIN_EXE_minikv-volume").expect("CARGO_BIN_EXE_minikv-volume not set by cargo test"));
    cmd.args([
        "serve",
        "--id",
        "vol-s3",
        "--bind",
        &format!("127.0.0.1:{}", http_port),
        "--grpc",
        &format!("127.0.0.1:{}", grpc_port),
        "--data",
        "./vol-s3-data",
        "--wal",
        "./vol-s3-wal",
        "--coordinators",
        &format!("http://127.0.0.1:{}", coord_http_port),
    ]);
    let log = std::fs::File::create("vol-s3.log").expect("Failed to create log file");
    let log_err = log.try_clone().expect("Failed to clone log file");
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    cmd.spawn().expect("Failed to launch minikv-volume server")
}

async fn wait_for_endpoint(childs: &mut [&mut Child], url: &str) {
    let client = Client::new();
    let start = Instant::now();
    loop {
        for child in childs.iter_mut() {
            if let Some(status) = child.try_wait().expect("Error waiting for server") {
                panic!("A server exited prematurely (exit code {status})");
            }
        }
        if start.elapsed() > Duration::from_secs(20) {
            panic!("Timeout: endpoint not ready at {url}");
        }
        if let Ok(resp) = client.get(url).send().await {
            if resp.status().is_success() || resp.status().as_u16() == 404 {
                break;
            }
        }
        sleep(Duration::from_millis(100));
    }
}

#[tokio::test]
async fn test_s3_put_get() {
    // Dynamic ports
    let coord_http = get_free_port();
    let coord_grpc = get_free_port();
    let vol_http = get_free_port();
    let vol_grpc = get_free_port();

    let mut coord = start_coord(coord_http, coord_grpc);
    let mut volume = start_volume(vol_http, vol_grpc, coord_http);

    // Wait until the S3 endpoint is ready (404 or 200)
    let s3_url = format!("http://127.0.0.1:{}/s3/testbucket/hello.txt", coord_http);
    wait_for_endpoint(&mut [&mut coord, &mut volume], &s3_url).await;

    let client = Client::new();
    let data = b"Hello, S3!";
    // PUT
    let put_resp = client
        .put(&s3_url)
        .body(data.as_ref())
        .send()
        .await
        .unwrap();
    assert!(put_resp.status().is_success(), "PUT failed: {:?}", put_resp);
    // GET
    let get_resp = client.get(&s3_url).send().await.unwrap();
    assert!(get_resp.status().is_success(), "GET failed: {:?}", get_resp);
    let body = get_resp.bytes().await.unwrap();
    assert_eq!(body.as_ref(), data, "GET body mismatch");

    // Stop the servers
    let _ = coord.kill();
    let _ = coord.wait();
    let _ = volume.kill();
    let _ = volume.wait();
}
