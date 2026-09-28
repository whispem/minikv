//! The `minikv` CLI keeps values byte for byte, accepts `--coordinator` before
//! or after the subcommand, and exits with status 1 when the coordinator
//! answers with an error or cannot be reached.

mod support;

use std::fs;
use support::{cli, stderr, Cluster};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn put_get_delete_keep_bytes_and_report_errors() {
    let cluster = Cluster::start(1);
    cluster.wait_ready(1).await;
    let dir = cluster.dir.path();
    let url = cluster.url.as_str();

    // Not valid UTF-8, and starts like a UTF-16 byte order mark.
    let value: Vec<u8> = vec![0xff, 0xfe, 0x00, 0x01, b'b', b'i', b'n', 0x80, b'\n'];
    let input = dir.join("in.bin");
    fs::write(&input, &value).unwrap();

    let put = cli(&[
        "--coordinator",
        url,
        "put",
        "bin",
        "--file",
        input.to_str().unwrap(),
    ]);
    assert!(put.status.success(), "{}", stderr(&put));

    // `--coordinator` after the subcommand.
    let output = dir.join("out.bin");
    let get = cli(&[
        "get",
        "bin",
        "--output",
        output.to_str().unwrap(),
        "--coordinator",
        url,
    ]);
    assert!(get.status.success(), "{}", stderr(&get));
    assert_eq!(fs::read(&output).unwrap(), value);

    // Without `--output`, the raw value goes to standard output.
    let get = cli(&["--coordinator", url, "get", "bin"]);
    assert!(get.status.success(), "{}", stderr(&get));
    assert_eq!(get.stdout, value);

    // A missing key is an error, and nothing is written.
    let missing = dir.join("missing.bin");
    let get = cli(&[
        "--coordinator",
        url,
        "get",
        "missing",
        "--output",
        missing.to_str().unwrap(),
    ]);
    assert_eq!(get.status.code(), Some(1), "{}", stderr(&get));
    assert!(
        stderr(&get).contains("error: GET missing failed: 404"),
        "{}",
        stderr(&get)
    );
    assert!(!missing.exists());

    let delete = cli(&["--coordinator", url, "delete", "missing"]);
    assert_eq!(delete.status.code(), Some(1), "{}", stderr(&delete));
    assert!(
        stderr(&delete).contains("error: DELETE missing failed: 404"),
        "{}",
        stderr(&delete)
    );

    let delete = cli(&["--coordinator", url, "delete", "bin"]);
    assert!(delete.status.success(), "{}", stderr(&delete));
    let get = cli(&["--coordinator", url, "get", "bin"]);
    assert_eq!(get.status.code(), Some(1), "{}", stderr(&get));
    assert!(get.stdout.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn put_fails_when_the_coordinator_refuses_the_write() {
    // No volume: the coordinator answers 503 after waiting for one.
    let cluster = Cluster::start(0);
    cluster.wait_ready(0).await;
    let input = cluster.dir.path().join("in.txt");
    fs::write(&input, b"value").unwrap();

    let put = cli(&[
        "--coordinator",
        &cluster.url,
        "put",
        "key",
        "--file",
        input.to_str().unwrap(),
    ]);
    assert_eq!(put.status.code(), Some(1), "{}", stderr(&put));
    assert!(
        stderr(&put).contains("error: PUT key failed: 503"),
        "{}",
        stderr(&put)
    );
}

#[test]
fn unreachable_coordinator_is_an_error() {
    let output = cli(&["--coordinator", "http://127.0.0.1:9", "get", "key"]);
    assert_eq!(output.status.code(), Some(1));
    assert!(
        stderr(&output).contains("error: cannot reach the coordinator at http://127.0.0.1:9"),
        "{}",
        stderr(&output)
    );
}
