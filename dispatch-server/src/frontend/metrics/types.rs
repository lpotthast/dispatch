use dispatch_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct MetricsSnapshot {
    pub(crate) selected_project: Option<String>,
    pub(crate) metrics: BackendMetricsSnapshot,
}
impl From<MetricsPageData> for MetricsSnapshot {
    fn from(value: MetricsPageData) -> Self {
        Self {
            selected_project: value.selected_project,
            metrics: value.metrics,
        }
    }
}
