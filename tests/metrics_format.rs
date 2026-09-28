//! `GET /metrics` follows the Prometheus text format (version 0.0.4) line by
//! line, and exports only values that minikv measures.

mod support;

use std::collections::{BTreeMap, HashSet};
use std::time::{Duration, Instant};
use support::Cluster;

#[derive(Debug, PartialEq)]
struct Sample {
    name: String,
    labels: Vec<(String, String)>,
    value: f64,
}

fn is_metric_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_' || c == ':')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == ':')
}

fn is_label_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_value(text: &str) -> Result<f64, String> {
    match text {
        "+Inf" => Ok(f64::INFINITY),
        "-Inf" => Ok(f64::NEG_INFINITY),
        "NaN" => Ok(f64::NAN),
        _ => text
            .parse::<f64>()
            .map_err(|_| format!("not a number: {:?}", text)),
    }
}

/// Parses `name{label="value",...} value [timestamp]`.
fn parse_sample(line: &str) -> Result<Sample, String> {
    let name_end = line.find(['{', ' ']).ok_or("no value")?;
    let name = &line[..name_end];
    if !is_metric_name(name) {
        return Err(format!("invalid metric name {:?}", name));
    }
    let mut rest = &line[name_end..];
    let mut labels = Vec::new();
    if let Some(after_brace) = rest.strip_prefix('{') {
        rest = after_brace;
        loop {
            if let Some(after) = rest.strip_prefix('}') {
                rest = after;
                break;
            }
            let equals = rest.find('=').ok_or("label without '='")?;
            let label = &rest[..equals];
            if !is_label_name(label) {
                return Err(format!("invalid label name {:?}", label));
            }
            rest = rest[equals + 1..]
                .strip_prefix('"')
                .ok_or("label value without quotes")?;
            let mut value = String::new();
            let mut chars = rest.char_indices();
            let end = loop {
                match chars.next().ok_or("unterminated label value")? {
                    (i, '"') => break i,
                    (_, '\\') => match chars.next().ok_or("dangling escape")?.1 {
                        '\\' => value.push('\\'),
                        '"' => value.push('"'),
                        'n' => value.push('\n'),
                        other => return Err(format!("invalid escape \\{}", other)),
                    },
                    (_, c) => value.push(c),
                }
            };
            labels.push((label.to_string(), value));
            rest = &rest[end + 1..];
            if let Some(after) = rest.strip_prefix(',') {
                rest = after;
            } else if !rest.starts_with('}') {
                return Err("expected ',' or '}' after a label".to_string());
            }
        }
    }
    let mut fields = rest
        .strip_prefix(' ')
        .ok_or("expected a space before the value")?
        .split(' ');
    let value = parse_value(fields.next().ok_or("no value")?)?;
    if let Some(timestamp) = fields.next() {
        timestamp
            .parse::<i64>()
            .map_err(|_| format!("invalid timestamp {:?}", timestamp))?;
    }
    if fields.next().is_some() {
        return Err("unexpected text after the value".to_string());
    }
    Ok(Sample {
        name: name.to_string(),
        labels,
        value,
    })
}

/// Parses a whole exposition, and checks the rules that apply across lines:
/// each family is declared once with a valid type before its samples, its
/// samples form one group, and no series appears twice.
fn parse_exposition(text: &str) -> Result<Vec<Sample>, String> {
    if !text.ends_with('\n') {
        return Err("the text does not end with a newline".to_string());
    }
    let mut types: BTreeMap<String, String> = BTreeMap::new();
    let mut finished_families: HashSet<String> = HashSet::new();
    let mut current_family: Option<String> = None;
    let mut series = HashSet::new();
    let mut samples = Vec::new();

    for (number, line) in text.lines().enumerate() {
        let context = |e: String| format!("line {}: {}: {:?}", number + 1, e, line);
        if line.is_empty() {
            continue;
        }
        if let Some(comment) = line.strip_prefix('#') {
            let words: Vec<&str> = comment.trim_start().splitn(3, ' ').collect();
            match words.as_slice() {
                ["TYPE", name, kind] => {
                    if !is_metric_name(name) {
                        return Err(context("invalid metric name".into()));
                    }
                    if !["counter", "gauge", "histogram", "summary", "untyped"].contains(kind) {
                        return Err(context("invalid type".into()));
                    }
                    if types.insert(name.to_string(), kind.to_string()).is_some() {
                        return Err(context("second TYPE line".into()));
                    }
                }
                ["HELP", name, ..] if !is_metric_name(name) => {
                    return Err(context("invalid metric name".into()));
                }
                _ => {}
            }
            continue;
        }
        let sample = parse_sample(line).map_err(context)?;
        if !types.contains_key(&sample.name) {
            return Err(context("sample without a TYPE line before it".into()));
        }
        if current_family.as_deref() != Some(sample.name.as_str()) {
            if finished_families.contains(&sample.name) {
                return Err(context("samples of the family are not grouped".into()));
            }
            if let Some(previous) = current_family.replace(sample.name.clone()) {
                finished_families.insert(previous);
            }
        }
        if !series.insert(format!("{}{:?}", sample.name, sample.labels)) {
            return Err(context("series already exported".into()));
        }
        samples.push(sample);
    }
    Ok(samples)
}

fn value_of(samples: &[Sample], name: &str, labels: &[(&str, &str)]) -> Option<f64> {
    samples
        .iter()
        .find(|s| {
            s.name == name
                && s.labels.len() == labels.len()
                && labels
                    .iter()
                    .all(|(k, v)| s.labels.iter().any(|(sk, sv)| sk == k && sv == v))
        })
        .map(|s| s.value)
}

#[test]
fn the_parser_rejects_invalid_text() {
    let valid = "# TYPE a gauge\na 1\n# TYPE b gauge\nb{x=\"1\",y=\"q\\\"\"} 2.5\n";
    assert_eq!(parse_exposition(valid).unwrap().len(), 2);

    for invalid in [
        // A text value, as 2.0.0 exported the Raft role.
        "# TYPE minikv_raft_role gauge\nminikv_raft_role \"leader\"\n",
        // No TYPE line: Prometheus would take it as untyped, but every minikv
        // family declares its type.
        "a 1\n",
        // A family split in two groups.
        "# TYPE a gauge\n# TYPE b gauge\na{x=\"1\"} 1\nb 1\na{x=\"2\"} 1\n",
        // The same series twice.
        "# TYPE a gauge\na 1\na 2\n",
        // An unterminated label value.
        "# TYPE a gauge\na{x=\"1} 1\n",
        // No final newline.
        "# TYPE a gauge\na 1",
    ] {
        assert!(parse_exposition(invalid).is_err(), "{:?}", invalid);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn metrics_follow_the_prometheus_text_format() {
    let cluster = Cluster::start(1);
    cluster.wait_ready(1).await;
    for key in ["first", "second"] {
        let response = cluster
            .client
            .put(cluster.url(&format!("/{}", key)))
            .body("value")
            .send()
            .await
            .unwrap();
        assert!(response.status().is_success());
    }

    // The blob counts come from the volume heartbeats (every 200 ms here).
    let deadline = Instant::now() + Duration::from_secs(20);
    let samples = loop {
        let response = cluster
            .client
            .get(cluster.url("/metrics"))
            .send()
            .await
            .unwrap();
        let content_type = response.headers()["content-type"].to_str().unwrap();
        assert!(content_type.starts_with("text/plain"), "{}", content_type);
        let text = response.text().await.unwrap();
        let samples = parse_exposition(&text).unwrap_or_else(|e| panic!("{}\n{}", e, text));
        if value_of(&samples, "minikv_total_keys", &[]) == Some(2.0) {
            break samples;
        }
        assert!(Instant::now() < deadline, "{}", text);
        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    let families: HashSet<&str> = samples.iter().map(|s| s.name.as_str()).collect();
    let expected: HashSet<&str> = [
        "minikv_healthy_volumes",
        "minikv_total_keys",
        "minikv_volume_bytes",
        "minikv_volume_total_keys",
        "minikv_raft_role",
        "minikv_raft_term",
        "minikv_raft_commit_index",
        "minikv_uptime_seconds",
        "minikv_s3_objects_total",
    ]
    .into_iter()
    .collect();
    assert_eq!(families, expected);

    assert_eq!(value_of(&samples, "minikv_healthy_volumes", &[]), Some(1.0));
    assert_eq!(
        value_of(
            &samples,
            "minikv_volume_total_keys",
            &[("volume_id", "vol-0")]
        ),
        Some(2.0)
    );
    assert_eq!(
        value_of(&samples, "minikv_s3_objects_total", &[]),
        Some(2.0)
    );
    assert_eq!(
        value_of(&samples, "minikv_raft_role", &[("role", "leader")]),
        Some(1.0)
    );
    for role in ["candidate", "follower"] {
        assert_eq!(
            value_of(&samples, "minikv_raft_role", &[("role", role)]),
            Some(0.0)
        );
    }
}
