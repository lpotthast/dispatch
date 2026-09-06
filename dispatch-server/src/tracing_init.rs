use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{
    Layer, Registry,
    filter::{LevelFilter, ParseError, Targets, filter_fn},
    fmt::format::{DefaultFields, Format, Full},
    prelude::__tracing_subscriber_SubscriberExt,
};

use dispatch_types::{
    SQL_DURATION_METRIC, SQL_QUERY_COUNT_METRIC, SQL_ROWS_AFFECTED_METRIC, SQL_ROWS_RETURNED_METRIC,
};

type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;
type StdoutWriter = fn() -> std::io::Stdout;

const DISPATCH_LOG_ENV: &str = "DISPATCH_LOG";
const DISPATCH_SQLX_LOG_ENV: &str = "DISPATCH_SQLX_LOG";
const SQLX_TARGET: &str = "sqlx";

#[derive(Default, Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum FmtLayerMode {
    Standard,
    #[default]
    Pretty,
    Json,
}

#[derive(Debug, Clone, Copy)]
pub struct TracingConfig {
    pub with_target: bool,
    pub with_file: bool,
    pub with_line_number: bool,
    pub with_ansi_coloring: bool,
    pub with_thread_name: bool,
    pub with_thread_id: bool,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            with_target: true,
            with_file: true,
            with_line_number: true,
            with_ansi_coloring: true,
            with_thread_name: false,
            with_thread_id: false,
        }
    }
}

impl TracingConfig {
    fn into_fmt_layer(
        self,
    ) -> tracing_subscriber::fmt::Layer<Registry, DefaultFields, Format<Full>, StdoutWriter> {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stdout as StdoutWriter)
            .with_target(self.with_target)
            .with_file(self.with_file)
            .with_line_number(self.with_line_number)
            .with_ansi(self.with_ansi_coloring)
            .with_thread_names(self.with_thread_name)
            .with_thread_ids(self.with_thread_id)
    }
}

fn default_fmt_filter(default_log_level: LevelFilter) -> Targets {
    Targets::new()
        .with_default(default_log_level)
        .with_target("tokio", LevelFilter::WARN)
        .with_target("runtime", LevelFilter::WARN)
        .with_target(SQLX_TARGET, LevelFilter::WARN)
}

fn parse_fmt_filter(value: &str) -> Result<Targets, ParseError> {
    value.trim().parse()
}

fn build_fmt_filter(
    default_log_level: LevelFilter,
    configured_filter: Option<&str>,
    sqlx_log_level: Option<LevelFilter>,
) -> Result<Targets, ParseError> {
    let mut filter = match configured_filter
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(value) => parse_fmt_filter(value)?,
        None => default_fmt_filter(default_log_level),
    };

    if let Some(level) = sqlx_log_level {
        filter = filter.with_target(SQLX_TARGET, level);
    }

    Ok(filter)
}

fn read_env_var(name: &str) -> Option<String> {
    match std::env::var(name) {
        Ok(value) if value.trim().is_empty() => None,
        Ok(value) => Some(value),
        Err(std::env::VarError::NotPresent) => None,
        Err(err) => {
            eprintln!("Ignoring unreadable {name}: {err}");
            None
        }
    }
}

fn read_sqlx_log_level() -> Option<LevelFilter> {
    let value = read_env_var(DISPATCH_SQLX_LOG_ENV)?;
    match value.trim().parse::<LevelFilter>() {
        Ok(level) => Some(level),
        Err(err) => {
            eprintln!("Ignoring invalid {DISPATCH_SQLX_LOG_ENV} value {value:?}: {err}");
            None
        }
    }
}

fn build_fmt_filter_from_env(default_log_level: LevelFilter) -> Targets {
    let configured_filter = read_env_var(DISPATCH_LOG_ENV);
    let sqlx_log_level = read_sqlx_log_level();

    match build_fmt_filter(
        default_log_level,
        configured_filter.as_deref(),
        sqlx_log_level,
    ) {
        Ok(filter) => filter,
        Err(err) => {
            if let Some(value) = configured_filter {
                eprintln!("Ignoring invalid {DISPATCH_LOG_ENV} value {value:?}: {err}");
            }

            build_fmt_filter(default_log_level, None, sqlx_log_level)
                .expect("default tracing filter must be valid")
        }
    }
}

fn build_fmt_layer(mode: FmtLayerMode, config: TracingConfig) -> BoxedLayer {
    let fmt_layer = config.into_fmt_layer();
    match mode {
        FmtLayerMode::Standard => Box::new(fmt_layer),
        FmtLayerMode::Pretty => Box::new(fmt_layer.pretty()),
        FmtLayerMode::Json => Box::new(fmt_layer.json()),
    }
}

pub fn init() {
    let fmt_filter = build_fmt_filter_from_env(LevelFilter::INFO);
    let fmt_layer = build_fmt_layer(FmtLayerMode::Pretty, Default::default());
    let fmt_layer_filtered = fmt_layer.with_filter(fmt_filter);
    let sql_metrics =
        SqlMetricsLayer.with_filter(filter_fn(|metadata| metadata.target() == "sqlx::query"));

    Registry::default()
        .with(fmt_layer_filtered)
        .with(sql_metrics)
        .init();
}

#[derive(Clone, Copy, Debug)]
struct SqlMetricsLayer;

impl<S> Layer<S> for SqlMetricsLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _context: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let mut fields = SqlEventFields::default();
        event.record(&mut fields);
        let Some(elapsed_seconds) = fields.elapsed_seconds else {
            return;
        };
        let statement = fields
            .summary
            .as_deref()
            .map(normalize_statement_summary)
            .filter(|summary| !summary.is_empty())
            .unwrap_or_else(|| "unknown".to_owned());

        metrics::histogram!(SQL_DURATION_METRIC, "statement" => statement.clone())
            .record(elapsed_seconds);
        metrics::counter!(SQL_QUERY_COUNT_METRIC, "statement" => statement.clone()).increment(1);
        if let Some(rows_returned) = fields.rows_returned {
            metrics::counter!(SQL_ROWS_RETURNED_METRIC, "statement" => statement.clone())
                .increment(rows_returned);
        }
        if let Some(rows_affected) = fields.rows_affected {
            metrics::counter!(SQL_ROWS_AFFECTED_METRIC, "statement" => statement)
                .increment(rows_affected);
        }
    }
}

#[derive(Default)]
struct SqlEventFields {
    summary: Option<String>,
    elapsed_seconds: Option<f64>,
    rows_returned: Option<u64>,
    rows_affected: Option<u64>,
}

impl tracing::field::Visit for SqlEventFields {
    fn record_f64(&mut self, field: &tracing::field::Field, value: f64) {
        if field.name() == "elapsed_secs" {
            self.elapsed_seconds = Some(value);
        }
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        match field.name() {
            "rows_returned" => self.rows_returned = Some(value),
            "rows_affected" => self.rows_affected = Some(value),
            _ => {}
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "summary" {
            self.summary = Some(value.to_owned());
        }
    }

    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "summary" && self.summary.is_none() {
            self.summary = Some(format!("{value:?}").trim_matches('"').to_owned());
        }
    }
}

fn normalize_statement_summary(summary: &str) -> String {
    const MAX_SUMMARY_LENGTH: usize = 120;

    let normalized = summary.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.chars().count() <= MAX_SUMMARY_LENGTH {
        return normalized;
    }
    normalized
        .chars()
        .take(MAX_SUMMARY_LENGTH.saturating_sub(1))
        .chain(std::iter::once('…'))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use assertr::prelude::*;
    use tracing::Level;

    #[test]
    fn default_filter_hides_sqlx_info() {
        let filter = default_fmt_filter(LevelFilter::INFO);

        assert_that!(&(filter.would_enable("dispatch_server", &Level::INFO))).is_true();
        assert_that!(&(!filter.would_enable("sqlx::query", &Level::INFO))).is_true();
        assert_that!(&(filter.would_enable("sqlx::query", &Level::WARN))).is_true();
    }

    #[test]
    fn dispatch_log_filter_can_enable_sqlx_info() {
        let filter = build_fmt_filter(LevelFilter::INFO, Some("info,sqlx=info"), None).unwrap();

        assert_that!(&(filter.would_enable("sqlx::query", &Level::INFO))).is_true();
    }

    #[test]
    fn sqlx_log_level_can_override_default_target() {
        let filter = build_fmt_filter(LevelFilter::INFO, None, Some(LevelFilter::DEBUG)).unwrap();

        assert_that!(&(filter.would_enable("sqlx::query", &Level::DEBUG))).is_true();
        assert_that!(&(!filter.would_enable("sqlx::query", &Level::TRACE))).is_true();
    }

    #[test]
    fn configured_filter_replaces_default_filter() {
        let filter = build_fmt_filter(LevelFilter::INFO, Some("off"), None).unwrap();

        assert_that!(&(!filter.would_enable("dispatch_server", &Level::ERROR))).is_true();
        assert_that!(&(!filter.would_enable("sqlx::query", &Level::WARN))).is_true();
    }

    #[test]
    fn sql_statement_summaries_are_whitespace_normalized_and_bounded() {
        assert_that!(&(normalize_statement_summary("SELECT  *\n FROM work_items")))
            .is_equal_to("SELECT * FROM work_items");
        let long = "x".repeat(200);
        let normalized = normalize_statement_summary(&long);
        assert_that!(&(normalized.chars().count())).is_equal_to(120);
        assert_that!(&(normalized.ends_with('…'))).is_true();
    }
}
