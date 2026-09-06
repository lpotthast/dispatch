use std::{
    collections::BTreeMap,
    future::Future,
    sync::{
        Arc, Mutex, OnceLock, PoisonError, RwLock,
        atomic::{AtomicU64, Ordering},
    },
    time::Instant,
};

use dispatch_types::{
    BackendMetricsSnapshot, MetricBucketView, MetricLabelView, MetricSeriesView, MetricValueView,
    REPOSITORY_DURATION_METRIC, SQL_DURATION_METRIC, SQL_QUERY_COUNT_METRIC,
    SQL_ROWS_AFFECTED_METRIC, SQL_ROWS_RETURNED_METRIC,
};
use metrics::{
    Counter, CounterFn, Gauge, GaugeFn, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};
use rootcause::{Result, report};

use crate::backend::storage::utc_now;

const DURATION_BUCKET_BOUNDS: [f64; 16] = [
    0.000_1, 0.000_25, 0.000_5, 0.001, 0.002_5, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5,
    5.0, 10.0,
];

static METRICS: OnceLock<MetricsHandle> = OnceLock::new();

pub(crate) fn install() -> Result<()> {
    let recorder = DispatchRecorder::default();
    let handle = recorder.handle();
    metrics::set_global_recorder(recorder)
        .map_err(|err| report!("failed to install metrics recorder: {err}"))?;
    METRICS
        .set(handle)
        .map_err(|_| report!("metrics recorder is already installed"))?;

    metrics::describe_histogram!(
        REPOSITORY_DURATION_METRIC,
        Unit::Seconds,
        "Elapsed time for backend repository operations."
    );
    metrics::describe_histogram!(
        SQL_DURATION_METRIC,
        Unit::Seconds,
        "Elapsed time reported by SQLx for completed database queries."
    );
    metrics::describe_counter!(
        SQL_QUERY_COUNT_METRIC,
        Unit::Count,
        "Completed database queries reported by SQLx."
    );
    metrics::describe_counter!(
        SQL_ROWS_RETURNED_METRIC,
        Unit::Count,
        "Rows returned by completed database queries."
    );
    metrics::describe_counter!(
        SQL_ROWS_AFFECTED_METRIC,
        Unit::Count,
        "Rows affected by completed database queries."
    );
    Ok(())
}

pub(crate) fn snapshot() -> BackendMetricsSnapshot {
    METRICS
        .get()
        .map(MetricsHandle::snapshot)
        .unwrap_or_else(|| BackendMetricsSnapshot {
            captured_at: utc_now(),
            series: Vec::new(),
        })
}

pub(crate) async fn time_repository<T, E>(
    operation: &'static str,
    future: impl Future<Output = std::result::Result<T, E>>,
) -> std::result::Result<T, E> {
    let started = Instant::now();
    let result = future.await;
    let outcome = if result.is_ok() { "success" } else { "error" };
    metrics::histogram!(
        REPOSITORY_DURATION_METRIC,
        "operation" => operation,
        "outcome" => outcome
    )
    .record(started.elapsed().as_secs_f64());
    result
}

#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct MetricKey {
    name: String,
    labels: Vec<MetricLabelView>,
}

impl MetricKey {
    fn from_metrics(key: &Key) -> Self {
        let mut labels = key
            .labels()
            .map(|label| MetricLabelView {
                key: label.key().to_owned(),
                value: label.value().to_owned(),
            })
            .collect::<Vec<_>>();
        labels.sort_by(|left, right| {
            left.key
                .cmp(&right.key)
                .then_with(|| left.value.cmp(&right.value))
        });
        Self {
            name: key.name().to_owned(),
            labels,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum MetricKind {
    Counter,
    Gauge,
    Histogram,
}

#[derive(Clone, Debug)]
struct MetricDescription {
    description: String,
    unit: Option<String>,
}

#[derive(Debug, Default)]
struct RecorderState {
    counters: RwLock<BTreeMap<MetricKey, Arc<CounterMetric>>>,
    gauges: RwLock<BTreeMap<MetricKey, Arc<GaugeMetric>>>,
    histograms: RwLock<BTreeMap<MetricKey, Arc<HistogramMetric>>>,
    descriptions: RwLock<BTreeMap<(MetricKind, String), MetricDescription>>,
}

impl RecorderState {
    fn describe(
        &self,
        kind: MetricKind,
        key: KeyName,
        unit: Option<Unit>,
        description: SharedString,
    ) {
        write_lock(&self.descriptions).insert(
            (kind, key.as_str().to_owned()),
            MetricDescription {
                description: description.to_string(),
                unit: unit.map(|unit| unit.as_str().to_owned()),
            },
        );
    }

    fn counter(&self, key: &Key) -> Arc<CounterMetric> {
        get_or_insert(&self.counters, MetricKey::from_metrics(key))
    }

    fn gauge(&self, key: &Key) -> Arc<GaugeMetric> {
        get_or_insert(&self.gauges, MetricKey::from_metrics(key))
    }

    fn histogram(&self, key: &Key) -> Arc<HistogramMetric> {
        get_or_insert(&self.histograms, MetricKey::from_metrics(key))
    }
}

fn get_or_insert<T: Default>(
    values: &RwLock<BTreeMap<MetricKey, Arc<T>>>,
    key: MetricKey,
) -> Arc<T> {
    if let Some(value) = read_lock(values).get(&key).cloned() {
        return value;
    }
    write_lock(values)
        .entry(key)
        .or_insert_with(|| Arc::new(T::default()))
        .clone()
}

#[derive(Clone, Debug, Default)]
struct DispatchRecorder {
    state: Arc<RecorderState>,
}

impl DispatchRecorder {
    fn handle(&self) -> MetricsHandle {
        MetricsHandle {
            state: Arc::clone(&self.state),
        }
    }
}

impl Recorder for DispatchRecorder {
    fn describe_counter(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.state
            .describe(MetricKind::Counter, key, unit, description);
    }

    fn describe_gauge(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.state
            .describe(MetricKind::Gauge, key, unit, description);
    }

    fn describe_histogram(&self, key: KeyName, unit: Option<Unit>, description: SharedString) {
        self.state
            .describe(MetricKind::Histogram, key, unit, description);
    }

    fn register_counter(&self, key: &Key, _metadata: &Metadata<'_>) -> Counter {
        Counter::from_arc(self.state.counter(key))
    }

    fn register_gauge(&self, key: &Key, _metadata: &Metadata<'_>) -> Gauge {
        Gauge::from_arc(self.state.gauge(key))
    }

    fn register_histogram(&self, key: &Key, _metadata: &Metadata<'_>) -> Histogram {
        Histogram::from_arc(self.state.histogram(key))
    }
}

#[derive(Clone, Debug)]
struct MetricsHandle {
    state: Arc<RecorderState>,
}

impl MetricsHandle {
    fn snapshot(&self) -> BackendMetricsSnapshot {
        let descriptions = read_lock(&self.state.descriptions);
        let mut series = Vec::new();
        for (key, metric) in read_lock(&self.state.counters).iter() {
            series.push(metric_series(
                key,
                descriptions.get(&(MetricKind::Counter, key.name.clone())),
                MetricValueView::Counter {
                    value: metric.value.load(Ordering::Relaxed),
                },
            ));
        }
        for (key, metric) in read_lock(&self.state.gauges).iter() {
            series.push(metric_series(
                key,
                descriptions.get(&(MetricKind::Gauge, key.name.clone())),
                MetricValueView::Gauge {
                    value: f64::from_bits(metric.value.load(Ordering::Relaxed)),
                },
            ));
        }
        for (key, metric) in read_lock(&self.state.histograms).iter() {
            let state = mutex_lock(&metric.state);
            let mut buckets = DURATION_BUCKET_BOUNDS
                .iter()
                .copied()
                .zip(state.buckets.iter().copied())
                .map(|(upper_bound, count)| MetricBucketView {
                    upper_bound: Some(upper_bound),
                    count,
                })
                .collect::<Vec<_>>();
            buckets.push(MetricBucketView {
                upper_bound: None,
                count: state.count,
            });
            series.push(metric_series(
                key,
                descriptions.get(&(MetricKind::Histogram, key.name.clone())),
                MetricValueView::Histogram {
                    count: state.count,
                    sum: state.sum,
                    min: state.min,
                    max: state.max,
                    buckets,
                },
            ));
        }
        BackendMetricsSnapshot {
            captured_at: utc_now(),
            series,
        }
    }
}

fn metric_series(
    key: &MetricKey,
    description: Option<&MetricDescription>,
    value: MetricValueView,
) -> MetricSeriesView {
    MetricSeriesView {
        name: key.name.clone(),
        description: description.map(|description| description.description.clone()),
        unit: description.and_then(|description| description.unit.clone()),
        labels: key.labels.clone(),
        value,
    }
}

#[derive(Debug, Default)]
struct CounterMetric {
    value: AtomicU64,
}

impl CounterFn for CounterMetric {
    fn increment(&self, value: u64) {
        self.value.fetch_add(value, Ordering::Relaxed);
    }

    fn absolute(&self, value: u64) {
        self.value.fetch_max(value, Ordering::Relaxed);
    }
}

#[derive(Debug, Default)]
struct GaugeMetric {
    value: AtomicU64,
}

impl GaugeMetric {
    fn update(&self, update: impl Fn(f64) -> f64) {
        let mut current = self.value.load(Ordering::Relaxed);
        loop {
            let next = update(f64::from_bits(current)).to_bits();
            match self.value.compare_exchange_weak(
                current,
                next,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(observed) => current = observed,
            }
        }
    }
}

impl GaugeFn for GaugeMetric {
    fn increment(&self, value: f64) {
        self.update(|current| current + value);
    }

    fn decrement(&self, value: f64) {
        self.update(|current| current - value);
    }

    fn set(&self, value: f64) {
        self.value.store(value.to_bits(), Ordering::Relaxed);
    }
}

#[derive(Debug)]
struct HistogramState {
    count: u64,
    sum: f64,
    min: Option<f64>,
    max: Option<f64>,
    buckets: [u64; DURATION_BUCKET_BOUNDS.len()],
}

impl Default for HistogramState {
    fn default() -> Self {
        Self {
            count: 0,
            sum: 0.0,
            min: None,
            max: None,
            buckets: [0; DURATION_BUCKET_BOUNDS.len()],
        }
    }
}

#[derive(Debug, Default)]
struct HistogramMetric {
    state: Mutex<HistogramState>,
}

impl HistogramFn for HistogramMetric {
    fn record(&self, value: f64) {
        if !value.is_finite() {
            return;
        }
        let mut state = mutex_lock(&self.state);
        state.count = state.count.saturating_add(1);
        state.sum += value;
        state.min = Some(state.min.map_or(value, |current| current.min(value)));
        state.max = Some(state.max.map_or(value, |current| current.max(value)));
        for (index, upper_bound) in DURATION_BUCKET_BOUNDS.iter().enumerate() {
            if value <= *upper_bound {
                state.buckets[index] = state.buckets[index].saturating_add(1);
            }
        }
    }
}

fn read_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockReadGuard<'_, T> {
    lock.read().unwrap_or_else(PoisonError::into_inner)
}

fn write_lock<T>(lock: &RwLock<T>) -> std::sync::RwLockWriteGuard<'_, T> {
    lock.write().unwrap_or_else(PoisonError::into_inner)
}

fn mutex_lock<T>(lock: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;

    use super::*;

    #[test]
    fn recorder_keeps_cumulative_bounded_histograms() {
        let recorder = DispatchRecorder::default();
        let handle = recorder.handle();

        metrics::with_local_recorder(&recorder, || {
            metrics::describe_histogram!("test_duration", Unit::Seconds, "Test duration");
            let histogram = metrics::histogram!("test_duration", "operation" => "load");
            histogram.record(0.001);
            histogram.record(2.0);
        });

        let snapshot = handle.snapshot();
        assert_that!(&(snapshot.series.len())).is_equal_to(1);
        let MetricValueView::Histogram {
            count,
            sum,
            min,
            max,
            buckets,
        } = &snapshot.series[0].value
        else {
            panic!("expected histogram")
        };
        assert_that!(count).is_equal_to(2);
        assert_that!(sum).is_equal_to(2.001);
        assert_that!(min).is_equal_to(Some(0.001));
        assert_that!(max).is_equal_to(Some(2.0));
        assert_that!(&(buckets.len())).is_equal_to(DURATION_BUCKET_BOUNDS.len() + 1);
        assert_that!(&(buckets.last().map(|bucket| bucket.count))).is_equal_to(Some(2));
    }

    #[test]
    fn recorder_supports_counter_and_gauge_handles() {
        let recorder = DispatchRecorder::default();
        let handle = recorder.handle();

        metrics::with_local_recorder(&recorder, || {
            metrics::counter!("test_counter").increment(3);
            metrics::gauge!("test_gauge").set(2.5);
            metrics::gauge!("test_gauge").increment(0.5);
        });

        let snapshot = handle.snapshot();
        assert_that!(&(snapshot.series.len())).is_equal_to(2);
        assert_that!(&snapshot.series[0].value).is_equal_to(MetricValueView::Counter { value: 3 });
        assert_that!(&snapshot.series[1].value).is_equal_to(MetricValueView::Gauge { value: 3.0 });
    }
}
