//! Additional tests for the S3-compatible API
use reqwest::Client;
use std::fs;
use std::net::TcpListener;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};
use uuid::Uuid;

fn get_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
        .port()
}

fn start_coord(http_port: u16, grpc_port: u16, test_id: &str) -> (Child, String) {
    let coord_data = format!("coord-s3extra-data-{}", test_id);
    let _ = fs::remove_dir_all(&coord_data);
    let _ = fs::create_dir_all(&coord_data);
    let config_path = format!("/tmp/minikv-config-{}.toml", test_id);
    fs::write(
        &config_path,
        format!(
            "node_id = 'coord-s3extra-{}'\nrole = 'coordinator'\nreplicas = 1\n",
            test_id
        ),
    )
    .expect("Failed to write config.toml");
    let mut cmd = Command::new("target/release/minikv-coord");
    cmd.args([
        "serve",
        "--id",
        &format!("coord-s3extra-{}", test_id),
        "--bind",
        &format!("127.0.0.1:{}", http_port),
        "--grpc",
        &format!("127.0.0.1:{}", grpc_port),
        "--db",
        &coord_data,
    ]);
    cmd.env_clear();
    for (key, value) in std::env::vars() {
        if key != "MINIKV_CONFIG" && key != "RUST_LOG" && key != "RUST_BACKTRACE" {
            cmd.env(&key, &value);
        }
    }
    cmd.env("MINIKV_CONFIG", &config_path);
    cmd.env("RUST_LOG", "debug");
    cmd.env("RUST_BACKTRACE", "1");
    let log_path = format!("coord-s3extra-{}.log", test_id);
    let log = fs::File::create(&log_path).expect("Failed to create log file");
    let log_err = log.try_clone().expect("Failed to clone log file");
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    (
        cmd.spawn().expect("Failed to launch minikv-coord server"),
        coord_data,
    )
}

fn start_volume(
    http_port: u16,
    grpc_port: u16,
    coord_http_port: u16,
    test_id: &str,
) -> (Child, String, String) {
    let vol_data = format!("vol-s3extra-data-{}", test_id);
    let vol_wal = format!("vol-s3extra-wal-{}", test_id);
    let _ = fs::remove_dir_all(&vol_data);
    let _ = fs::remove_dir_all(&vol_wal);
    let _ = fs::create_dir_all(&vol_data);
    let _ = fs::create_dir_all(&vol_wal);
    let mut cmd = Command::new("target/release/minikv-volume");
    cmd.args([
        "serve",
        "--id",
        &format!("vol-s3extra-{}", test_id),
        "--bind",
        &format!("127.0.0.1:{}", http_port),
        "--grpc",
        &format!("127.0.0.1:{}", grpc_port),
        "--data",
        &vol_data,
        "--wal",
        &vol_wal,
        "--coordinators",
        &format!("http://127.0.0.1:{}", coord_http_port),
    ]);
    let log_path = format!("vol-s3extra-{}.log", test_id);
    let log = fs::File::create(&log_path).expect("Failed to create log file");
    let log_err = log.try_clone().expect("Failed to clone log file");
    cmd.stdout(Stdio::from(log));
    cmd.stderr(Stdio::from(log_err));
    cmd.env_clear();
    for (key, value) in std::env::vars() {
        if key != "MINIKV_CONFIG" && key != "RUST_LOG" && key != "RUST_BACKTRACE" {
            cmd.env(&key, &value);
        }
    }
    cmd.env("RUST_LOG", "debug");
    cmd.env("RUST_BACKTRACE", "1");
    (
        cmd.spawn().expect("Failed to launch minikv-volume server"),
        vol_data,
        vol_wal,
    )
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
async fn test_s3_404() {
    let test_id = Uuid::new_v4().to_string();
    let coord_http = get_free_port();
    let coord_grpc = get_free_port();
    let vol_http = get_free_port();
    let vol_grpc = get_free_port();
    let (mut coord, coord_data) = start_coord(coord_http, coord_grpc, &test_id);
    let (mut volume, vol_data, vol_wal) = start_volume(vol_http, vol_grpc, coord_http, &test_id);
    let url = format!("http://127.0.0.1:{}/s3/testbucket/notfound.txt", coord_http);
    wait_for_endpoint(&mut [&mut coord, &mut volume], &url).await;
    let client = Client::new();
    let resp = client.get(&url).send().await.unwrap();
    assert_eq!(resp.status(), 404);
    sleep(Duration::from_millis(500));
    let _ = coord.kill();
    let _ = coord.wait();
    let _ = volume.kill();
    let _ = volume.wait();
    let _ = fs::remove_file(format!("/tmp/minikv-config-{}.toml", test_id));
    let _ = fs::remove_file(format!("coord-s3extra-{}.log", test_id));
    let _ = fs::remove_file(format!("vol-s3extra-{}.log", test_id));
    let _ = fs::remove_dir_all(&coord_data);
    let _ = fs::remove_dir_all(&vol_data);
    let _ = fs::remove_dir_all(&vol_wal);
}

#[tokio::test]
async fn test_s3_overwrite() {
    let test_id = Uuid::new_v4().to_string();
    let coord_http = get_free_port();
    let coord_grpc = get_free_port();
    let vol_http = get_free_port();
    let vol_grpc = get_free_port();
    let (mut coord, coord_data) = start_coord(coord_http, coord_grpc, &test_id);
    let (mut volume, vol_data, vol_wal) = start_volume(vol_http, vol_grpc, coord_http, &test_id);
    let url = format!(
        "http://127.0.0.1:{}/s3/testbucket/overwrite.txt",
        coord_http
    );
    wait_for_endpoint(&mut [&mut coord, &mut volume], &url).await;
    let client = Client::new();
    let data1 = b"first";
    let data2 = b"second";
    // PUT first
    let put1 = client.put(&url).body(data1.as_ref()).send().await.unwrap();
    assert!(put1.status().is_success());
    // PUT overwrite
    let put2 = client.put(&url).body(data2.as_ref()).send().await.unwrap();
    assert!(put2.status().is_success());
    // GET
    let get = client.get(&url).send().await.unwrap();
    let body = get.bytes().await.unwrap();
    assert_eq!(body.as_ref(), data2);
    let _ = coord.kill();
    let _ = coord.wait();
    let _ = volume.kill();
    let _ = volume.wait();
    let _ = fs::remove_file(format!("/tmp/minikv-config-{}.toml", test_id));
    let _ = fs::remove_file(format!("coord-s3extra-{}.log", test_id));
    let _ = fs::remove_file(format!("vol-s3extra-{}.log", test_id));
    let _ = fs::remove_dir_all(&coord_data);
    let _ = fs::remove_dir_all(&vol_data);
    let _ = fs::remove_dir_all(&vol_wal);
}

#[tokio::test]
async fn test_s3_multiple_objects() {
    let test_id = Uuid::new_v4().to_string();
    let coord_http = get_free_port();
    let coord_grpc = get_free_port();
    let vol_http = get_free_port();
    let vol_grpc = get_free_port();
    let (mut coord, coord_data) = start_coord(coord_http, coord_grpc, &test_id);
    let (mut volume, vol_data, vol_wal) = start_volume(vol_http, vol_grpc, coord_http, &test_id);
    let url1 = format!("http://127.0.0.1:{}/s3/testbucket/obj1.txt", coord_http);
    let url2 = format!("http://127.0.0.1:{}/s3/testbucket/obj2.txt", coord_http);
    wait_for_endpoint(&mut [&mut coord, &mut volume], &url1).await;
    let client = Client::new();
    let data1 = b"foo";
    let data2 = b"bar";
    // PUT obj1
    let put1 = client.put(&url1).body(data1.as_ref()).send().await.unwrap();
    assert!(put1.status().is_success());
    // PUT obj2
    let put2 = client.put(&url2).body(data2.as_ref()).send().await.unwrap();
    assert!(put2.status().is_success());
    // GET obj1
    let get1 = client.get(&url1).send().await.unwrap();
    let body1 = get1.bytes().await.unwrap();
    assert_eq!(body1.as_ref(), data1);
    // GET obj2
    let get2 = client.get(&url2).send().await.unwrap();
    let body2 = get2.bytes().await.unwrap();
    assert_eq!(body2.as_ref(), data2);
    let _ = coord.kill();
    let _ = coord.wait();
    let _ = volume.kill();
    let _ = volume.wait();
    let _ = fs::remove_file(format!("/tmp/minikv-config-{}.toml", test_id));
    let _ = fs::remove_file(format!("coord-s3extra-{}.log", test_id));
    let _ = fs::remove_file(format!("vol-s3extra-{}.log", test_id));
    let _ = fs::remove_dir_all(&coord_data);
    let _ = fs::remove_dir_all(&vol_data);
    let _ = fs::remove_dir_all(&vol_wal);
}
