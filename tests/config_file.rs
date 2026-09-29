//! The configuration file of `minikv-coord`.

mod support;

use minikv::common::{Config, CoordinatorConfig, NodeRole};
use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};
use support::{free_port, Process, COORD_BIN};

#[test]
fn the_example_file_loads() {
    let config: Config = config::Config::builder()
        .add_source(config::File::from_str(
            include_str!("../config.toml.example"),
            config::FileFormat::Toml,
        ))
        .build()
        .unwrap()
        .try_deserialize()
        .unwrap();

    assert_eq!(config.role, NodeRole::Coordinator);
    let coordinator = config.coordinator.expect("no [coordinator] section");
    let default = CoordinatorConfig::default();
    assert_eq!(coordinator.bind_addr, default.bind_addr);
    assert_eq!(coordinator.grpc_addr, default.grpc_addr);
    assert_eq!(coordinator.db_path, default.db_path);
    assert!(coordinator.peers.is_empty());
    assert_eq!(coordinator.replicas, default.replicas);
    // The settings that minikv-coord does not apply stay commented out.
    assert_eq!(coordinator.election_timeout_ms, default.election_timeout_ms);
    assert_eq!(
        coordinator.heartbeat_interval_ms,
        default.heartbeat_interval_ms
    );
    assert_eq!(coordinator.snapshot_threshold, default.snapshot_threshold);
    assert_eq!(coordinator.num_shards, default.num_shards);
    assert_eq!(coordinator.tls_cert_path, None);
    assert_eq!(coordinator.tls_key_path, None);
}

/// Starts `minikv-coord serve --id coord` in `dir`, with no other flag, and
/// waits until its HTTP API answers on `http`.
async fn start_coordinator(dir: &Path, http: u16) -> Process {
    let coordinator = Process::start(
        "coord",
        COORD_BIN,
        vec!["serve".into(), "--id".into(), "coord".into()],
        dir,
    );
    let client = reqwest::Client::new();
    let url = format!("http://127.0.0.1:{}/health/live", http);
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if matches!(client.get(&url).send().await, Ok(r) if r.status().is_success()) {
            return coordinator;
        }
        assert!(
            Instant::now() < deadline,
            "the coordinator does not answer on {}:\n{}",
            url,
            coordinator.log()
        );
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

/// The addresses of the `[coordinator]` section, which minikv-coord applies.
fn applied_settings(http: u16, grpc: u16) -> String {
    format!(
        "node_id = \"coord\"\nrole = \"coordinator\"\n\n[coordinator]\n\
         bind_addr = \"127.0.0.1:{}\"\ngrpc_addr = \"127.0.0.1:{}\"\n\
         db_path = \"db\"\npeers = []\n",
        http, grpc
    )
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn the_settings_minikv_coord_ignores_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let (http, grpc) = (free_port(), free_port());
    let settings = applied_settings(http, grpc)
        + "election_timeout_ms = 1000\nheartbeat_interval_ms = 100\n\
           snapshot_threshold = 5\nnum_shards = 64\n\
           tls_cert_path = \"cert.pem\"\ntls_key_path = \"key.pem\"\n";
    fs::write(dir.path().join("config.toml"), settings).unwrap();

    // The HTTP API answers without TLS, on the address of the file.
    let coordinator = start_coordinator(dir.path(), http).await;

    let log = coordinator.log();
    for setting in [
        "election_timeout_ms = 1000 (the coordinator uses 300)",
        "heartbeat_interval_ms = 100 (the coordinator uses 50)",
        "snapshot_threshold = 5 (reserved: there are no Raft snapshots)",
        "num_shards = 64 (reserved: placement uses no shards)",
        "tls_cert_path and tls_key_path (TLS stays off)",
    ] {
        let warning = format!("[coordinator] {} is not applied by minikv-coord", setting);
        assert!(log.contains(&warning), "no {:?} in:\n{}", warning, log);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_with_applied_settings_only_gives_no_warning() {
    let dir = tempfile::tempdir().unwrap();
    let (http, grpc) = (free_port(), free_port());
    fs::write(
        dir.path().join("config.toml"),
        applied_settings(http, grpc) + "election_timeout_ms = 300\n",
    )
    .unwrap();

    let coordinator = start_coordinator(dir.path(), http).await;

    let log = coordinator.log();
    assert!(!log.contains("is not applied"), "{}", log);
}
