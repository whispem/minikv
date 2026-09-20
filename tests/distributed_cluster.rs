use reqwest::{Client, StatusCode};
use serde_json::Value;
use std::collections::HashSet;
use std::fs;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

const COORD_BIN: &str = env!("CARGO_BIN_EXE_minikv-coord");
const VOLUME_BIN: &str = env!("CARGO_BIN_EXE_minikv-volume");

static USED_PORTS: Mutex<Option<HashSet<u16>>> = Mutex::new(None);

fn free_port() -> u16 {
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

struct Node {
    name: String,
    binary: &'static str,
    args: Vec<String>,
    http: u16,
    log: PathBuf,
    child: Mutex<Option<Child>>,
}

impl Node {
    fn start(&mut self, root: &Path) {
        self.log = root.join(format!("{}.log", self.name));
        let log = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.log)
            .unwrap();
        let log_err = log.try_clone().unwrap();
        let child = Command::new(self.binary)
            .args(&self.args)
            .current_dir(root)
            .env("RUST_LOG", "info")
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(log_err))
            .spawn()
            .unwrap_or_else(|e| panic!("cannot start {}: {}", self.name, e));
        *self.child.lock().unwrap() = Some(child);
    }

    fn stop(&mut self) {
        if let Some(mut child) = self.child.lock().unwrap().take() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    fn assert_alive(&self) {
        let mut child = self.child.lock().unwrap();
        if let Some(Ok(Some(status))) = child.as_mut().map(|c| c.try_wait()) {
            let log = fs::read_to_string(&self.log).unwrap_or_default();
            panic!("{} exited with {}:\n{}", self.name, status, log);
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://127.0.0.1:{}{}", self.http, path)
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        self.stop();
    }
}

struct Cluster {
    root: PathBuf,
    coordinators: Vec<Node>,
    volumes: Vec<Node>,
    client: Client,
}

impl Cluster {
    fn start(coordinator_count: usize, volume_count: usize) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!(
            "minikv-distributed-{}-{}",
            std::process::id(),
            nanos
        ));
        fs::create_dir_all(&root).unwrap();
        eprintln!("cluster logs: {}", root.display());

        let coordinator_ports: Vec<(u16, u16)> = (0..coordinator_count)
            .map(|_| (free_port(), free_port()))
            .collect();
        let coordinator_urls: Vec<String> = coordinator_ports
            .iter()
            .map(|(http, _)| format!("http://127.0.0.1:{}", http))
            .collect();

        let mut coordinators = Vec::new();
        for (i, (http, grpc)) in coordinator_ports.iter().enumerate() {
            let peers: Vec<String> = coordinator_ports
                .iter()
                .enumerate()
                .filter(|(j, _)| *j != i)
                .map(|(_, (_, peer_grpc))| format!("127.0.0.1:{}", peer_grpc))
                .collect();
            let mut node = Node {
                name: format!("coord-{}", i),
                binary: COORD_BIN,
                args: vec![
                    "serve".into(),
                    "--id".into(),
                    format!("coord-{}", i),
                    "--bind".into(),
                    format!("127.0.0.1:{}", http),
                    "--grpc".into(),
                    format!("127.0.0.1:{}", grpc),
                    "--db".into(),
                    root.join(format!("coord-{}", i)).display().to_string(),
                    "--peers".into(),
                    peers.join(","),
                    "--replicas".into(),
                    "3".into(),
                ],
                http: *http,
                log: PathBuf::new(),
                child: Mutex::new(None),
            };
            node.start(&root);
            coordinators.push(node);
        }

        let mut volumes = Vec::new();
        for i in 0..volume_count {
            let (http, grpc) = (free_port(), free_port());
            let dir = root.join(format!("vol-{}", i));
            let mut node = Node {
                name: format!("vol-{}", i),
                binary: VOLUME_BIN,
                args: vec![
                    "serve".into(),
                    "--id".into(),
                    format!("vol-{}", i),
                    "--bind".into(),
                    format!("127.0.0.1:{}", http),
                    "--grpc".into(),
                    format!("127.0.0.1:{}", grpc),
                    "--data".into(),
                    dir.join("data").display().to_string(),
                    "--wal".into(),
                    dir.join("wal").display().to_string(),
                    "--coordinators".into(),
                    coordinator_urls.join(","),
                    "--heartbeat-ms".into(),
                    "200".into(),
                ],
                http,
                log: PathBuf::new(),
                child: Mutex::new(None),
            };
            node.start(&root);
            volumes.push(node);
        }

        Self {
            root,
            coordinators,
            volumes,
            client: Client::builder()
                .timeout(Duration::from_secs(10))
                .build()
                .unwrap(),
        }
    }

    fn check_alive(&self) {
        for node in self.coordinators.iter().chain(self.volumes.iter()) {
            node.assert_alive();
        }
    }

    async fn json(&self, url: String) -> Option<Value> {
        let response = self
            .client
            .get(url)
            .timeout(Duration::from_secs(1))
            .send()
            .await
            .ok()?;
        serde_json::from_str(&response.text().await.ok()?).ok()
    }

    async fn status(&self, coordinator: usize) -> Option<Value> {
        self.json(self.coordinators[coordinator].url("/admin/status"))
            .await
    }

    async fn wait_for_leader(&self, candidates: &[usize]) -> usize {
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            self.check_alive();
            let mut leaders = Vec::new();
            let mut views = Vec::new();
            for &i in candidates {
                let status = self.status(i).await;
                if let Some(status) = &status {
                    if status["is_leader"] == Value::Bool(true) {
                        leaders.push(i);
                    }
                }
                views.push(status.and_then(|s| s["leader"].as_str().map(String::from)));
            }
            if leaders.len() == 1 {
                let expected = format!("coord-{}", leaders[0]);
                if views
                    .iter()
                    .all(|v| v.as_deref() == Some(expected.as_str()))
                {
                    return leaders[0];
                }
            }
            assert!(
                Instant::now() < deadline,
                "no single leader among {:?} (leaders {:?}, views {:?}), logs in {}",
                candidates,
                leaders,
                views,
                self.root.display()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn term(&self, coordinator: usize) -> u64 {
        self.status(coordinator).await.unwrap()["term"]
            .as_u64()
            .unwrap()
    }

    async fn wait_for_volumes(&self, coordinator: usize, expected: u64) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.check_alive();
            let seen = self
                .status(coordinator)
                .await
                .and_then(|s| s["nb_volumes"].as_u64())
                .unwrap_or(0);
            if seen == expected {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "coord-{} sees {} volume(s) instead of {}, logs in {}",
                coordinator,
                seen,
                expected,
                self.root.display()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn put(&self, coordinator: usize, path: &str, body: &str) -> (StatusCode, String) {
        let response = self
            .client
            .put(self.coordinators[coordinator].url(path))
            .body(body.to_string())
            .send()
            .await
            .unwrap();
        (response.status(), response.text().await.unwrap())
    }

    async fn get(&self, coordinator: usize, path: &str) -> (StatusCode, String) {
        match self
            .client
            .get(self.coordinators[coordinator].url(path))
            .send()
            .await
        {
            Ok(response) => (response.status(), response.text().await.unwrap_or_default()),
            Err(e) => (StatusCode::SERVICE_UNAVAILABLE, e.to_string()),
        }
    }

    async fn assert_get(
        &self,
        coordinator: usize,
        path: &str,
        status: StatusCode,
        body: Option<&str>,
    ) {
        let (got_status, got_body) = self.get(coordinator, path).await;
        assert_eq!(
            got_status,
            status,
            "GET {} on coord-{}: {:?}, logs in {}",
            path,
            coordinator,
            got_body,
            self.root.display()
        );
        if let Some(body) = body {
            assert_eq!(got_body, body, "GET {} on coord-{}", path, coordinator);
        }
    }

    async fn eventually_get(
        &self,
        coordinator: usize,
        path: &str,
        status: StatusCode,
        body: Option<&str>,
    ) {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            self.check_alive();
            let (got_status, got_body) = self.get(coordinator, path).await;
            if got_status == status && body.map(|b| b == got_body).unwrap_or(true) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "GET {} on coord-{} gave {} {:?}, expected {} {:?}, logs in {}",
                path,
                coordinator,
                got_status,
                got_body,
                status,
                body,
                self.root.display()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    async fn volume_keys(&self, volume: usize) -> Option<u64> {
        self.json(self.volumes[volume].url("/health"))
            .await
            .and_then(|h| h["total_keys"].as_u64())
    }

    async fn eventually_volume_keys(&self, volume: usize, expected: u64) {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            self.check_alive();
            let keys = self.volume_keys(volume).await;
            if keys == Some(expected) {
                return;
            }
            assert!(
                Instant::now() < deadline,
                "vol-{} holds {:?} blob(s), expected {}, logs in {}",
                volume,
                keys,
                expected,
                self.root.display()
            );
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    }

    fn cleanup(mut self) {
        for node in self.coordinators.iter_mut().chain(self.volumes.iter_mut()) {
            node.stop();
        }
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn three_coordinators_three_volumes() {
    let mut cluster = Cluster::start(3, 3);
    let all = [0, 1, 2];

    let leader = cluster.wait_for_leader(&all).await;
    let first_term = cluster.term(leader).await;
    for i in all {
        cluster.wait_for_volumes(i, 3).await;
    }

    let (status, body) = cluster.put(leader, "/s3/photos/cat.txt", "miaou").await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert!(body.contains("on 3 volume(s)"), "{}", body);

    let follower = all.into_iter().find(|&i| i != leader).unwrap();
    let response = cluster
        .client
        .put(cluster.coordinators[follower].url("/s3/photos/cat.txt"))
        .body("ignored")
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        response.headers()["x-minikv-leader"].to_str().unwrap(),
        format!("coord-{}", leader)
    );

    for i in all {
        cluster
            .assert_get(i, "/s3/photos/cat.txt", StatusCode::OK, Some("miaou"))
            .await;
    }
    for v in 0..3 {
        cluster.eventually_volume_keys(v, 1).await;
    }

    let (status, body) = cluster.put(leader, "/s3/photos/cat.txt", "ronron").await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    for i in all {
        cluster
            .assert_get(i, "/s3/photos/cat.txt", StatusCode::OK, Some("ronron"))
            .await;
    }
    for v in 0..3 {
        cluster.eventually_volume_keys(v, 1).await;
    }

    cluster.coordinators[leader].stop();
    let survivors: Vec<usize> = all.into_iter().filter(|&i| i != leader).collect();
    let new_leader = cluster.wait_for_leader(&survivors).await;
    assert!(cluster.term(new_leader).await > first_term);

    cluster
        .eventually_get(
            new_leader,
            "/s3/photos/cat.txt",
            StatusCode::OK,
            Some("ronron"),
        )
        .await;
    let (status, body) = cluster.put(new_leader, "/s3/photos/dog.txt", "wouf").await;
    assert_eq!(status, StatusCode::OK, "{}", body);
    assert!(body.contains("on 3 volume(s)"), "{}", body);
    for &i in &survivors {
        cluster
            .assert_get(i, "/s3/photos/dog.txt", StatusCode::OK, Some("wouf"))
            .await;
    }

    cluster.volumes[0].stop();
    cluster.volumes[1].stop();
    for &i in &survivors {
        cluster
            .assert_get(i, "/s3/photos/cat.txt", StatusCode::OK, Some("ronron"))
            .await;
        cluster
            .assert_get(i, "/s3/photos/dog.txt", StatusCode::OK, Some("wouf"))
            .await;
    }

    let response = cluster
        .client
        .delete(cluster.coordinators[new_leader].url("/s3/photos/cat.txt"))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NO_CONTENT);
    for &i in &survivors {
        cluster
            .assert_get(i, "/s3/photos/cat.txt", StatusCode::NOT_FOUND, None)
            .await;
    }

    let root = cluster.root.clone();
    cluster.coordinators[leader].start(&root);
    cluster.wait_for_leader(&all).await;
    cluster
        .eventually_get(leader, "/s3/photos/dog.txt", StatusCode::OK, Some("wouf"))
        .await;
    cluster
        .eventually_get(leader, "/s3/photos/cat.txt", StatusCode::NOT_FOUND, None)
        .await;

    cluster.cleanup();
}
