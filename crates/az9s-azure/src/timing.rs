//! Lightweight timing collection.
//!
//! Every interesting operation (Azure calls, cache writes, frame renders, event
//! handling) records its duration here. Recording is cheap and always on; the
//! summary is printed when az9s is started with `--timings`, which gives
//! before/after numbers for performance work without attaching a profiler.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tracing::debug;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stat {
    pub count: u64,
    pub total: Duration,
    pub max: Duration,
}

impl Stat {
    pub fn avg(&self) -> Duration {
        if self.count == 0 {
            Duration::ZERO
        } else {
            self.total / self.count as u32
        }
    }
}

fn registry() -> &'static Mutex<BTreeMap<&'static str, Stat>> {
    static REGISTRY: OnceLock<Mutex<BTreeMap<&'static str, Stat>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(BTreeMap::new()))
}

fn process_start_cell() -> &'static OnceLock<Instant> {
    static START: OnceLock<Instant> = OnceLock::new();
    &START
}

/// Mark the process start. Call as early as possible in `main`.
/// Later calls are ignored.
pub fn mark_process_start() {
    let _ = process_start_cell().set(Instant::now());
}

/// Time elapsed since [`mark_process_start`] (or since first use if never marked).
pub fn since_process_start() -> Duration {
    process_start_cell().get_or_init(Instant::now).elapsed()
}

/// Record one sample for `name`.
pub fn record(name: &'static str, elapsed: Duration) {
    debug!(target: "az9s::timing", op = name, elapsed_ms = elapsed.as_secs_f64() * 1000.0);
    if let Ok(mut map) = registry().lock() {
        let stat = map.entry(name).or_default();
        stat.count += 1;
        stat.total += elapsed;
        stat.max = stat.max.max(elapsed);
    }
}

/// Starts a timer that records into `name` when dropped.
#[must_use = "the timer records when dropped; binding it to `_` drops it immediately"]
pub fn timer(name: &'static str) -> Timer {
    Timer {
        name,
        start: Instant::now(),
    }
}

pub struct Timer {
    name: &'static str,
    start: Instant,
}

impl Drop for Timer {
    fn drop(&mut self) {
        record(self.name, self.start.elapsed());
    }
}

/// Snapshot of all recorded stats, sorted by name.
pub fn snapshot() -> Vec<(&'static str, Stat)> {
    registry()
        .lock()
        .map(|m| m.iter().map(|(k, v)| (*k, *v)).collect())
        .unwrap_or_default()
}

/// Human-readable table of all recorded stats.
pub fn summary() -> String {
    let rows = snapshot();
    let mut out = String::from("az9s timings\n");
    out.push_str(&format!(
        "{:<36} {:>8} {:>12} {:>12} {:>12}\n",
        "operation", "count", "total ms", "avg ms", "max ms"
    ));
    for (name, s) in rows {
        out.push_str(&format!(
            "{:<36} {:>8} {:>12.1} {:>12.2} {:>12.2}\n",
            name,
            s.count,
            ms(s.total),
            ms(s.avg()),
            ms(s.max)
        ));
    }
    out
}

fn ms(d: Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_accumulates_count_total_and_max() {
        record("test.accumulate", Duration::from_millis(2));
        record("test.accumulate", Duration::from_millis(6));
        let (_, s) = snapshot()
            .into_iter()
            .find(|(n, _)| *n == "test.accumulate")
            .unwrap();
        assert_eq!(s.count, 2);
        assert_eq!(s.total, Duration::from_millis(8));
        assert_eq!(s.max, Duration::from_millis(6));
        assert_eq!(s.avg(), Duration::from_millis(4));
    }

    #[test]
    fn timer_records_on_drop() {
        {
            let _t = timer("test.timer");
        }
        assert!(snapshot()
            .iter()
            .any(|(n, s)| *n == "test.timer" && s.count == 1));
    }

    #[test]
    fn summary_lists_operations() {
        record("test.summary", Duration::from_millis(1));
        assert!(summary().contains("test.summary"));
    }
}
