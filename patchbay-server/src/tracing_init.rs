use tracing::Level;
use tracing_subscriber::util::SubscriberInitExt;
use tracing_subscriber::{
    Layer, Registry,
    filter::Targets,
    fmt::format::{DefaultFields, Format, Full},
    prelude::__tracing_subscriber_SubscriberExt,
};

type BoxedLayer = Box<dyn Layer<Registry> + Send + Sync + 'static>;
type StderrWriter = fn() -> std::io::Stderr;

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
    ) -> tracing_subscriber::fmt::Layer<Registry, DefaultFields, Format<Full>, StderrWriter> {
        tracing_subscriber::fmt::layer()
            .with_writer(std::io::stderr as StderrWriter)
            .with_target(self.with_target)
            .with_file(self.with_file)
            .with_line_number(self.with_line_number)
            .with_ansi(self.with_ansi_coloring)
            .with_thread_names(self.with_thread_name)
            .with_thread_ids(self.with_thread_id)
    }
}

fn build_fmt_filter(default_log_level: tracing::level_filters::LevelFilter) -> Targets {
    Targets::new()
        .with_default(default_log_level)
        .with_target("tokio", Level::WARN)
        .with_target("runtime", Level::WARN)
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
    let fmt_filter = build_fmt_filter(tracing::level_filters::LevelFilter::INFO);
    let fmt_layer = build_fmt_layer(FmtLayerMode::Pretty, Default::default());
    let fmt_layer_filtered = fmt_layer.with_filter(fmt_filter);

    Registry::default().with(fmt_layer_filtered).init();
}
