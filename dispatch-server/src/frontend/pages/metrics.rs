use crate::{
    frontend::{
        components::{cached_query, selected_project_signal},
        services::metrics_service,
    },
    shared::view_models::{
        BackendMetricsSnapshot, MetricLabelView, MetricSeriesView, MetricValueView,
        REPOSITORY_DURATION_METRIC, SQL_DURATION_METRIC,
    },
};
use leptos::prelude::*;
use leptos_meta::Title;
use leptos_use::use_interval_fn;

const METRICS_REFRESH_INTERVAL_MS: u64 = 2_000;

#[component]
pub fn PageMetrics() -> impl IntoView {
    let selected_project = selected_project_signal();
    let service = metrics_service();
    let initial = service.cached_page_untracked(&selected_project.get_untracked());
    let service_for_cache = service.clone();
    let service_for_load = service.clone();
    let result = cached_query(
        initial,
        move || selected_project.get(),
        move |project| service_for_cache.cached_page(project),
        move |project| {
            let service = service_for_load.clone();
            let project = project.clone();
            async move { service.load_page(project).await }
        },
    );
    let refresh = result.refresh;
    let _poll = use_interval_fn(move || refresh.run(()), METRICS_REFRESH_INTERVAL_MS);

    view! {
        <Title text="Backend metrics"/>
        <div>
            <main class="page-shell metrics-page">
                <section class="page-heading metrics-page-heading">
                    <div>
                        <h1>"Backend metrics"</h1>
                        <p class="muted">
                            "Cumulative in-process measurements since server start. Values refresh every two seconds and reset when the server restarts."
                        </p>
                    </div>
                    <button class="secondary" on:click=move |_| refresh.run(())>"Refresh"</button>
                </section>
                {move || {
                    result
                        .value
                        .get()
                        .map(|page| view! { <MetricsContent snapshot=page.metrics/> }.into_any())
                        .unwrap_or_else(|| {
                            view! { <p class="muted" role="status">"Collecting metrics…"</p> }
                                .into_any()
                        })
                }}
            </main>
        </div>
    }
}

#[component]
fn MetricsContent(snapshot: BackendMetricsSnapshot) -> impl IntoView {
    let repository_rows = timing_rows(&snapshot, REPOSITORY_DURATION_METRIC, "operation");
    let sql_rows = timing_rows(&snapshot, SQL_DURATION_METRIC, "statement");
    let repository_calls = repository_rows.iter().map(|row| row.count).sum::<u64>();
    let sql_queries = sql_rows.iter().map(|row| row.count).sum::<u64>();
    let sql_time = sql_rows.iter().map(|row| row.sum).sum::<f64>();
    let other_series = snapshot
        .series
        .iter()
        .filter(|series| {
            series.name != REPOSITORY_DURATION_METRIC && series.name != SQL_DURATION_METRIC
        })
        .cloned()
        .collect::<Vec<_>>();

    view! {
        <section class="metrics-summary" aria-label="Metrics summary">
            <MetricSummary label="Repository calls" value=repository_calls.to_string()/>
            <MetricSummary label="SQL queries" value=sql_queries.to_string()/>
            <MetricSummary label="Observed SQL time" value=format_duration(sql_time)/>
            <MetricSummary label="Captured at" value=snapshot.captured_at/>
        </section>
        <TimingPanel
            title="Repository timings"
            description="Backend operations measured around repository boundaries. Nested rows make expensive phases visible without tying instrumentation to the UI."
            empty_message="No repository operations have completed yet."
            rows=repository_rows
        />
        <TimingPanel
            title="SQL timings"
            description="All completed SeaORM/SQLx queries, grouped by SQLx's normalized statement summary."
            empty_message="No SQL queries have completed yet."
            rows=sql_rows
        />
        <MetricSeriesPanel series=other_series/>
    }
}

#[component]
fn MetricSummary(label: &'static str, value: String) -> impl IntoView {
    view! {
        <article class="panel metric-summary-card">
            <span class="muted">{label}</span>
            <strong>{value}</strong>
        </article>
    }
}

#[derive(Clone, Debug, PartialEq)]
struct TimingRow {
    name: String,
    labels: String,
    count: u64,
    sum: f64,
    average: f64,
    maximum: Option<f64>,
    p95_upper_bound: Option<f64>,
}

fn timing_rows(
    snapshot: &BackendMetricsSnapshot,
    metric_name: &str,
    primary_label: &str,
) -> Vec<TimingRow> {
    let mut rows = snapshot
        .series
        .iter()
        .filter(|series| series.name == metric_name)
        .filter_map(|series| {
            let MetricValueView::Histogram {
                count,
                sum,
                max,
                buckets,
                ..
            } = &series.value
            else {
                return None;
            };
            let name = label_value(&series.labels, primary_label)
                .unwrap_or("unlabeled")
                .to_owned();
            let labels = series
                .labels
                .iter()
                .filter(|label| label.key != primary_label)
                .map(|label| format!("{}={}", label.key, label.value))
                .collect::<Vec<_>>()
                .join(", ");
            let p95_target = count.saturating_mul(95).div_ceil(100);
            let p95_upper_bound = buckets
                .iter()
                .find(|bucket| bucket.count >= p95_target)
                .and_then(|bucket| bucket.upper_bound)
                .or(*max);
            Some(TimingRow {
                name,
                labels,
                count: *count,
                sum: *sum,
                average: if *count == 0 {
                    0.0
                } else {
                    *sum / *count as f64
                },
                maximum: *max,
                p95_upper_bound,
            })
        })
        .collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .sum
            .total_cmp(&left.sum)
            .then_with(|| left.name.cmp(&right.name))
            .then_with(|| left.labels.cmp(&right.labels))
    });
    rows
}

fn label_value<'a>(labels: &'a [MetricLabelView], key: &str) -> Option<&'a str> {
    labels
        .iter()
        .find(|label| label.key == key)
        .map(|label| label.value.as_str())
}

#[component]
fn TimingPanel(
    title: &'static str,
    description: &'static str,
    empty_message: &'static str,
    rows: Vec<TimingRow>,
) -> impl IntoView {
    view! {
        <section class="panel metrics-panel">
            <div class="panel-heading">
                <div>
                    <h2>{title}</h2>
                    <p class="muted">{description}</p>
                </div>
            </div>
            {if rows.is_empty() {
                view! { <p class="muted">{empty_message}</p> }.into_any()
            } else {
                view! {
                    <div class="metrics-table-scroll">
                        <table class="metrics-table">
                            <thead>
                                <tr>
                                    <th>"Operation"</th>
                                    <th>"Labels"</th>
                                    <th class="numeric">"Calls"</th>
                                    <th class="numeric">"Total"</th>
                                    <th class="numeric">"Average"</th>
                                    <th class="numeric">"p95 ≤"</th>
                                    <th class="numeric">"Maximum"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {rows.into_iter().map(|row| view! {
                                    <tr>
                                        <td><code>{row.name}</code></td>
                                        <td class="muted">{row.labels}</td>
                                        <td class="numeric">{row.count}</td>
                                        <td class="numeric">{format_duration(row.sum)}</td>
                                        <td class="numeric">{format_duration(row.average)}</td>
                                        <td class="numeric">{format_optional_duration(row.p95_upper_bound)}</td>
                                        <td class="numeric">{format_optional_duration(row.maximum)}</td>
                                    </tr>
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
                .into_any()
            }}
        </section>
    }
}

#[component]
fn MetricSeriesPanel(series: Vec<MetricSeriesView>) -> impl IntoView {
    view! {
        <section class="panel metrics-panel">
            <div class="panel-heading">
                <div>
                    <h2>"Counters and gauges"</h2>
                    <p class="muted">"Supporting measurements emitted through the same metrics recorder."</p>
                </div>
            </div>
            {if series.is_empty() {
                view! { <p class="muted">"No counters or gauges have been recorded yet."</p> }
                    .into_any()
            } else {
                view! {
                    <div class="metrics-table-scroll">
                        <table class="metrics-table">
                            <thead>
                                <tr>
                                    <th>"Metric"</th>
                                    <th>"Labels"</th>
                                    <th>"Description"</th>
                                    <th class="numeric">"Value"</th>
                                </tr>
                            </thead>
                            <tbody>
                                {series.into_iter().map(|series| {
                                    let labels = series.labels.iter()
                                        .map(|label| format!("{}={}", label.key, label.value))
                                        .collect::<Vec<_>>()
                                        .join(", ");
                                    let value = match series.value {
                                        MetricValueView::Counter { value } => value.to_string(),
                                        MetricValueView::Gauge { value } => format!("{value:.3}"),
                                        MetricValueView::Histogram { count, .. } => format!("{count} samples"),
                                    };
                                    view! {
                                        <tr>
                                            <td><code>{series.name}</code></td>
                                            <td class="muted">{labels}</td>
                                            <td class="muted">{series.description.unwrap_or_default()}</td>
                                            <td class="numeric">{value}</td>
                                        </tr>
                                    }
                                }).collect_view()}
                            </tbody>
                        </table>
                    </div>
                }
                .into_any()
            }}
        </section>
    }
}

fn format_optional_duration(seconds: Option<f64>) -> String {
    seconds
        .map(format_duration)
        .unwrap_or_else(|| "—".to_owned())
}

fn format_duration(seconds: f64) -> String {
    if seconds < 0.001 {
        format!("{:.0} µs", seconds * 1_000_000.0)
    } else if seconds < 1.0 {
        format!("{:.2} ms", seconds * 1_000.0)
    } else {
        format!("{seconds:.2} s")
    }
}

#[cfg(test)]
mod tests {
    use assertr::prelude::*;
    use dispatch_types::MetricBucketView;

    use super::*;

    #[test]
    fn timing_rows_sort_by_total_time_and_calculate_summary_values() {
        let snapshot = BackendMetricsSnapshot {
            captured_at: String::new(),
            series: vec![MetricSeriesView {
                name: SQL_DURATION_METRIC.to_owned(),
                description: None,
                unit: Some("seconds".to_owned()),
                labels: vec![MetricLabelView {
                    key: "statement".to_owned(),
                    value: "SELECT work_items".to_owned(),
                }],
                value: MetricValueView::Histogram {
                    count: 2,
                    sum: 0.03,
                    min: Some(0.01),
                    max: Some(0.02),
                    buckets: vec![
                        MetricBucketView {
                            upper_bound: Some(0.01),
                            count: 1,
                        },
                        MetricBucketView {
                            upper_bound: Some(0.025),
                            count: 2,
                        },
                    ],
                },
            }],
        };

        let rows = timing_rows(&snapshot, SQL_DURATION_METRIC, "statement");
        assert_that!(&(rows.len())).is_equal_to(1);
        assert_that!(&(rows[0].count)).is_equal_to(2);
        assert_that!(&(rows[0].average)).is_equal_to(0.015);
        assert_that!(&(rows[0].p95_upper_bound)).is_equal_to(Some(0.025));
    }

    #[test]
    fn duration_format_uses_readable_units() {
        assert_that!(&(format_duration(0.000_2))).is_equal_to("200 µs");
        assert_that!(&(format_duration(0.125))).is_equal_to("125.00 ms");
        assert_that!(&(format_duration(2.5))).is_equal_to("2.50 s");
    }
}
