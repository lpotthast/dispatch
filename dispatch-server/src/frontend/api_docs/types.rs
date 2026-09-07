use dispatch_types::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct ApiDocsSelection {
    pub(crate) selected_project: Option<String>,
}
impl From<ApiDocsPage> for ApiDocsSelection {
    fn from(value: ApiDocsPage) -> Self {
        Self {
            selected_project: value.selected_project,
        }
    }
}
