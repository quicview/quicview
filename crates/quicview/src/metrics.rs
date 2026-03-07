use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Application-wide metrics using atomic counters.
///
/// Thread-safe, lock-free counters for tracking frame throughput,
/// byte counts, and connection activity. Designed for export to
/// Prometheus, logging, or the CLI status display.
#[derive(Debug, Clone)]
pub struct Metrics {
    inner: Arc<MetricsInner>,
}

#[derive(Debug)]
struct MetricsInner {
    start_time: Instant,
    frames_sent: AtomicU64,
    frames_received: AtomicU64,
    bytes_sent: AtomicU64,
    bytes_received: AtomicU64,
    active_connections: AtomicU64,
    total_connections: AtomicU64,
    errors: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                start_time: Instant::now(),
                frames_sent: AtomicU64::new(0),
                frames_received: AtomicU64::new(0),
                bytes_sent: AtomicU64::new(0),
                bytes_received: AtomicU64::new(0),
                active_connections: AtomicU64::new(0),
                total_connections: AtomicU64::new(0),
                errors: AtomicU64::new(0),
            }),
        }
    }

    pub fn record_frame_sent(&self, bytes: u64) {
        self.inner.frames_sent.fetch_add(1, Ordering::Relaxed);
        self.inner.bytes_sent.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn record_frame_received(&self, bytes: u64) {
        self.inner.frames_received.fetch_add(1, Ordering::Relaxed);
        self.inner
            .bytes_received
            .fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn connection_opened(&self) {
        self.inner
            .active_connections
            .fetch_add(1, Ordering::Relaxed);
        self.inner
            .total_connections
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn connection_closed(&self) {
        self.inner
            .active_connections
            .fetch_sub(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.inner.errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        MetricsSnapshot {
            uptime_secs: self.inner.start_time.elapsed().as_secs(),
            frames_sent: self.inner.frames_sent.load(Ordering::Relaxed),
            frames_received: self.inner.frames_received.load(Ordering::Relaxed),
            bytes_sent: self.inner.bytes_sent.load(Ordering::Relaxed),
            bytes_received: self.inner.bytes_received.load(Ordering::Relaxed),
            active_connections: self.inner.active_connections.load(Ordering::Relaxed),
            total_connections: self.inner.total_connections.load(Ordering::Relaxed),
            errors: self.inner.errors.load(Ordering::Relaxed),
        }
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Point-in-time snapshot of all metrics.
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub uptime_secs: u64,
    pub frames_sent: u64,
    pub frames_received: u64,
    pub bytes_sent: u64,
    pub bytes_received: u64,
    pub active_connections: u64,
    pub total_connections: u64,
    pub errors: u64,
}

impl std::fmt::Display for MetricsSnapshot {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "QuicView Metrics")?;
        writeln!(f, "  uptime:             {}s", self.uptime_secs)?;
        writeln!(f, "  frames sent:        {}", self.frames_sent)?;
        writeln!(f, "  frames received:    {}", self.frames_received)?;
        writeln!(
            f,
            "  bytes sent:         {} ({:.1} MB)",
            self.bytes_sent,
            self.bytes_sent as f64 / 1_048_576.0
        )?;
        writeln!(
            f,
            "  bytes received:     {} ({:.1} MB)",
            self.bytes_received,
            self.bytes_received as f64 / 1_048_576.0
        )?;
        writeln!(f, "  active connections: {}", self.active_connections)?;
        writeln!(f, "  total connections:  {}", self.total_connections)?;
        write!(f, "  errors:             {}", self.errors)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_frame_counting() {
        let m = Metrics::new();
        m.record_frame_sent(1024);
        m.record_frame_sent(2048);
        m.record_frame_received(512);

        let snap = m.snapshot();
        assert_eq!(snap.frames_sent, 2);
        assert_eq!(snap.bytes_sent, 3072);
        assert_eq!(snap.frames_received, 1);
        assert_eq!(snap.bytes_received, 512);
    }

    #[test]
    fn metrics_connection_tracking() {
        let m = Metrics::new();
        m.connection_opened();
        m.connection_opened();
        assert_eq!(m.snapshot().active_connections, 2);
        assert_eq!(m.snapshot().total_connections, 2);

        m.connection_closed();
        assert_eq!(m.snapshot().active_connections, 1);
        assert_eq!(m.snapshot().total_connections, 2);
    }

    #[test]
    fn metrics_clone_shares_state() {
        let m1 = Metrics::new();
        let m2 = m1.clone();
        m1.record_frame_sent(100);
        assert_eq!(m2.snapshot().frames_sent, 1);
    }

    #[test]
    fn metrics_snapshot_display() {
        let m = Metrics::new();
        m.record_frame_sent(1_048_576);
        let text = m.snapshot().to_string();
        assert!(text.contains("frames sent:"));
        assert!(text.contains("1.0 MB"));
    }
}
