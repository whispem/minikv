//! Enhanced metrics collection (v0.5.0)
//!
//! This module provides comprehensive Prometheus-compatible metrics including:
//! - Request latency histograms
//! - Request counters by endpoint and status
//! - Error rates
//! - System metrics

use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Histogram bucket boundaries for latency measurements (in milliseconds)
const LATENCY_BUCKETS: [f64; 11] = [
    1.0, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0,
];

/// A simple histogram implementation for latency tracking
#[derive(Debug)]
pub struct Histogram {
    buckets: Vec<AtomicU64>,
    boundaries: Vec<f64>,
    sum: AtomicU64,
    count: AtomicU64,
}

impl Histogram {
    /// Create a new histogram with default latency buckets
    pub fn new() -> Self {
        Self::with_buckets(&LATENCY_BUCKETS)
    }

    /// Create a histogram with custom bucket boundaries
    pub fn with_buckets(boundaries: &[f64]) -> Self {
        let mut buckets = Vec::with_capacity(boundaries.len() + 1);
        for _ in 0..=boundaries.len() {
            buckets.push(AtomicU64::new(0));
        }
        Self {
            buckets,
            boundaries: boundaries.to_vec(),
            sum: AtomicU64::new(0),
            count: AtomicU64::new(0),
        }
    }

    /// Record a value in the histogram
    pub fn observe(&self, value: f64) {
        // Find the bucket
        let mut bucket_idx = self.boundaries.len();
        for (i, &boundary) in self.boundaries.iter().enumerate() {
            if value <= boundary {
                bucket_idx = i;
                break;
            }
        }

        self.buckets[bucket_idx].fetch_add(1, Ordering::Relaxed);
        self.sum
            .fetch_add((value * 1000.0) as u64, Ordering::Relaxed); // Store as microseconds for precision
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Get histogram data for Prometheus format
    pub fn get_buckets(&self) -> Vec<(f64, u64)> {
        let mut cumulative = 0u64;
        let mut result = Vec::with_capacity(self.boundaries.len() + 1);

        for (i, &boundary) in self.boundaries.iter().enumerate() {
            cumulative += self.buckets[i].load(Ordering::Relaxed);
            result.push((boundary, cumulative));
        }

        // +Inf bucket
        cumulative += self.buckets[self.boundaries.len()].load(Ordering::Relaxed);
        result.push((f64::INFINITY, cumulative));

        result
    }

    /// Get sum of all observed values
    pub fn sum(&self) -> f64 {
        self.sum.load(Ordering::Relaxed) as f64 / 1000.0 // Convert back from microseconds
    }

    /// Get count of observations
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::Relaxed)
    }
}

impl Default for Histogram {
    fn default() -> Self {
        Self::new()
    }
}

/// Counter for tracking request counts
#[derive(Debug, Default)]
pub struct Counter {
    value: AtomicU64,
}

impl Counter {
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add(&self, n: u64) {
        self.value.fetch_add(n, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Gauge for tracking current values
#[derive(Debug, Default)]
pub struct Gauge {
    value: AtomicU64,
}

impl Gauge {
    pub fn new() -> Self {
        Self {
            value: AtomicU64::new(0),
        }
    }

    pub fn set(&self, v: u64) {
        self.value.store(v, Ordering::Relaxed);
    }

    pub fn inc(&self) {
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec(&self) {
        self.value.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn get(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }
}

/// Endpoint metrics
#[derive(Debug)]
pub struct EndpointMetrics {
    pub requests_total: Counter,
    pub requests_success: Counter,
    pub requests_error: Counter,
    pub latency: Histogram,
}

impl EndpointMetrics {
    pub fn new() -> Self {
        Self {
            requests_total: Counter::new(),
            requests_success: Counter::new(),
            requests_error: Counter::new(),
            latency: Histogram::new(),
        }
    }
}

impl Default for EndpointMetrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Global metrics registry
#[derive(Debug)]
pub struct MetricsRegistry {
    /// Per-endpoint metrics
    endpoints: Mutex<HashMap<String, Arc<EndpointMetrics>>>,

    /// Global counters
    pub total_requests: Counter,
    pub total_errors: Counter,
    pub total_bytes_read: Counter,
    pub total_bytes_written: Counter,

    /// Gauges
    pub active_connections: Gauge,
    pub keys_with_ttl: Gauge,
    pub compressed_blobs: Gauge,
    pub rate_limited_requests: Counter,

    /// Start time for uptime calculation
    start_time: Instant,
}

impl MetricsRegistry {
    /// Create a new metrics registry
    pub fn new() -> Self {
        Self {
            endpoints: Mutex::new(HashMap::new()),
            total_requests: Counter::new(),
            total_errors: Counter::new(),
            total_bytes_read: Counter::new(),
            total_bytes_written: Counter::new(),
            active_connections: Gauge::new(),
            keys_with_ttl: Gauge::new(),
            compressed_blobs: Gauge::new(),
            rate_limited_requests: Counter::new(),
            start_time: Instant::now(),
        }
    }

    /// Get or create metrics for an endpoint
    pub fn endpoint(&self, path: &str) -> Arc<EndpointMetrics> {
        let mut endpoints = self.endpoints.lock().unwrap();
        endpoints
            .entry(path.to_string())
            .or_insert_with(|| Arc::new(EndpointMetrics::new()))
            .clone()
    }

    /// Record a request
    pub fn record_request(&self, path: &str, duration: Duration, success: bool) {
        let endpoint = self.endpoint(path);

        endpoint.requests_total.inc();
        endpoint.latency.observe(duration.as_secs_f64() * 1000.0); // Convert to ms

        self.total_requests.inc();

        if success {
            endpoint.requests_success.inc();
        } else {
            endpoint.requests_error.inc();
            self.total_errors.inc();
        }
    }

    /// Get uptime in seconds
    pub fn uptime_seconds(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Generate Prometheus-compatible metrics output
    pub fn to_prometheus(&self) -> String {
        use std::fmt::Write;
        let mut out = String::new();

        // Global metrics
        out.push_str("# HELP minikv_requests_total Total number of requests\n");
        out.push_str("# TYPE minikv_requests_total counter\n");
        writeln!(out, "minikv_requests_total {}", self.total_requests.get()).unwrap();

        out.push_str("# HELP minikv_errors_total Total number of errors\n");
        out.push_str("# TYPE minikv_errors_total counter\n");
        writeln!(out, "minikv_errors_total {}", self.total_errors.get()).unwrap();

        out.push_str("# HELP minikv_bytes_read_total Total bytes read\n");
        out.push_str("# TYPE minikv_bytes_read_total counter\n");
        writeln!(
            out,
            "minikv_bytes_read_total {}",
            self.total_bytes_read.get()
        )
        .unwrap();

        out.push_str("# HELP minikv_bytes_written_total Total bytes written\n");
        out.push_str("# TYPE minikv_bytes_written_total counter\n");
        writeln!(
            out,
            "minikv_bytes_written_total {}",
            self.total_bytes_written.get()
        )
        .unwrap();

        out.push_str("# HELP minikv_active_connections Current active connections\n");
        out.push_str("# TYPE minikv_active_connections gauge\n");
        writeln!(
            out,
            "minikv_active_connections {}",
            self.active_connections.get()
        )
        .unwrap();

        out.push_str("# HELP minikv_keys_with_ttl Number of keys with TTL\n");
        out.push_str("# TYPE minikv_keys_with_ttl gauge\n");
        writeln!(out, "minikv_keys_with_ttl {}", self.keys_with_ttl.get()).unwrap();

        out.push_str("# HELP minikv_rate_limited_requests Total rate limited requests\n");
        out.push_str("# TYPE minikv_rate_limited_requests counter\n");
        writeln!(
            out,
            "minikv_rate_limited_requests {}",
            self.rate_limited_requests.get()
        )
        .unwrap();

        out.push_str("# HELP minikv_uptime_seconds Server uptime in seconds\n");
        out.push_str("# TYPE minikv_uptime_seconds gauge\n");
        writeln!(out, "minikv_uptime_seconds {}", self.uptime_seconds()).unwrap();

        // Per-endpoint metrics
        let endpoints = self.endpoints.lock().unwrap();

        out.push_str("# HELP minikv_endpoint_requests_total Requests per endpoint\n");
        out.push_str("# TYPE minikv_endpoint_requests_total counter\n");
        for (path, metrics) in endpoints.iter() {
            writeln!(
                out,
                "minikv_endpoint_requests_total{{path=\"{}\"}} {}",
                path,
                metrics.requests_total.get()
            )
            .unwrap();
        }

        out.push_str("# HELP minikv_endpoint_errors_total Errors per endpoint\n");
        out.push_str("# TYPE minikv_endpoint_errors_total counter\n");
        for (path, metrics) in endpoints.iter() {
            writeln!(
                out,
                "minikv_endpoint_errors_total{{path=\"{}\"}} {}",
                path,
                metrics.requests_error.get()
            )
            .unwrap();
        }

        // Latency histograms
        out.push_str("# HELP minikv_request_duration_ms Request duration in milliseconds\n");
        out.push_str("# TYPE minikv_request_duration_ms histogram\n");
        for (path, metrics) in endpoints.iter() {
            for (le, count) in metrics.latency.get_buckets() {
                if le.is_infinite() {
                    writeln!(
                        out,
                        "minikv_request_duration_ms_bucket{{path=\"{}\",le=\"+Inf\"}} {}",
                        path, count
                    )
                    .unwrap();
                } else {
                    writeln!(
                        out,
                        "minikv_request_duration_ms_bucket{{path=\"{}\",le=\"{}\"}} {}",
                        path, le, count
                    )
                    .unwrap();
                }
            }
            writeln!(
                out,
                "minikv_request_duration_ms_sum{{path=\"{}\"}} {}",
                path,
                metrics.latency.sum()
            )
            .unwrap();
            writeln!(
                out,
                "minikv_request_duration_ms_count{{path=\"{}\"}} {}",
                path,
                metrics.latency.count()
            )
            .unwrap();
        }

        out
    }
}

impl Default for MetricsRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Global metrics instance
pub static METRICS: once_cell::sync::Lazy<MetricsRegistry> =
    once_cell::sync::Lazy::new(MetricsRegistry::new);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_histogram() {
        let hist = Histogram::new();

        hist.observe(5.0);
        hist.observe(50.0);
        hist.observe(500.0);

        assert_eq!(hist.count(), 3);

        let buckets = hist.get_buckets();
        assert!(!buckets.is_empty());
    }

    #[test]
    fn test_counter() {
        let counter = Counter::new();

        assert_eq!(counter.get(), 0);
        counter.inc();
        assert_eq!(counter.get(), 1);
        counter.add(5);
        assert_eq!(counter.get(), 6);
    }

    #[test]
    fn test_gauge() {
        let gauge = Gauge::new();

        assert_eq!(gauge.get(), 0);
        gauge.set(10);
        assert_eq!(gauge.get(), 10);
        gauge.inc();
        assert_eq!(gauge.get(), 11);
        gauge.dec();
        assert_eq!(gauge.get(), 10);
    }

    #[test]
    fn test_metrics_registry() {
        let registry = MetricsRegistry::new();

        registry.record_request("/test", Duration::from_millis(50), true);
        registry.record_request("/test", Duration::from_millis(100), false);

        assert_eq!(registry.total_requests.get(), 2);
        assert_eq!(registry.total_errors.get(), 1);

        let endpoint = registry.endpoint("/test");
        assert_eq!(endpoint.requests_total.get(), 2);
        assert_eq!(endpoint.requests_success.get(), 1);
        assert_eq!(endpoint.requests_error.get(), 1);
    }
}
