use super::{
    model::{AttributionInput, RequestAttribution},
    service::AttributionService,
};
use axum::http::HeaderMap;
use rootcause::{Result, prelude::*};

pub(crate) const AGENT_ID_HEADER: &str = "x-dispatch-agent-id";
pub(crate) const AGENT_RUN_ID_HEADER: &str = "x-dispatch-agent-run-id";

pub(crate) fn parse(headers: &HeaderMap) -> Result<AttributionInput> {
    let agent_id = header_value(headers, AGENT_ID_HEADER)?;
    let agent_run_id = header_value(headers, AGENT_RUN_ID_HEADER)?
        .map(|value| {
            value
                .parse::<i64>()
                .context_with(|| format!("invalid {AGENT_RUN_ID_HEADER} '{value}'"))
        })
        .transpose()?;
    Ok(AttributionInput {
        agent_id,
        agent_run_id,
    })
}

pub(crate) async fn from_headers(
    service: &AttributionService,
    project: &str,
    headers: &HeaderMap,
) -> Result<RequestAttribution> {
    service.validate(project, parse(headers)?).await
}

pub(crate) async fn from_knowledge_headers(
    service: &AttributionService,
    project: &str,
    headers: &HeaderMap,
) -> Result<RequestAttribution> {
    service.validate_knowledge(project, parse(headers)?).await
}

fn header_value(headers: &HeaderMap, name: &str) -> Result<Option<String>> {
    headers
        .get(name)
        .map(|value| {
            let value = value
                .to_str()
                .context_with(|| format!("invalid {name} header"))?
                .trim();
            if value.is_empty() {
                bail!("{name} header cannot be empty");
            }
            Ok(value.to_owned())
        })
        .transpose()
}
