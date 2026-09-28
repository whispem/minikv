//! Helpers shared by the integration tests that start real processes.
//!
//! Every cluster runs in its own temporary directory, on free ports, and its
//! processes are killed when the cluster is dropped.

#![allow(dead_code)]

use reqwest::Client;
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Output, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const COORD_BIN: &str = env!("CARGO_BIN_EXE_minikv-coord");
pub const VOLUME_BIN: &str = env!("CARGO_BIN_EXE_minikv-volume");
pub const CLI_BIN: &str = env!("CARGO_BIN_EXE_minikv");

static USED_PORTS: Mutex<Option<HashSet<u16>>> = Mutex::new(None);

/// A TCP port that is free now and that no other test of this binary got.
pub fn free_port() -> u16 {
    let mut used = USED_PORTS.lock().unwrap();
    let used = used.get_or_insert_with(HashSet::new);
    loop {
        let port = 20_000 + (rand::random::<u16>() % 12_000);
        if used.contains(&port) || TcpListener::bind(("127.0.0.1", port)).is_err() {
            continue;
        }
        used.insert(port);
        return port;
    }
}

/// Runs the `minikv` CLI with `args` and returns its output.
///
/// `RUST_BACKTRACE` is set, as in CI, to check that errors stay short.
pub fn cli(args: &[&str]) -> Output {
    Command::new(CLI_BIN)
        .args(args)
        .env("RUST_BACKTRACE", "1")
        .output()
        .expect("cannot run the minikv CLI")
}

/// The body of an HTTP answer, decoded as JSON.
pub async fn json(response: reqwest::Response) -> Value {
    let text = response.text().await.expect("cannot read the answer");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("not JSON ({}): {}", e, text))
}

/// The standard error of a finished process, as text.
pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A child process whose output goes to a log file, killed on drop.
pub struct Process {
    name: String,
    binary: &'static str,
    args: Vec<String>,
    dir: PathBuf,
    log: PathBuf,
    child: Option<Child>,
}

impl Process {
    pub fn start(name: &str, binary: &'static str, args: Vec<String>, dir: &Path) -> Self {
        let mut process = Self {
            name: name.to_string(),
            binary,
            args,
            dir: dir.to_path_buf(),
            log: dir.join(format!("{}.log", name)),
            child: None,
        };
        process.spawn();
        process
    }

    fn spawn(&mut self) {
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .unwrap();
        let log_err = log.try_clone().unwrap();
        let child = Command::new(self.binary)
            .args(&self.args)
            .current_dir(&self.dir)
            .env("RUST_LOG", "info")
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            .spawn()
            .unwrap_or_else(|e| panic!("cannot start {}: {}", self.name, e));
        self.child = Some(child);
    }

    /// Kills the process and waits for it.
    pub fn stop(&mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    /// Starts the process again with the same arguments and directory.
    pub fn restart(&mut self) {
        self.stop();
        self.spawn();
    }

    /// Everything the process wrote so far.
    pub fn log(&self) -> String {
        fs::read_to_string(&self.log).unwrap_or_default()
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        self.stop();
    }
}

/// One coordinator, with a replication factor of 1, and some volume servers.
pub struct Cluster {
    pub coordinator: Process,
    pub volumes: Vec<Process>,
    pub volume_urls: Vec<String>,
    pub url: String,
    pub client: Client,
    // Dropped last, after the processes.
    pub dir: tempfile::TempDir,
}

impl Cluster {
    pub fn start(volume_count: usize) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let (http, grpc) = (free_port(), free_port());
        let url = format!("http://127.0.0.1:{}", http);
        let coordinator = Process::start(
            "coord",
            COORD_BIN,
            vec![
                "serve".into(),
                "--id".into(),
                "coord".into(),
                "--bind".into(),
                format!("127.0.0.1:{}", http),
                "--grpc".into(),
                format!("127.0.0.1:{}", grpc),
                "--db".into(),
                dir.path().join("coord").display().to_string(),
                "--replicas".into(),
                "1".into(),
            ],
            dir.path(),
        );

        let mut volumes = Vec::new();
        let mut volume_urls = Vec::new();
        for i in 0..volume_count {
            let (http, grpc) = (free_port(), free_port());
            let name = format!("vol-{}", i);
            let data = dir.path().join(&name);
            volumes.push(Process::start(
                &name,
                VOLUME_BIN,
                vec![
                    "serve".into(),
                    "--id".into(),
                    name.clone(),
                    "--bind".into(),
                    format!("127.0.0.1:{}", http),
                    "--grpc".into(),
                    format!("127.0.0.1:{}", grpc),
                    "--data".into(),
                    data.join("data").display().to_string(),
                    "--wal".into(),
                    data.join("wal").display().to_string(),
                    "--coordinators".into(),
                    url.clone(),
                    "--heartbeat-ms".into(),
                    "200".into(),
                ],
                dir.path(),
            ));
            volume_urls.push(format!("http://127.0.0.1:{}", http));
        }

        Self {
            coordinator,
            volumes,
            volume_urls,
            url,
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
            dir,
        }
    }

    /// The coordinator URL of `path`, which starts with `/`.
    pub fn url(&self, path: &str) -> String {
        format!("{}{}", self.url, path)
    }

    /// Waits until the coordinator is the leader and sees `volumes` live volumes.
    pub async fn wait_ready(&self, volumes: u64) {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            let status = self.status().await;
            let leader = status.as_ref().map(|s| s["is_leader"] == Value::Bool(true));
            let live = status.as_ref().and_then(|s| s["nb_volumes"].as_u64());
            if leader == Some(true) && live == Some(volumes) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "cluster not ready (status {:?}), coordinator log:\n{}",
                status,
                self.coordinator.log()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    /// The body of `GET /admin/status`, if the coordinator answers.
    pub async fn status(&self) -> Option<Value> {
        let response = self
            .client
            .get(self.url("/admin/status"))
            .timeout(Duration::from_secs(1))
            .send()
            .await
            .ok()?;
        serde_json::from_str(&response.text().await.ok()?).ok()
    }
}
