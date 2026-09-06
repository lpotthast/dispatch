use serde::{Deserialize, Serialize};

pub const REPOSITORY_DURATION_METRIC: &str = "dispatch_repository_operation_duration_seconds";
pub const SQL_DURATION_METRIC: &str = "dispatch_sql_query_duration_seconds";
pub const SQL_QUERY_COUNT_METRIC: &str = "dispatch_sql_queries_total";
pub const SQL_ROWS_RETURNED_METRIC: &str = "dispatch_sql_rows_returned_total";
pub const SQL_ROWS_AFFECTED_METRIC: &str = "dispatch_sql_rows_affected_total";

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct BackendMetricsSnapshot {
    pub captured_at: String,
    pub series: Vec<MetricSeriesView>,
}

#[derive(Clone, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct MetricLabelView {
    pub key: String,
    pub value: String,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetricSeriesView {
    pub name: String,
    pub description: Option<String>,
    pub unit: Option<String>,
    pub labels: Vec<MetricLabelView>,
    pub value: MetricValueView,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum MetricValueView {
    Counter {
        value: u64,
    },
    Gauge {
        value: f64,
    },
    Histogram {
        count: u64,
        sum: f64,
        min: Option<f64>,
        max: Option<f64>,
        buckets: Vec<MetricBucketView>,
    },
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct MetricBucketView {
    pub upper_bound: Option<f64>,
    pub count: u64,
}
